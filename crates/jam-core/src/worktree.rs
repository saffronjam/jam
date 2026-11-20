//! Git is the authority for checkout identity; Jam only owns terminal sessions.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    path::{Path, PathBuf},
    process::Command,
};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Checkout {
    pub path: String,
    pub branch: String,
    pub main: bool,
    pub locked: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Repository {
    pub common_dir: String,
    pub main_path: String,
    pub create_disabled_reason: Option<String>,
    pub checkouts: Vec<Checkout>,
}
fn git_bytes(path: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .context("run git")?;
    if !out.status.success() {
        bail!("{}", String::from_utf8_lossy(&out.stderr).trim())
    }
    Ok(out.stdout)
}
fn git(path: &Path, args: &[&str]) -> Result<String> {
    Ok(String::from_utf8(git_bytes(path, args)?)?)
}
pub fn inspect(path: &Path) -> Result<Repository> {
    let common = git(
        path,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let common = std::fs::canonicalize(common.trim())?
        .to_string_lossy()
        .into_owned();
    let raw = git(path, &["worktree", "list", "--porcelain", "-z"])?;
    let mut checkouts = Vec::new();
    let mut current: Option<Checkout> = None;
    for field in raw.split('\0') {
        if let Some(p) = field.strip_prefix("worktree ") {
            if let Some(c) = current.take() {
                checkouts.push(c)
            };
            current = Some(Checkout {
                path: p.into(),
                branch: "detached HEAD".into(),
                main: checkouts.is_empty(),
                locked: false,
            });
        } else if let Some(c) = current.as_mut() {
            if let Some(b) = field.strip_prefix("branch refs/heads/") {
                c.branch = b.into()
            } else if field.starts_with("locked") {
                c.locked = true
            } else if field == "bare" {
                bail!("Bare repositories are not supported")
            }
        }
    }
    if let Some(c) = current {
        checkouts.push(c)
    }
    let main_path = checkouts.first().context("No checkout")?.path.clone();
    Ok(Repository {
        common_dir: common,
        main_path,
        create_disabled_reason: git(path, &["rev-parse", "--verify", "HEAD^{commit}"])
            .err()
            .map(|_| "Create an initial commit before adding a worktree".into()),
        checkouts,
    })
}
pub fn identify(path: &Path) -> Option<(String, String, String)> {
    let repo = inspect(path).ok()?;
    let root = git(path, &["rev-parse", "--show-toplevel"]).ok()?;
    let c = repo.checkouts.iter().find(|c| c.path == root.trim())?;
    Some((repo.common_dir, repo.main_path, c.branch.clone()))
}
pub fn create(source: &Path, branch: &str, base: &str, path: Option<&str>) -> Result<PathBuf> {
    git(source, &["check-ref-format", "--branch", branch])?;
    if git(source, &["rev-parse", "--verify", "HEAD"]).is_err() {
        bail!("Create an initial commit before adding a worktree")
    };
    let repo = inspect(source)?;
    let target = if let Some(p) = path.filter(|p| !p.is_empty()) {
        PathBuf::from(p)
    } else {
        let name = Path::new(&repo.main_path)
            .file_name()
            .context("Repository name")?;
        crate::worktree_template::render(
            &crate::config::host_settings().worktrees_directory,
            name.to_str().context("Repository name must be UTF-8")?,
            branch,
            Path::new(&std::env::var("HOME")?),
        )?
    };
    if !target.is_absolute() {
        bail!("Checkout path must be absolute")
    }
    if target.exists() {
        bail!("Checkout path already exists; use Open Worktree")
    }
    std::fs::create_dir_all(target.parent().context("Checkout parent")?)?;
    let target_text = target.to_str().context("Checkout path must be UTF-8")?;
    let exists = Command::new("git")
        .arg("-C")
        .arg(source)
        .args([
            "show-ref",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ])
        .status()?
        .success();
    if exists {
        git(source, &["worktree", "add", "--", target_text, branch])?;
    } else {
        git(
            source,
            &[
                "worktree",
                "add",
                "-b",
                branch,
                "--",
                target_text,
                if base.is_empty() { "HEAD" } else { base },
            ],
        )?;
    }
    Ok(std::fs::canonicalize(target)?)
}
pub fn is_clean(path: &Path) -> Result<bool> {
    Ok(git(path, &["status", "--porcelain", "--untracked-files=all"])?.is_empty())
}
pub fn remove(path: &Path, force: bool) -> Result<()> {
    let path = std::fs::canonicalize(path)?;
    let repo = inspect(&path)?;
    let c = repo
        .checkouts
        .iter()
        .find(|c| Path::new(&c.path) == path)
        .context("Not a registered checkout")?;
    if c.main {
        bail!("The primary checkout cannot be deleted")
    }
    if c.locked {
        bail!("Unlock this worktree in Git before deleting it")
    }
    if !force && !git(&path, &["status", "--porcelain", "--untracked-files=all"])?.is_empty() {
        bail!("Checkout has local changes. Explicit force confirmation is required")
    }
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force")
    };
    args.push("--");
    args.push(c.path.as_str());
    git(Path::new(&repo.main_path), &args)?;
    Ok(())
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GitSummary {
    pub added: u64,
    pub removed: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review: Option<crate::reviews::Review>,
}
pub fn diff_summary(path: &Path) -> Result<GitSummary> {
    use std::{io::Read, os::unix::ffi::OsStrExt};
    let mut summary = GitSummary::default();
    let mut args = vec![
        "diff",
        "--numstat",
        "--no-ext-diff",
        "--no-textconv",
        "--no-renames",
    ];
    let has_head = git(path, &["rev-parse", "--verify", "HEAD^{commit}"]).is_ok();
    if has_head {
        args.push("HEAD");
    }
    let mut reports = vec![git(path, &args)?];
    if !has_head {
        args.push("--cached");
        reports.push(git(path, &args)?);
    }
    for report in reports {
        for line in report.lines() {
            let mut fields = line.split('\t');
            summary.added += fields
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            summary.removed += fields
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
        }
    }
    let root = git(path, &["rev-parse", "--show-toplevel"])?;
    for file in git_bytes(path, &["ls-files", "--others", "--exclude-standard", "-z"])?
        .split(|b| *b == 0)
        .filter(|s| !s.is_empty())
    {
        let file = Path::new(root.trim()).join(std::ffi::OsStr::from_bytes(file));
        let metadata = match file.symlink_metadata() {
            Ok(metadata) => metadata,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e.into()),
        };
        if !metadata.file_type().is_file() {
            continue;
        }
        let mut input = std::fs::File::open(file)?;
        let (mut lines, mut bytes, mut last, mut binary) = (0, 0, 0, false);
        let mut buffer = [0u8; 65536];
        loop {
            let n = input.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            if buffer[..n].contains(&0) {
                binary = true;
                break;
            }
            lines += buffer[..n].iter().filter(|b| **b == b'\n').count() as u64;
            bytes += n;
            last = buffer[n - 1];
        }
        if !binary {
            summary.added += lines + u64::from(bytes > 0 && last != b'\n');
        }
    }
    Ok(summary)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn lifecycle_preserves_branches_and_refuses_dirty() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path();
        git(p, &["init", "-b", "main"]).unwrap();
        assert!(inspect(p).unwrap().create_disabled_reason.is_some());
        git(
            p,
            &[
                "-c",
                "user.name=Test",
                "-c",
                "user.email=test@example.invalid",
                "commit",
                "--allow-empty",
                "-m",
                "init",
            ],
        )
        .unwrap();
        assert!(inspect(p).unwrap().create_disabled_reason.is_none());
        std::fs::write(p.join("untracked.txt"), "one\ntwo").unwrap();
        std::fs::write(p.join("binary.dat"), b"ignore\0binary").unwrap();
        assert_eq!(
            diff_summary(p).unwrap(),
            GitSummary {
                added: 2,
                removed: 0,
                review: None,
            }
        );
        git(p, &["add", "untracked.txt"]).unwrap();
        assert_eq!(diff_summary(p).unwrap().added, 2);
        std::fs::write(p.join("untracked.txt"), "one\ntwo\nthree\n").unwrap();
        assert_eq!(diff_summary(p).unwrap().added, 3);
        let target = p.join("checkout with spaces");
        create(p, "feature/test", "HEAD", target.to_str()).unwrap();
        assert_eq!(inspect(p).unwrap().checkouts.len(), 2);
        git(p, &["worktree", "lock", target.to_str().unwrap()]).unwrap();
        assert!(remove(&target, true).is_err());
        git(p, &["worktree", "unlock", target.to_str().unwrap()]).unwrap();
        std::fs::write(target.join("untracked"), "keep").unwrap();
        assert!(remove(&target, false).is_err());
        assert!(target.exists());
        remove(&target, true).unwrap();
        assert!(!target.exists());
        assert!(git(p, &["rev-parse", "--verify", "refs/heads/feature/test"]).is_ok());
        assert!(remove(p, true).is_err());
    }
}
