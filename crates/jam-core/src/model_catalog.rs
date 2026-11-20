//! Provider metadata only: discovery never submits a user message or starts inference.
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{Read, Write},
    os::unix::{io::AsRawFd, process::CommandExt},
    process::{Command, Stdio},
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Model {
    #[serde(rename = "model")]
    pub id: String,
    pub name: String,
    pub is_default: bool,
    pub efforts: Vec<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Catalog {
    pub provider: String,
    pub models: Vec<Model>,
    pub default_model: Option<String>,
}
impl Catalog {
    pub fn from_claude(value: &Value) -> Result<Self> {
        let entries = value["models"]
            .as_array()
            .context("Claude did not return a model catalog")?;
        let mut models: Vec<Model> = vec![];
        let mut default_model = None;
        for entry in entries {
            if entry["disabled"] == true {
                continue;
            }
            let alias = entry["value"]
                .as_str()
                .filter(|s| !s.is_empty())
                .context("Claude returned a model without an ID")?;
            let default = alias == "default";
            let resolved = entry["resolvedModel"].as_str().filter(|s| !s.is_empty());
            if default {
                default_model = resolved.map(str::to_owned);
            }
            let id = resolved.unwrap_or(alias);
            if default && resolved.is_none() {
                continue;
            }
            if let Some(existing) = models.iter_mut().find(|m| m.id == id) {
                existing.is_default |= default;
                continue;
            }
            models.push(Model {
                id: id.into(),
                name: entry["displayName"].as_str().unwrap_or(id).into(),
                is_default: default,
                efforts: entry["supportedEffortLevels"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect(),
            });
        }
        if models.is_empty()
            && !entries
                .iter()
                .any(|e| e["value"] == "default" && e["disabled"] != true)
        {
            bail!("Claude returned an empty model catalog")
        }
        Ok(Self {
            provider: "claude".into(),
            models,
            default_model,
        })
    }
}
struct Entry {
    fetched: Instant,
    catalog: Catalog,
}
static CACHE: LazyLock<Mutex<HashMap<String, Entry>>> = LazyLock::new(Default::default);
static FETCH: LazyLock<Mutex<()>> = LazyLock::new(Default::default);
pub fn load(provider: &str, refresh: bool) -> Result<Catalog> {
    if !["claude", "codex"].contains(&provider) {
        bail!("Unknown agent provider")
    }
    let executable = crate::coordination::provider_executable(provider)?;
    let home = std::env::var("HOME").context("Home directory unavailable")?;
    let profile = std::env::var(if provider == "claude" {
        "CLAUDE_CONFIG_DIR"
    } else {
        "CODEX_HOME"
    })
    .unwrap_or_default();
    let key = format!("{provider}:{home}:{profile}:{}", executable.display());
    let cached = || {
        CACHE
            .lock()
            .unwrap()
            .get(&key)
            .filter(|entry| entry.fetched.elapsed() < Duration::from_secs(300))
            .map(|entry| entry.catalog.clone())
    };
    if !refresh && let Some(catalog) = cached() {
        return Ok(catalog);
    }
    // Serialize metadata probes, without holding the daemon/state lock.
    let _fetch = FETCH.lock().unwrap();
    if !refresh && let Some(catalog) = cached() {
        return Ok(catalog);
    }
    let catalog = if provider == "claude" {
        claude(&executable, &home, Duration::from_secs(15))?
    } else {
        crate::codex_session::catalog(&home)?
    };
    let mut cache = CACHE.lock().unwrap();
    if cache.len() >= 8 {
        cache.clear();
    }
    cache.insert(
        key,
        Entry {
            fetched: Instant::now(),
            catalog: catalog.clone(),
        },
    );
    Ok(catalog)
}
// Nonblocking drains can always stop, even if an unrelated fork briefly inherited
// a pipe descriptor. Killing the provider must never leave discovery waiting for EOF.
fn drain(
    mut pipe: impl Read + AsRawFd,
    limit: usize,
    cancelled: Arc<AtomicBool>,
    mut chunk: impl FnMut(&[u8]) -> bool,
) {
    let fd = pipe.as_raw_fd();
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return;
        }
    }
    let mut total = 0;
    let mut buffer = [0; 4096];
    while !cancelled.load(Ordering::Relaxed) && total < limit {
        let take = buffer.len().min(limit - total);
        match pipe.read(&mut buffer[..take]) {
            Ok(0) => break,
            Ok(n) => {
                total += n;
                if !chunk(&buffer[..n]) {
                    break;
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => (),
            Err(_) => break,
        }
    }
}
fn claude(executable: &std::path::Path, home: &str, timeout: Duration) -> Result<Catalog> {
    let mut child = Command::new(executable)
        .current_dir(home)
        .process_group(0)
        .args([
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            "--no-session-persistence",
            "--tools",
            "",
            "--strict-mcp-config",
            "--mcp-config",
            "{\"mcpServers\":{}}",
            "--settings",
            "{\"disableAllHooks\":true}",
        ])
        .env_remove("JAM_AGENT_ID")
        .env_remove("JAM_PANE_ID")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Could not discover Claude models")?;
    let output = child.stdout.take().unwrap();
    let errors = child.stderr.take().unwrap();
    let cancelled = Arc::new(AtomicBool::new(false));
    let diagnostic = Arc::new(Mutex::new(Vec::new()));
    let captured = diagnostic.clone();
    let cancel_errors = cancelled.clone();
    let stderr = std::thread::spawn(move || {
        drain(errors, 64 * 1024, cancel_errors, |bytes| {
            captured.lock().unwrap().extend_from_slice(bytes);
            true
        });
    });
    let (tx, rx) = crossbeam_channel::bounded(64);
    let cancel_output = cancelled.clone();
    let reader = std::thread::spawn(move || {
        let mut pending = Vec::new();
        drain(output, 2 * 1024 * 1024, cancel_output, |bytes| {
            pending.extend_from_slice(bytes);
            while let Some(end) = pending.iter().position(|b| *b == b'\n') {
                let line = String::from_utf8_lossy(&pending[..end]).into_owned();
                pending.drain(..=end);
                if tx.try_send(line).is_err() {
                    return false;
                }
            }
            true
        });
    });
    let result = (|| -> Result<Catalog> {
        let input = child.stdin.as_mut().unwrap();
        writeln!(
            input,
            "{}",
            json!({"type":"control_request","request_id":"jam-model-catalog","request":{"subtype":"initialize"}})
        )?;
        input.flush()?;
        let deadline = Instant::now() + timeout;
        loop {
            let line = rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .context("Claude model discovery timed out or ended")?;
            let value: Value =
                serde_json::from_str(&line).context("Invalid Claude model response")?;
            if value["type"] != "control_response"
                || value["response"]["request_id"] != "jam-model-catalog"
            {
                continue;
            }
            if value["response"]["subtype"] != "success" {
                bail!(
                    "Claude model discovery failed: {}",
                    value["response"]["error"]
                        .as_str()
                        .unwrap_or("initialization rejected")
                )
            }
            return Catalog::from_claude(&value["response"]["response"]);
        }
    })();
    drop(rx);
    child.stdin.take();
    unsafe {
        libc::kill(-(child.id() as i32), libc::SIGKILL);
    }
    let _ = child.kill();
    let _ = child.wait();
    cancelled.store(true, Ordering::Relaxed);
    let _ = reader.join();
    let _ = stderr.join();
    result.with_context(|| {
        let bytes = diagnostic.lock().unwrap();
        format!(
            "Claude model discovery: {}",
            String::from_utf8_lossy(&bytes).trim()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn discovery_is_bounded_and_does_not_start_a_turn() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let executable = dir.path().join("claude");
        std::fs::write(&executable, "#!/bin/sh\nprintf '%s\\n' \"$@\" > args\nread line\nprintf '%s' \"$line\" > request\nprintf '%s\\n' '{\"type\":\"control_response\",\"response\":{\"request_id\":\"jam-model-catalog\",\"subtype\":\"success\",\"response\":{\"models\":[{\"value\":\"default\",\"resolvedModel\":\"model-test\"}]}}}'\nwhile :; do sleep 30; done\n").unwrap();
        std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
        let total = Instant::now();
        let result = claude(
            &executable,
            dir.path().to_str().unwrap(),
            Duration::from_secs(2),
        )
        .unwrap();
        assert_eq!(result.default_model.as_deref(), Some("model-test"));
        let request: Value =
            serde_json::from_str(&std::fs::read_to_string(dir.path().join("request")).unwrap())
                .unwrap();
        assert_eq!(request["type"], "control_request");
        assert_eq!(request["request"]["subtype"], "initialize");
        let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
        assert!(args.contains("disableAllHooks") && args.contains("--no-session-persistence"));
        std::fs::write(&executable, "#!/bin/sh\nsleep 30\n").unwrap();
        let started = Instant::now();
        assert!(
            claude(
                &executable,
                dir.path().to_str().unwrap(),
                Duration::from_millis(100)
            )
            .is_err()
        );
        assert!(started.elapsed() < Duration::from_secs(2));
        assert!(total.elapsed() < Duration::from_secs(3));
    }
    #[test]
    fn claude_catalog_preserves_resolved_defaults_and_filters_disabled_models() {
        let catalog = Catalog::from_claude(&json!({"models":[
            {"value":"default","resolvedModel":"claude-opus-test","displayName":"Default"},
            {"value":"opus","resolvedModel":"claude-opus-test","displayName":"Opus"},
            {"value":"sonnet","resolvedModel":"claude-sonnet-test","supportedEffortLevels":["low","high"]},
            {"value":"hidden","disabled":true}
        ]})).unwrap();
        assert_eq!(catalog.default_model.as_deref(), Some("claude-opus-test"));
        assert_eq!(catalog.models.len(), 2);
        assert_eq!(catalog.models[1].efforts, ["low", "high"]);
        let older = Catalog::from_claude(&json!({"models":[{"value":"default"},{"value":"opus"}]}))
            .unwrap();
        assert_eq!(older.default_model, None);
        assert_eq!(older.models[0].id, "opus");
        assert!(
            Catalog::from_claude(&json!({"models":[{"value":"hidden","disabled":true}]})).is_err()
        );
    }
}
