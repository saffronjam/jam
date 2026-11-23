use super::{Profile, Provider};
use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

pub(super) struct Process {
    pid: u32,
    parent: u32,
    provider: Option<Provider>,
    wrapper: bool,
}

fn provider(executable: &str, arguments: &str) -> Option<Provider> {
    for candidate in [Some(executable), arguments.split_whitespace().next()]
        .into_iter()
        .flatten()
    {
        match Path::new(candidate).file_name()?.to_str()? {
            "codex" => return Some(Provider::Codex),
            "claude" => return Some(Provider::Claude),
            "node" | "bun" if arguments.contains("/@anthropic-ai/claude-code/") => {
                return Some(Provider::Claude);
            }
            "node" | "bun" if arguments.contains("/@openai/codex/") => {
                return Some(Provider::Codex);
            }
            _ => (),
        }
    }
    None
}

fn owned(pid: u32, roots: &HashSet<u32>, processes: &HashMap<u32, u32>) -> bool {
    let mut current = pid;
    for _ in 0..128 {
        if roots.contains(&current) {
            return true;
        }
        let Some(parent) = processes.get(&current) else {
            return false;
        };
        if *parent == current || *parent <= 1 {
            return false;
        }
        current = *parent;
    }
    false
}

pub(super) fn discover(roots: &[u32]) -> Vec<(Profile, u32)> {
    if roots.is_empty() {
        return Vec::new();
    }
    let roots: HashSet<_> = roots.iter().copied().filter(|id| *id > 1).collect();
    let Ok(processes) = processes() else {
        return Vec::new();
    };
    let parents = processes.iter().map(|p| (p.pid, p.parent)).collect();
    let wrappers: Vec<_> = processes
        .iter()
        .filter(|p| p.wrapper)
        .map(|p| (p.pid, p.provider))
        .collect();
    let mut profiles: HashMap<String, (Profile, u32)> = HashMap::new();
    for process in processes {
        let Some(provider) = process.provider else {
            continue;
        };
        if !owned(process.pid, &roots, &parents) {
            continue;
        }
        if owned(
            process.parent,
            &wrappers
                .iter()
                .filter(|(_, p)| *p == Some(provider))
                .map(|(pid, _)| *pid)
                .collect(),
            &parents,
        ) {
            continue;
        }
        let Ok(environment) = environment(process.pid) else {
            continue;
        };
        let Some(profile) = profile(provider, &environment) else {
            continue;
        };
        profiles
            .entry(profile.key.clone())
            .and_modify(|(_, count)| *count += 1)
            .or_insert((profile, 1));
        if profiles.len() >= 64 {
            break;
        }
    }
    profiles.into_values().collect()
}

/// Each daemon can also report its configured login while no Jam tab uses it.
/// Only this process's provider configuration directories are candidates.
pub(super) fn configured_profiles() -> Vec<Profile> {
    let environment = [
        "HOME",
        "CODEX_HOME",
        "CLAUDE_CONFIG_DIR",
        "CLAUDE_SECURESTORAGE_CONFIG_DIR",
        "CLAUDE_CODE_OAUTH_TOKEN",
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_AUTH_TOKEN",
    ]
    .into_iter()
    .filter_map(|key| std::env::var(key).ok().map(|v| (key.to_owned(), v)))
    .collect();
    [Provider::Codex, Provider::Claude]
        .into_iter()
        .filter_map(|provider| {
            let profile = profile(provider, &environment)?;
            let configured = match provider {
                Provider::Codex => profile.directory.join("auth.json").is_file(),
                Provider::Claude => {
                    !profile.api_key
                        && (profile.token.is_some()
                            || profile.directory.join(".credentials.json").is_file()
                            || (cfg!(target_os = "macos") && profile.directory.is_dir()))
                }
            };
            configured.then_some(profile)
        })
        .collect()
}

