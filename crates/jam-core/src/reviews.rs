//! Read-only review discovery. Network requests never run on a terminal or UI path.
use anyhow::{Context, Result, bail};
use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashMap,
    io::Read,
    os::unix::process::CommandExt,
    path::Path,
    process::{Command, Stdio},
    sync::{Arc, LazyLock, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Github,
    Gitlab,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct Review {
    pub provider: Provider,
    pub number: u64,
    pub url: String,
    pub title: String,
    pub draft: bool,
}

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
struct Query {
    provider: String,
    host: String,
    project: String,
    branch: String,
}

fn encoded(value: &str) -> String {
    utf8_percent_encode(value, NON_ALPHANUMERIC).to_string()
}

fn remote(url: &str, provider: Option<&str>, host_override: Option<&str>) -> Option<Query> {
    let (authority, path) = if let Some((scheme, rest)) = url.split_once("://") {
        if !["https", "http", "ssh", "git"].contains(&scheme) {
            return None;
        }
        rest.split_once('/')?
    } else {
        url.split_once(':')?
    };
    let host = host_override
        .unwrap_or(authority.rsplit('@').next()?)
        .to_lowercase();
    if host.is_empty()
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b".-:".contains(&c))
    {
        return None;
    }
    let project = path
        .trim_matches('/')
        .strip_suffix(".git")
        .unwrap_or(path.trim_matches('/'));
    if project.split('/').count() < 2
        || project
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
    {
        return None;
    }
    let provider = match provider {
        Some("github") => "github",
        Some("gitlab") => "gitlab",
        Some(_) => return None,
        None if host == "github.com" => "github",
        None if host == "gitlab.com" => "gitlab",
        None => return None,
    };
    if provider == "github" && project.split('/').count() != 2 {
        return None;
    }
    Some(Query {
        provider: provider.into(),
        host,
        project: project.into(),
        branch: String::new(),
    })
}

/// Bound process lifetime and output, including any credential helper children.
fn output(mut command: Command) -> Result<Vec<u8>> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    unsafe {
        command.pre_exec(|| {
            if libc::setpgid(0, 0) == 0 {
                Ok(())
            } else {
                Err(std::io::Error::last_os_error())
            }
        });
    }
    let mut child = command.spawn()?;
    let stdout = child.stdout.take().context("missing stdout")?;
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .map(|_| bytes)
    });
    let deadline = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            unsafe {
                libc::kill(-(child.id() as i32), libc::SIGKILL);
            }
            let _ = child.wait();
            let _ = reader.join();
            bail!("review lookup timed out");
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let bytes = reader
        .join()
        .map_err(|_| anyhow::anyhow!("review reader failed"))??;
    if !status.success() || bytes.len() > 1024 * 1024 {
        bail!("review lookup unavailable");
    }
    Ok(bytes)
}

fn git(path: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0");
    String::from_utf8(output(command).ok()?)
        .ok()
        .map(|s| s.trim().to_owned())
}

fn identify(path: &Path) -> Option<Query> {
    let branch = git(path, &["symbolic-ref", "--quiet", "--short", "HEAD"])?;
    let config = git(
        path,
        &[
            "config",
            "--null",
            "--get-regexp",
            r"^(branch\.|remote\.|jam\.)",
        ],
    )?;
    let values: HashMap<&str, &str> = config
        .split('\0')
        .filter_map(|entry| entry.split_once('\n'))
        .collect();
    let upstream = values
        .get(format!("branch.{branch}.remote").as_str())
        .copied();
    let remote_name = values
        .get(format!("branch.{branch}.pushremote").as_str())
        .copied()
        .or_else(|| values.get("remote.pushdefault").copied())
        .or(upstream)
        .filter(|r| *r != ".")
        .or_else(|| {
            let names: Vec<_> = values
                .keys()
                .filter_map(|key| key.strip_prefix("remote.")?.strip_suffix(".url"))
                .collect();
            if names.contains(&"origin") {
                Some("origin")
            } else if names.len() == 1 {
                Some(names[0])
            } else {
                None
            }
        })?;
    if remote_name.starts_with('-') {
        return None;
    }
    let url = git(path, &["remote", "get-url", "--push", remote_name])?;
    let mut query = remote(
        &url,
        values.get("jam.reviewprovider").copied(),
        values.get("jam.reviewhost").copied(),
    )?;
    query.branch = if upstream == Some(remote_name) {
        values
            .get(format!("branch.{branch}.merge").as_str())
            .and_then(|r| r.strip_prefix("refs/heads/"))
            .unwrap_or(&branch)
            .to_owned()
    } else {
        branch
    };
    Some(query)
}

