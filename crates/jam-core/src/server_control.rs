//! Explicit server lifecycle actions. Status probes never install or stop a server.
use crate::{Message, Request, call_at, remote, server, socket, state_dir, stop_local_at};
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    fs, io,
    os::unix::{fs::OpenOptionsExt, net::UnixStream, process::CommandExt},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

fn binary_info(daemon: &Path) -> Result<Value> {
    let output = Command::new(daemon)
        .arg("--build-info")
        .output()
        .context("Read bundled server version")?;
    if !output.status.success() {
        bail!("Bundled server version check failed");
    }
    let info: Value = serde_json::from_slice(&output.stdout)?;
    if info["build"].as_str().is_none() {
        bail!("Bundled server has no build identifier");
    }
    Ok(info)
}
fn artifact(artifacts: &Path, arch: &str) -> Result<(PathBuf, String)> {
    let archive = artifacts.join(format!("{arch}.tar.gz"));
    let manifest: Value =
        serde_json::from_slice(&fs::read(artifacts.join(format!("{arch}.json")))?)?;
    let build = manifest["server_build"]
        .as_str()
        .context("Bundled server metadata is missing; rebuild the server bundle")?
        .to_owned();
    if build != crate::SERVER_BUILD || manifest["protocol"].as_str() != Some(crate::VERSION) {
        bail!("Bundled remote server is outdated; rebuild the server bundle");
    }
    if !archive.is_file() {
        bail!("Bundled server archive is missing");
    }
    Ok((archive, build))
}
fn endpoint_path(endpoint: &str, local_socket: &Path) -> Result<PathBuf> {
    if endpoint.is_empty() {
        Ok(local_socket.to_path_buf())
    } else {
        remote::local_socket(endpoint)
    }
}
fn status(endpoint: &str, daemon: &Path, artifacts: &Path, local_socket: &Path) -> Result<Value> {
    let available = if endpoint.is_empty() {
        binary_info(daemon)
            .ok()
            .and_then(|v| v["build"].as_str().map(str::to_owned))
    } else {
        ["aarch64", "x86_64"]
            .into_iter()
            .find_map(|arch| artifact(artifacts, arch).ok().map(|(_, build)| build))
    };
    let path = endpoint_path(endpoint, local_socket)?;
    let running = UnixStream::connect(&path).is_ok();
    let mut result = json!({"running":running,"compatible":false,"build":null,"available_build":available,"update_available":false,"terminals":null,"error":null});
    match call_at(&path, Request::HostInfo) {
        Ok(Message::HostInfo { build, .. }) => {
            result["compatible"] = true.into();
            result["update_available"] = available
                .as_ref()
                .is_some_and(|expected| expected != &build)
                .into();
            result["build"] = build.into();
            if let Message::State(state) = call_at(&path, Request::State)? {
                result["terminals"] = state.panes.iter().filter(|p| !p.exited).count().into();
            }
        }
        Ok(_) => result["error"] = "Unexpected server response".into(),
        Err(error) => {
            result["error"] = error.to_string().into();
            result["update_available"] = (running && available.is_some()).into();
        }
    }
    Ok(result)
}

fn start_local(
    daemon: &Path,
    expected: &str,
    local_socket: &Path,
    local_state: &Path,
) -> Result<()> {
    server::private_dir(local_state)?;
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(local_state.join("jamd.log"))?;
    let mut command = Command::new(daemon);
    command
        .env("JAM_SOCKET", local_socket)
        .env("JAM_STATE_DIR", local_state)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        });
    }
    let mut child = command.spawn()?;
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if let Some(code) = child.try_wait()? {
            bail!("Server exited ({code}); inspect jamd.log");
        }
        if matches!(call_at(local_socket, Request::HostInfo), Ok(Message::HostInfo { build, .. }) if build == expected)
        {
            std::thread::spawn(move || {
                let _ = child.wait();
            });
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    bail!("Server did not become ready; inspect jamd.log")
}

pub fn manage(request: &Value) -> Result<Value> {
    manage_at(request, &socket(), &state_dir())
}

pub fn manage_at(request: &Value, local_socket: &Path, local_state: &Path) -> Result<Value> {
    let endpoint = request["endpoint"].as_str().unwrap_or("");
    let daemon = Path::new(request["daemon"].as_str().unwrap_or(""));
    let artifacts = Path::new(request["artifacts"].as_str().unwrap_or(""));
    let action = request["action"].as_str().unwrap_or("status");
    if action == "status" {
        return status(endpoint, daemon, artifacts, local_socket);
    }
    if !["update", "restart"].contains(&action) {
        bail!("Unknown server action");
    }
    let force = request["force"].as_bool().unwrap_or(false);
    if endpoint.is_empty() {
        static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
        let _guard = LOCK.lock().unwrap();
        let expected = binary_info(daemon)?["build"].as_str().unwrap().to_owned();
        if UnixStream::connect(local_socket).is_ok() {
            if !force {
                match call_at(local_socket, Request::PrepareUpgrade) {
                    Ok(Message::Ok) => (),
                    Err(error) if error.to_string().contains("prevent upgrade") => {
                        return Ok(json!({"deferred":true}));
                    }
                    _ => bail!(
                        "Cannot safely update this server automatically; use Update and Restart"
                    ),
                }
            }
            stop_local_at(local_socket)?;
            let deadline = Instant::now() + Duration::from_secs(10);
            while UnixStream::connect(local_socket).is_ok() {
                if Instant::now() >= deadline {
                    bail!("Server did not stop");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        }
        start_local(daemon, &expected, local_socket, local_state)?;
    } else {
        let expected = if action == "update" {
            let arch = remote::server_arch(endpoint)?;
            let (archive, build) = artifact(artifacts, &arch)?;
            if !remote::install_mode(endpoint, &archive, force)? {
                return Ok(json!({"deferred":true}));
            }
            Some(build)
        } else {
            if !force {
                bail!("A remote restart requires explicit confirmation");
            }
            remote::restart_server(endpoint)?;
            None
        };
        remote::disconnect(endpoint);
        let path = remote::connect(endpoint)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Ok(Message::HostInfo { build, .. }) = call_at(&path, Request::HostInfo)
                && expected.as_ref().is_none_or(|expected| expected == &build)
            {
                break;
            }
            if Instant::now() >= deadline {
                bail!("Server changed but reconnect verification failed; reconnect to inspect it");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    Ok(json!({"ok":true,"deferred":false}))
}