fn profile(provider: Provider, env: &HashMap<String, String>) -> Option<Profile> {
    let home = PathBuf::from(env.get("HOME")?);
    if !home.is_absolute() {
        return None;
    }
    let directory = match provider {
        Provider::Codex => env
            .get("CODEX_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".codex")),
        Provider::Claude => env
            .get("CLAUDE_SECURESTORAGE_CONFIG_DIR")
            .or_else(|| env.get("CLAUDE_CONFIG_DIR"))
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".claude")),
    };
    if !directory.is_absolute() {
        return None;
    }
    let token = if provider == Provider::Claude {
        env.get("CLAUDE_CODE_OAUTH_TOKEN").cloned()
    } else {
        None
    };
    let api_key = match provider {
        Provider::Claude => {
            env.contains_key("ANTHROPIC_API_KEY") || env.contains_key("ANTHROPIC_AUTH_TOKEN")
        }
        Provider::Codex => false, // Codex's auth_mode decides whether an exported API key is in use.
    };
    Some(Profile::new(provider, directory, home, token, api_key))
}

fn keep_environment(bytes: &[u8]) -> HashMap<String, String> {
    bytes
        .split(|c| *c == 0)
        .filter_map(|entry| {
            let (key, value) = std::str::from_utf8(entry).ok()?.split_once('=')?;
            if [
                "HOME",
                "CODEX_HOME",
                "CLAUDE_CONFIG_DIR",
                "CLAUDE_SECURESTORAGE_CONFIG_DIR",
                "CLAUDE_CODE_OAUTH_TOKEN",
                "ANTHROPIC_API_KEY",
                "ANTHROPIC_AUTH_TOKEN",
            ]
            .contains(&key)
                && !value.is_empty()
            {
                Some((key.to_owned(), value.to_owned()))
            } else {
                None
            }
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn processes() -> std::io::Result<Vec<Process>> {
    let mut result = Vec::new();
    for entry in std::fs::read_dir("/proc")?.flatten() {
        let Ok(pid) = entry.file_name().to_string_lossy().parse() else {
            continue;
        };
        let Ok(stat) = std::fs::read_to_string(entry.path().join("stat")) else {
            continue;
        };
        let Some((_, rest)) = stat.rsplit_once(") ") else {
            continue;
        };
        let Some(parent) = rest.split_whitespace().nth(1).and_then(|p| p.parse().ok()) else {
            continue;
        };
        let Ok(cmdline) = std::fs::read(entry.path().join("cmdline")) else {
            continue;
        };
        let args = String::from_utf8_lossy(&cmdline).replace('\0', " ");
        let executable = cmdline.split(|b| *b == 0).next().unwrap_or_default();
        result.push(Process {
            pid,
            parent,
            provider: provider(&String::from_utf8_lossy(executable), &args),
            wrapper: matches!(
                Path::new(String::from_utf8_lossy(executable).as_ref())
                    .file_name()
                    .and_then(|s| s.to_str()),
                Some("node" | "bun")
            ),
        });
    }
    Ok(result)
}

#[cfg(target_os = "linux")]
fn environment(pid: u32) -> std::io::Result<HashMap<String, String>> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(format!("/proc/{pid}/environ"))?
        .take(2 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    Ok(keep_environment(&bytes))
}

#[cfg(target_os = "macos")]
fn processes() -> std::io::Result<Vec<Process>> {
    let output = std::process::Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,comm=,args="])
        .output()?;
    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let pid = fields.next()?.parse().ok()?;
            let parent = fields.next()?.parse().ok()?;
            let executable = fields.next()?;
            let args = fields.collect::<Vec<_>>().join(" ");
            Some(Process {
                pid,
                parent,
                provider: provider(executable, &args),
                wrapper: matches!(
                    Path::new(executable).file_name().and_then(|s| s.to_str()),
                    Some("node" | "bun")
                ),
            })
        })
        .collect())
}