fn api(query: &Query, route: &str) -> Result<Value> {
    let tool = if query.provider == "github" {
        "gh"
    } else {
        "glab"
    };
    // GUI launches and systemd user services often have a shorter PATH than a shell.
    let executable = ["/opt/homebrew/bin", "/usr/local/bin"].map(|dir| Path::new(dir).join(tool));
    let mut command = Command::new(tool);
    if !std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
        .any(|dir| dir.join(tool).is_file())
        && let Some(path) = std::env::var_os("HOME")
            .map(|home| std::path::PathBuf::from(home).join(".local/bin").join(tool))
            .into_iter()
            .chain(executable)
            .find(|path| path.is_file())
    {
        command = Command::new(path);
    }
    command
        .args(["api", "--hostname", &query.host, "--method", "GET", route])
        .env("GH_PROMPT_DISABLED", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GLAB_CHECK_UPDATE", "false")
        .env("GH_NO_UPDATE_NOTIFIER", "1");
    Ok(serde_json::from_slice(&output(command)?)?)
}

fn fetch(query: &Query, request: impl Fn(&str) -> Result<Value>) -> Result<Option<Review>> {
    let (provider, metadata_route) = if query.provider == "github" {
        (Provider::Github, format!("repos/{}", query.project))
    } else {
        (
            Provider::Gitlab,
            format!("projects/{}", encoded(&query.project)),
        )
    };
    let project = request(&metadata_route)?;
    let route = if provider == Provider::Github {
        let base = project["parent"]["full_name"]
            .as_str()
            .unwrap_or(&query.project);
        let owner = query
            .project
            .split('/')
            .next()
            .context("missing repository owner")?;
        format!(
            "repos/{base}/pulls?state=open&head={}&per_page=100&sort=updated&direction=desc",
            encoded(&format!("{owner}:{}", query.branch))
        )
    } else {
        let source = project["id"].as_u64().context("missing project ID")?;
        let base = project["forked_from_project"]["id"]
            .as_u64()
            .unwrap_or(source);
        format!(
            "projects/{base}/merge_requests?state=opened&source_branch={}&per_page=100&order_by=updated_at&sort=desc",
            encoded(&query.branch)
        )
    };
    let reviews = request(&route)?;
    let reviews = reviews.as_array().context("invalid review list")?;
    let value = reviews.iter().find(|r| {
        if provider == Provider::Github {
            r["state"] == "open"
                && r["head"]["ref"] == query.branch
                && r["head"]["repo"]["full_name"]
                    .as_str()
                    .is_some_and(|name| name.eq_ignore_ascii_case(&query.project))
        } else {
            r["state"] == "opened"
                && r["source_branch"] == query.branch
                && r["source_project_id"] == project["id"]
        }
    });
    let Some(value) = value else {
        return Ok(None);
    };
    let (number, url) = if provider == Provider::Github {
        (&value["number"], &value["html_url"])
    } else {
        (&value["iid"], &value["web_url"])
    };
    Ok(Some(Review {
        provider,
        number: number.as_u64().context("missing review number")?,
        url: url.as_str().context("missing review URL")?.into(),
        title: value["title"].as_str().unwrap_or("").into(),
        draft: value["draft"].as_bool().unwrap_or(false),
    }))
}

struct Entry {
    checked: Option<Instant>,
    touched: Instant,
    pending: bool,
    review: Option<Review>,
}
type Entries = Arc<Mutex<HashMap<Query, Entry>>>;
struct Cache {
    entries: Entries,
    jobs: crossbeam_channel::Sender<Query>,
}
impl Cache {
    fn new(fetcher: impl Fn(&Query) -> Option<Review> + Send + Sync + 'static) -> Self {
        let entries: Entries = Arc::default();
        let (jobs, queue) = crossbeam_channel::bounded::<Query>(64);
        let fetcher = Arc::new(fetcher);
        for _ in 0..2 {
            let (entries, queue, fetcher) = (entries.clone(), queue.clone(), fetcher.clone());
            std::thread::spawn(move || {
                for query in queue {
                    let review = fetcher(&query);
                    if let Some(entry) = entries.lock().unwrap().get_mut(&query) {
                        entry.review = review;
                        entry.checked = Some(Instant::now());
                        entry.pending = false;
                    }
                }
            });
        }
        Self { entries, jobs }
    }
    fn get(&self, query: Query) -> Option<Review> {
        let mut entries = self.entries.lock().unwrap();
        if !entries.contains_key(&query) && entries.len() >= 256 {
            let oldest = entries
                .iter()
                .filter(|(_, e)| !e.pending)
                .min_by_key(|(_, e)| e.touched)
                .map(|(q, _)| q.clone());
            entries.remove(&oldest?);
        }
        let now = Instant::now();
        let entry = entries.entry(query.clone()).or_insert(Entry {
            checked: None,
            touched: now,
            pending: false,
            review: None,
        });
        entry.touched = now;
        if !entry.pending
            && entry
                .checked
                .is_none_or(|at| at.elapsed() >= Duration::from_secs(60))
        {
            entry.pending = self.jobs.try_send(query).is_ok();
        }
        entry.review.clone()
    }
}
static CACHE: LazyLock<Cache> =
    LazyLock::new(|| Cache::new(|q| fetch(q, |route| api(q, route)).ok().flatten()));

pub fn lookup(path: &Path) -> Option<Review> {
    CACHE.get(identify(path)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_hosted_remotes_without_confusing_local_paths_or_ssh_aliases() {
        for value in [
            "git@github.com:owner/repo.git",
            "https://github.com/owner/repo.git",
        ] {
            let query = remote(value, None, None).unwrap();
            assert_eq!(query.host, "github.com");
            assert_eq!(query.project, "owner/repo");
        }
        assert_eq!(
            remote("ssh://git@gitlab.com/group/sub/repo.git", None, None)
                .unwrap()
                .project,
            "group/sub/repo"
        );
        assert!(remote("/local/repo", None, None).is_none());
        assert!(remote("file:///local/repo", None, None).is_none());
        assert!(remote("git@unknown.example:owner/repo.git", None, None).is_none());
        let alias = remote(
            "git@work:group/repo.git",
            Some("gitlab"),
            Some("git.example.com"),
        )
        .unwrap();
        assert_eq!(alias.host, "git.example.com");
    }

    #[test]
    fn worktree_branch_and_push_remote_determine_source_identity() {
        let temp = tempfile::tempdir().unwrap();
        let p = temp.path();
        let run = |args: &[&str]| git(p, args).unwrap();
        run(&["init", "-b", "main"]);
        run(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ]);
        run(&[
            "remote",
            "add",
            "origin",
            "https://gitlab.com/group/repo.git",
        ]);
        run(&["remote", "add", "fork", "git@gitlab.com:me/repo.git"]);
        run(&["config", "branch.main.remote", "origin"]);
        run(&["config", "branch.main.merge", "refs/heads/remote-name"]);
        assert_eq!(identify(p).unwrap().branch, "remote-name");
        run(&["config", "branch.main.pushRemote", "fork"]);
        assert_eq!(identify(p).unwrap().project, "me/repo");
        assert_eq!(identify(p).unwrap().branch, "main");
        let checkout = p.join("checkout");
        run(&[
            "worktree",
            "add",
            "-b",
            "feature/other",
            checkout.to_str().unwrap(),
        ]);
        assert_eq!(identify(&checkout).unwrap().branch, "feature/other");
        run(&["checkout", "--detach"]);
        assert!(identify(p).is_none());
    }

    #[test]
    fn github_matches_fork_owner_and_open_head_branch() {
        let mut query = remote("git@github.com:me/repo.git", None, None).unwrap();
        query.branch = "feature/ui".into();
        let result = fetch(&query, |route| {
            if route == "repos/me/repo" {
                return Ok(json!({"parent":{"full_name":"upstream/repo"}}));
            }
            assert!(route.starts_with("repos/upstream/repo/pulls?state=open&head=me%3Afeature%2Fui&"));
            Ok(json!([
                {"number":1,"state":"open","head":{"ref":"feature/ui","repo":{"full_name":"someone/repo"}}},
                {"number":2,"state":"closed","head":{"ref":"feature/ui","repo":{"full_name":"me/repo"}}},
                {"number":3,"state":"open","head":{"ref":"feature/ui","repo":{"full_name":"me/repo"}},"html_url":"https://github.com/upstream/repo/pull/3","title":"UI","draft":true}
            ]))
        }).unwrap().unwrap();
        assert_eq!(result.number, 3);
        assert!(result.draft);
        assert_eq!(result.provider, Provider::Github);
    }

    #[test]
    fn gitlab_matches_nested_project_and_ignores_other_sources_and_closed_requests() {
        let mut query = remote("https://gitlab.com/group/nested/repo.git", None, None).unwrap();
        query.branch = "feature/api".into();
        let result = fetch(&query, |route| {
            if route == "projects/group%2Fnested%2Frepo" {
                return Ok(json!({"id":12,"forked_from_project":{"id":4}}));
            }
            assert!(route.starts_with("projects/4/merge_requests?state=opened&source_branch=feature%2Fapi&"));
            Ok(json!([
                {"iid":1,"state":"opened","source_project_id":99,"source_branch":"feature/api"},
                {"iid":2,"state":"merged","source_project_id":12,"source_branch":"feature/api"},
                {"iid":3,"state":"opened","source_project_id":12,"source_branch":"feature/api","web_url":"https://gitlab.com/upstream/repo/-/merge_requests/3","title":"API","draft":false}
            ]))
        }).unwrap().unwrap();
        assert_eq!(result.number, 3);
        assert_eq!(result.provider, Provider::Gitlab);
        assert!(!result.draft);
        assert!(
            fetch(&query, |route| Ok(if route.starts_with("projects/group") {
                json!({"id":12})
            } else {
                json!([])
            }))
            .unwrap()
            .is_none()
        );
    }

    #[test]
    fn slow_network_does_not_block_cache_reads_and_branch_keys_stay_separate() {
        let (started, starting) = crossbeam_channel::bounded(1);
        let (release, wait) = crossbeam_channel::bounded(1);
        let cache = Cache::new(move |query| {
            started.send(query.branch.clone()).unwrap();
            wait.recv().unwrap();
            Some(Review {
                provider: Provider::Github,
                number: 7,
                url: "https://github.com/o/r/pull/7".into(),
                title: query.branch.clone(),
                draft: false,
            })
        });
        let mut query = remote("https://github.com/o/r", None, None).unwrap();
        query.branch = "first".into();
        assert!(cache.get(query.clone()).is_none());
        assert_eq!(
            starting.recv_timeout(Duration::from_secs(1)).unwrap(),
            "first"
        );
        assert!(cache.get(query.clone()).is_none());
        assert!(starting.try_recv().is_err());
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(1);
        while cache.get(query.clone()).is_none() {
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(cache.get(query.clone()).unwrap().title, "first");
        query.branch = "second".into();
        assert!(cache.get(query.clone()).is_none());
        assert_eq!(
            starting.recv_timeout(Duration::from_secs(1)).unwrap(),
            "second"
        );
        release.send(()).unwrap();
    }
}
