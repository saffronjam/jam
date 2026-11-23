//! Server inspection and explicit, identity-bound takeover transactions.
use super::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

#[derive(Clone, Serialize, Deserialize)]
struct Approval {
    endpoint: String,
    profile: String,
    identity: Value,
    expires: u64,
}
static APPROVALS: LazyLock<Mutex<HashMap<String, Approval>>> = LazyLock::new(Default::default);
static OPERATIONS: LazyLock<Mutex<HashMap<String, Value>>> = LazyLock::new(Default::default);
static ACTIVE: LazyLock<Mutex<HashMap<String, String>>> = LazyLock::new(Default::default);
fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn failure(error: &anyhow::Error, phase: &str) -> Value {
    json!({"code": if phase == "inspection" { "inspection_failed" } else { "server_action_failed" },
        "summary":if phase == "inspection" { "Could not inspect the server." } else { "Could not complete the server action." }, "phase":phase,
        "details":format!("{error:#}"), "retryable":true})
}
fn belongs_to_installation(executable: &Path, daemon: &Path) -> Result<bool> {
    let daemon = fs::canonicalize(daemon)?;
    if executable == daemon {
        return Ok(true);
    }
    // The macOS packager retains live, atomically replaced bundles beside the
    // installation. They remain ours even though their path has moved.
    let Some(bundle) = daemon.ancestors().nth(3) else {
        return Ok(false);
    };
    let Some(name) = bundle.file_name().and_then(|v| v.to_str()) else {
        return Ok(false);
    };
    if !name.ends_with(".app") {
        return Ok(false);
    }
    let Some(parent) = bundle.parent() else {
        return Ok(false);
    };
    let retired = parent.join(format!(".{name}.retired"));
    let Ok(relative) = executable.strip_prefix(retired) else {
        return Ok(false);
    };
    let mut components = relative.components();
    let Some(std::path::Component::Normal(generation)) = components.next() else {
        return Ok(false);
    };
    Ok(generation.to_string_lossy().starts_with("bundle-")
        && components.as_path() == Path::new(name).join("Contents/MacOS/jamd"))
}
fn inspect(
    endpoint: &str,
    daemon: &Path,
    artifacts: &Path,
    path: &Path,
    state_dir: &Path,
) -> Result<Value> {
    let mut value = if endpoint.is_empty() {
        super::status(endpoint, daemon, artifacts, path)?
    } else {
        json!({"running":false,"compatible":false,"build":null,"available_build":null,"update_available":false,"terminals":null,"error":null})
    };
    let mut info = Value::Null;
    let mut identity = Value::Null;
    let mut foreign = false;
    let mut versions = vec![];
    let mut remote_recovery = Value::Null;
    if endpoint.is_empty() {
        if value["running"] == true {
            let stream = UnixStream::connect(path)?;
            let (pid, uid) = crate::protocol::peer_identity(&stream)?;
            anyhow::ensure!(
                uid == unsafe { libc::geteuid() },
                "Server belongs to another user"
            );
            // A bundle swap can unlink the old executable while its daemon is
            // still serving sessions. Live protocol metadata remains valid.
            let executable = crate::protocol::local_daemon_executable_on(&stream)
                .ok()
                .filter(|path| path.is_file());
            if let Some(executable) = &executable {
                anyhow::ensure!(
                    matches!(
                        executable.file_name().and_then(|v| v.to_str()),
                        Some("jamd" | "jam")
                    ),
                    "Socket is not owned by a Jam server"
                );
            }
            info = if value["compatible"] == true {
                json!({"build":value["build"], "protocol":value["protocol"],
                    "release_version":value["release_version"]})
            } else {
                binary_info(
                    executable
                        .as_deref()
                        .context("Could not locate the running server executable")?,
                )?
            };
            if let Some(parent) = path.parent()
                && let Ok(bytes) = fs::read(parent.join("release.json"))
                && let Ok(metadata) = serde_json::from_slice::<Value>(&bytes)
                && metadata["pid"] == pid
            {
                versions.push(metadata["release_version"].clone());
            }
            value["management_available"] = json!(executable.is_some());
            if let Some(executable) = executable {
                foreign = !belongs_to_installation(&executable, daemon)?;
                identity = json!({"pid":pid,"start":crate::protocol::process_start(pid)?,"executable":executable,"build":info["build"]});
            }
        }
    } else {
        let remote = remote::inspect_server(endpoint)?;
        remote_recovery = remote["recovery"].clone();
        value["running"] = remote["running"].clone();
        info = remote["info"].clone();
        identity = remote["identity"].clone();
        versions.extend([
            remote["installed_version"].clone(),
            remote["runtime_version"].clone(),
        ]);
        if let Some(arch) = remote["arch"].as_str() {
            value["available_build"] = artifact(artifacts, arch)
                .ok()
                .map(|(_, build)| json!(build))
                .unwrap_or(Value::Null);
        }
        // A wire connection is only attempted for a supported protocol.
        if info["protocol"] == crate::VERSION
            && value["running"] == true
            && let Ok(socket) = remote::connect(endpoint)
            && let Ok(current) = super::status(endpoint, daemon, artifacts, &socket)
        {
            value = current;
        }
    }
    versions.push(info["release_version"].clone());
    let current = semver::Version::parse(crate::RELEASE_VERSION)?;
    let newer = versions
        .iter()
        .filter_map(|v| v.as_str())
        .filter_map(|v| semver::Version::parse(v).ok())
        .filter(|v| v > &current)
        .max();
    let state = if let Some(version) = newer {
        value["minimum_version"] = json!(version.to_string());
        "client_update_required"
    } else if value["running"] != true {
        "offline"
    } else if value["compatible"] != true
        || info["release_version"]
            .as_str()
            .and_then(|v| semver::Version::parse(v).ok())
            .is_none()
    {
        "unsupported"
    } else if foreign {
        "takeover_required"
    } else if value["update_available"] == true {
        "update_available"
    } else {
        "connected"
    };
    value["state"] = json!(state);
    value["identity"] = identity;
    value["build"] = info
        .get("build")
        .cloned()
        .unwrap_or_else(|| value["build"].clone());
    if !remote_recovery.is_null() && state != "client_update_required" {
        value["identity"] = json!({"recovery_remote":remote_recovery["id"],"phase":remote_recovery["phase"],"live":value["identity"]});
        value["state"] = json!("recovery_required");
        value["can_takeover"] = json!(value["available_build"].is_string());
        return Ok(value);
    }
    if endpoint.is_empty()
        && state != "client_update_required"
        && let Ok(entries) = fs::read_dir(state_dir.join("server-actions"))
    {
        for entry in entries {
            let entry = entry?;
            let journal = entry.path().join("transaction.json");
            if !journal.is_file() {
                continue;
            }
            let saved: Value = serde_json::from_slice(&fs::read(&journal)?)?;
            if !matches!(saved["phase"].as_str(), Some("committed" | "rolled_back")) {
                value["state"] = json!("recovery_required");
                value["identity"] = json!({"recovery":entry.file_name().to_string_lossy(),"phase":saved["phase"],"live":value["identity"]});
                value["can_takeover"] = json!(true);
                return Ok(value);
            }
        }
    }
    value["can_takeover"] = json!(
        matches!(state, "unsupported" | "takeover_required")
            && value["available_build"].is_string()
            && !value["identity"].is_null()
    );
    // Incompatibility is a persistent status, not an operation error.
    if matches!(
        state,
        "unsupported" | "takeover_required" | "client_update_required"
    ) {
        value["error"] = Value::Null;
    }
    Ok(value)
}
fn journal_phase(path: &Path, phase: &str) -> Result<()> {
    let mut value: Value = serde_json::from_slice(&fs::read(path)?)?;
    value["phase"] = json!(phase);
    write_journal(path, &value)
}
fn write_journal(path: &Path, value: &Value) -> Result<()> {
    use std::io::Write;
    let temporary = path.with_extension(format!("{}.tmp", crate::id()));
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(0o600)
        .open(&temporary)?;
    file.write_all(&serde_json::to_vec(&value)?)?;
    file.sync_all()?;
    fs::rename(temporary, path)?;
    fs::File::open(path.parent().context("Missing journal directory")?)?.sync_all()?;
    Ok(())
}
fn recover(id: &str, identity: &Value, socket: &Path, state: &Path) -> Result<()> {
    use fs2::FileExt;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(state.join("server-action.lock"))?;
    lock.try_lock_exclusive()
        .context("Another server action is running")?;

    let recovery = identity["recovery"]
        .as_str()
        .context("Missing recovery identity")?;
    uuid::Uuid::parse_str(recovery)?;
    let journal = state
        .join("server-actions")
        .join(recovery)
        .join("transaction.json");
    let value: Value = serde_json::from_slice(&fs::read(&journal)?)?;
    let old = Path::new(
        value["previous"]
            .as_str()
            .context("Missing original server")?,
    );
    let new = Path::new(
        value["replacement"]
            .as_str()
            .context("Missing replacement server")?,
    );
    if value["phase"] == "health_verified" {
        anyhow::ensure!(
            crate::protocol::local_daemon_executable(socket)? == fs::canonicalize(new)?,
            "The running server changed. Recovery was left pending"
        );
        if state.join(".upgrade-boot.json").exists() {
            call_at(
                socket,
                Request::CommitUpgrade {
                    token: recovery.into(),
                },
            )?;
        }
        journal_phase(&journal, "committed")?;
        return Ok(());
    }
    progress(id, "restoring");
    if let Ok(stream) = UnixStream::connect(socket) {
        let executable = crate::protocol::local_daemon_executable_on(&stream)?;
        if executable == fs::canonicalize(old)? && value["phase"] == "stopping" {
            journal_phase(&journal, "rolled_back")?;
            return Ok(());
        }
        anyhow::ensure!(
            executable == fs::canonicalize(new)? && state.join(".upgrade-boot.json").is_file(),
            "Another server is running. Recovery was left pending"
        );
        let (pid, _) = crate::protocol::peer_identity(&stream)?;
        anyhow::ensure!(
            identity["live"]["pid"] == pid,
            "Server changed before recovery"
        );
        crate::protocol::stop_local_expected(
            socket,
            pid,
            &executable,
            identity["live"]["start"]
                .as_str()
                .context("Missing process identity")?,
        )?;
    }
    anyhow::ensure!(
        UnixStream::connect(socket).is_err(),
        "Server did not stop for recovery"
    );
    crate::upgrade::restore_checkpoint_at(&journal, state)?;
    let build = binary_info(old)?["build"]
        .as_str()
        .context("Missing original build")?
        .to_owned();
    start_local(old, &build, socket, state)?;
    journal_phase(&journal, "rolled_back")
}
fn progress(id: &str, phase: &str) {
    if let Some(value) = OPERATIONS.lock().unwrap().get_mut(id) {
        value["phase"] = json!(phase);
    }
}
fn run(id: &str, request: &Value, approval: Approval, socket: &Path, state: &Path) -> Result<()> {
    let daemon = Path::new(
        request["daemon"]
            .as_str()
            .context("Missing server executable")?,
    );
    let artifacts = Path::new(request["artifacts"].as_str().unwrap_or(""));
    let current = inspect(&approval.endpoint, daemon, artifacts, socket, state)?;
    anyhow::ensure!(
        current["can_takeover"] == true && current["identity"] == approval.identity,
        "The server changed. Inspect it again before taking over"
    );
    anyhow::ensure!(
        now() <= approval.expires,
        "Confirmation expired. Inspect the server again"
    );
    if approval.identity.get("recovery").is_some() {
        return recover(id, &approval.identity, socket, state);
    }
    progress(id, "installing");
    if approval.endpoint.is_empty() {
        use fs2::FileExt;
        server::private_dir(state)?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(state.join("server-action.lock"))?;
        lock.try_lock_exclusive()
            .context("Another server action is running")?;
        let check = inspect("", daemon, artifacts, socket, state)?;
        anyhow::ensure!(
            check["identity"] == approval.identity && check["can_takeover"] == true,
            "The server changed before shutdown"
        );
        let previous = PathBuf::from(
            approval.identity["executable"]
                .as_str()
                .context("Missing previous executable")?,
        );
        let previous_build = approval.identity["build"]
            .as_str()
            .context("Missing previous build")?;
        let directory = state.join("server-actions").join(id);
        server::private_dir(&directory)?;
        let journal = directory.join("transaction.json");
        write_journal(
            &journal,
            &json!({"phase":"stopping","previous":previous,"replacement":daemon,"socket":socket,"state":state}),
        )?;
        crate::protocol::stop_local_expected(
            socket,
            approval.identity["pid"]
                .as_i64()
                .context("Missing server PID")? as i32,
            &previous,
            approval.identity["start"]
                .as_str()
                .context("Missing process identity")?,
        )?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while let Ok(stream) = UnixStream::connect(socket) {
            anyhow::ensure!(
                crate::protocol::peer_identity(&stream)?.0 as i64
                    == approval.identity["pid"].as_i64().unwrap_or(-1),
                "Another server started during shutdown"
            );
            anyhow::ensure!(Instant::now() < deadline, "Server did not stop");
            std::thread::sleep(Duration::from_millis(25));
        }
        if let Err(error) = crate::upgrade::checkpoint_at(&journal, state) {
            start_local(&previous, previous_build, socket, state)
                .context("Checkpoint failed and the previous server could not restart")?;
            return Err(error);
        }
        write_journal(&state.join(".upgrade-boot.json"), &json!({"token":id}))?;
        write_journal(
            &journal,
            &json!({"phase":"installing","previous":previous,"replacement":daemon,"socket":socket,"state":state}),
        )?;
        let expected = binary_info(daemon)?["build"]
            .as_str()
            .context("Missing replacement build")?
            .to_owned();
        if let Err(error) = start_local(daemon, &expected, socket, state) {
            progress(id, "restoring");
            anyhow::ensure!(
                UnixStream::connect(socket).is_err(),
                "Another server started during takeover. Saved state and checkpoint were left untouched"
            );
            crate::upgrade::restore_checkpoint_at(&journal, state)?;
            start_local(&previous, previous_build, socket, state).context(format!(
                "Replacement failed: {error:#}. Could not restore the previous server"
            ))?;
            journal_phase(&journal, "rolled_back")?;
            bail!(
                "Replacement failed. The previous server and saved state were restored. {error:#}"
            );
        }
        // Once commit can have resumed agents, never roll back uncertain delivery.
        write_journal(
            &journal,
            &json!({"phase":"health_verified","previous":previous,"replacement":daemon,"socket":socket,"state":state,"token":id}),
        )?;
        call_at(socket, Request::CommitUpgrade { token: id.into() })?;
        journal_phase(&journal, "committed")?;
    } else {
        let arch = remote::server_arch(&approval.endpoint)?;
        let (archive, _) = artifact(artifacts, &arch)?;
        remote::take_over(&approval.endpoint, &archive, &approval.identity)?;
    }
    progress(id, "connecting");
    let path = if approval.endpoint.is_empty() {
        socket.to_path_buf()
    } else {
        remote::disconnect(&approval.endpoint);
        remote::connect(&approval.endpoint)?
    };
    anyhow::ensure!(
        matches!(call_at(&path, Request::HostInfo)?, Message::HostInfo { .. }),
        "Server did not confirm its connection"
    );
    Ok(())
}
pub(super) fn manage(request: &Value, socket: &Path, state: &Path) -> Result<Value> {
    let endpoint = request["endpoint"].as_str().unwrap_or("");
    let profile = format!("{}|{}|{}", endpoint, socket.display(), state.display());
    match request["action"].as_str().unwrap_or("status") {
        "status" => match inspect(
            endpoint,
            Path::new(request["daemon"].as_str().unwrap_or("")),
            Path::new(request["artifacts"].as_str().unwrap_or("")),
            socket,
            state,
        ) {
            Ok(value) => Ok(value),
            Err(error) => {
                let running = endpoint.is_empty() && UnixStream::connect(socket).is_ok();
                Ok(
                    json!({"running":running,"compatible":false,"update_available":running,
                    "state":"inspection_failed", "management_available":false,
                    "failure":failure(&error,"inspection"),"error":format!("{error:#}")}),
                )
            }
        },
        "prepare_takeover" => {
            let value = inspect(
                endpoint,
                Path::new(request["daemon"].as_str().unwrap_or("")),
                Path::new(request["artifacts"].as_str().unwrap_or("")),
                socket,
                state,
            )?;
            anyhow::ensure!(
                value["can_takeover"] == true,
                "This server cannot be taken over"
            );
            let token = crate::id();
            APPROVALS
                .lock()
                .unwrap()
                .retain(|_, value| value.expires >= now());
            APPROVALS.lock().unwrap().insert(
                token.clone(),
                Approval {
                    endpoint: endpoint.into(),
                    profile: profile.clone(),
                    identity: value["identity"].clone(),
                    expires: now() + 300,
                },
            );
            Ok(json!({"token":token,"status":value}))
        }
        "operation" => OPERATIONS
            .lock()
            .unwrap()
            .get(request["id"].as_str().unwrap_or(""))
            .cloned()
            .context("Unknown server operation"),
        "takeover" | "begin_operation" => {
            let approval = if request["action"] == "begin_operation" {
                anyhow::ensure!(
                    matches!(request["kind"].as_str(), Some("update" | "restart")),
                    "Unknown server action"
                );
                Approval {
                    endpoint: endpoint.into(),
                    profile: profile.clone(),
                    identity: Value::Null,
                    expires: now() + 300,
                }
            } else {
                let token = request["token"].as_str().context("Missing confirmation")?;
                let approval = APPROVALS
                    .lock()
                    .unwrap()
                    .remove(token)
                    .context("Confirmation expired or already used")?;
                anyhow::ensure!(
                    approval.endpoint == endpoint && approval.profile == profile,
                    "Confirmation belongs to another machine"
                );
                approval
            };
            let id = crate::id();
            let mut active = ACTIVE.lock().unwrap();
            anyhow::ensure!(
                !active.contains_key(&profile),
                "Another server action is running"
            );
            active.insert(profile.clone(), id.clone());
            drop(active);
            OPERATIONS.lock().unwrap().insert(
                id.clone(),
                json!({"id":id,"phase":"preparing","done":false}),
            );
            let (request, socket, state, operation) = (
                request.clone(),
                socket.to_path_buf(),
                state.to_path_buf(),
                id.clone(),
            );
            std::thread::spawn(move || {
                let askpass = request["askpass"].as_str().map(str::to_owned);
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    remote::interactive(askpass, || {
                        if request["action"] == "begin_operation" {
                            let mut action = request.clone();
                            action["action"] = request["kind"].clone();
                            progress(&operation, "installing");
                            super::manage_at(&action, &socket, &state)
                        } else {
                            run(&operation, &request, approval.clone(), &socket, &state)
                                .map(|_| json!({"ok":true}))
                        }
                    })
                }))
                .unwrap_or_else(|_| Err(anyhow::anyhow!("Server action stopped unexpectedly")));
                let mut operations = OPERATIONS.lock().unwrap();
                let value = operations.get_mut(&operation).unwrap();
                value["done"] = json!(true);
                if let Err(error) = result {
                    value["failure"] =
                        failure(&error, value["phase"].as_str().unwrap_or("unknown"));
                } else if let Ok(reply) = result {
                    value["deferred"] = reply["deferred"].clone();
                    value["phase"] = json!("complete");
                }
                ACTIVE.lock().unwrap().remove(&approval.profile);
            });
            Ok(json!({"id":id}))
        }
        _ => bail!("Unknown server action"),
    }
}