#[cfg(target_os = "macos")]
fn environment(pid: u32) -> std::io::Result<HashMap<String, String>> {
    let mut mib = [libc::CTL_KERN, libc::KERN_PROCARGS2, pid as i32];
    let mut size: usize = 0;
    unsafe {
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    if !(4..=2 * 1024 * 1024).contains(&size) {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut bytes = vec![0u8; size];
    unsafe {
        if libc::sysctl(
            mib.as_mut_ptr(),
            3,
            bytes.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return Err(std::io::Error::last_os_error());
        }
    }
    bytes.truncate(size);
    let argc = i32::from_ne_bytes(bytes[..4].try_into().unwrap());
    if !(0..=65536).contains(&argc) {
        return Err(std::io::ErrorKind::InvalidData.into());
    }
    let mut cursor = 4;
    while cursor < bytes.len() && bytes[cursor] != 0 {
        cursor += 1;
    }
    while cursor < bytes.len() && bytes[cursor] == 0 {
        cursor += 1;
    }
    for _ in 0..argc {
        while cursor < bytes.len() && bytes[cursor] != 0 {
            cursor += 1;
        }
        cursor += 1;
    }
    Ok(keep_environment(bytes.get(cursor..).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_owned_agents_and_known_executables_are_detected() {
        let parents = HashMap::from([(20, 10), (30, 20), (40, 1)]);
        assert!(owned(30, &HashSet::from([10]), &parents));
        assert!(!owned(40, &HashSet::from([10]), &parents));
        assert_eq!(
            provider("/bin/node", "node /x/@anthropic-ai/claude-code/cli.js"),
            Some(Provider::Claude)
        );
        assert_eq!(provider("bash", "echo claude"), None);
        assert!(discover(&[]).is_empty());
    }
    #[test]
    fn profiles_respect_overrides_without_retaining_unrelated_environment() {
        let env = keep_environment(b"HOME=/home/a\0CODEX_HOME=/profiles/work\0SECRET=never\0");
        let p = profile(Provider::Codex, &env).unwrap();
        assert_eq!(p.directory, PathBuf::from("/profiles/work"));
        assert!(!env.contains_key("SECRET"));
        let other = profile(
            Provider::Codex,
            &keep_environment(b"HOME=/home/a\0CODEX_HOME=/profiles/personal\0"),
        )
        .unwrap();
        assert_ne!(p.key, other.key);
    }

    #[test]
    fn discovers_a_real_child_process_and_its_profile_without_reading_credentials() {
        struct Child(std::process::Child);
        impl Drop for Child {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let directory = tempfile::tempdir().unwrap();
        use std::os::unix::process::CommandExt;
        let _child = Child(
            std::process::Command::new(std::env::current_exe().unwrap())
                .arg0("codex")
                .args([
                    "--ignored",
                    "--exact",
                    "usage::process::tests::child_fixture",
                ])
                .env("JAM_USAGE_FIXTURE", "1")
                .env("CODEX_HOME", directory.path())
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            let profiles = discover(&[std::process::id()]);
            if profiles.iter().any(|(p, count)| {
                p.directory == directory.path() && p.provider == Provider::Codex && *count == 1
            }) {
                break;
            }
            if std::time::Instant::now() >= deadline {
                let child = processes()
                    .unwrap()
                    .into_iter()
                    .find(|p| p.pid == _child.0.id());
                let metadata = child.map(|p| (p.pid, p.parent, p.provider, p.wrapper));
                let keys =
                    environment(_child.0.id()).map(|env| env.into_keys().collect::<Vec<_>>());
                panic!("Did not discover child: {metadata:?}; env keys: {keys:?}");
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
    }
    #[test]
    #[ignore = "launched only as a controlled child by the process discovery test"]
    fn child_fixture() {
        if std::env::var("JAM_USAGE_FIXTURE").as_deref() == Ok("1") {
            std::thread::sleep(std::time::Duration::from_secs(10));
        }
    }
}
