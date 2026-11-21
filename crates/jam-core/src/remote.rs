//! Saved SSH endpoints. OpenSSH owns authentication, channel multiplexing and encryption.
use crate::*;
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    os::unix::fs::{DirBuilderExt, FileTypeExt},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::{Mutex, OnceLock},
};
#[derive(Clone, Serialize, Deserialize)]
pub struct Remote {
    pub id: String,
    pub name: String,
    pub destination: String,
}
fn registry_lock() -> Result<fs::File> {
    use fs2::FileExt;
    server::private_dir(&state_dir())?;
    let lock = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(state_dir().join("remotes.lock"))?;
    lock.lock_exclusive()?;
    Ok(lock)
}
fn registry() -> PathBuf {
    state_dir().join("remotes.json")
}
pub fn list() -> Result<Vec<Remote>> {
    match fs::read(registry()) {
        Ok(bytes) => Ok(serde_json::from_slice(&bytes)?),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(vec![]),
        Err(e) => Err(e.into()),
    }
}
pub fn add(name: String, destination: String) -> Result<Remote> {
    let _lock = registry_lock()?;
    validate_destination(&destination)?;
    let mut entries = list()?;
    if name.trim().is_empty()
        || entries
            .iter()
            .any(|r| r.name == name || r.destination == destination)
    {
        bail!("Remote name and destination must be nonempty and unique")
    }
    let remote = Remote {
        id: id(),
        name,
        destination,
    };
    entries.push(remote.clone());
    save(&entries)?;
    Ok(remote)
}
pub fn remove(id: &str) -> Result<()> {
    let _lock = registry_lock()?;
    let mut entries = list()?;
    entries.retain(|r| r.id != id);
    save(&entries)
}
fn save(entries: &[Remote]) -> Result<()> {
    server::private_dir(&state_dir())?;
    let path = registry();
    let temporary = path.with_extension(format!("{}.tmp", id()));
    fs::write(&temporary, serde_json::to_vec_pretty(entries)?)?;
    fs::rename(temporary, path)?;
    Ok(())
}
pub fn validate_destination(value: &str) -> Result<()> {
    if value.is_empty()
        || value.starts_with('-')
        || value.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        bail!("Use an SSH alias or user@host, without options or whitespace")
    }
    Ok(())
}
thread_local! { static ASKPASS: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) }; }
pub fn interactive<T>(helper: Option<String>, operation: impl FnOnce() -> T) -> T {
    struct Reset;
    impl Drop for Reset {
        fn drop(&mut self) {
            ASKPASS.with(|v| *v.borrow_mut() = None);
        }
    }
    ASKPASS.with(|v| *v.borrow_mut() = helper);
    let _reset = Reset;
    operation()
}
#[cfg(test)]
thread_local! { static TEST_SSH: std::cell::RefCell<Option<PathBuf>> = const { std::cell::RefCell::new(None) }; }
#[cfg(test)]
pub(crate) fn with_test_ssh<T>(path: PathBuf, body: impl FnOnce() -> T) -> T {
    struct Reset(Option<PathBuf>);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_SSH.with(|command| *command.borrow_mut() = self.0.take());
        }
    }
    let _reset = Reset(TEST_SSH.with(|command| command.replace(Some(path))));
    body()
}
fn ssh(destination: &str) -> Result<Command> {
    validate_destination(destination)?;
    #[cfg(test)]
    let executable = TEST_SSH
        .with(|path| path.borrow().clone())
        .unwrap_or_else(|| "/usr/bin/ssh".into());
    #[cfg(not(test))]
    let executable = "/usr/bin/ssh";
    let mut cmd = Command::new(executable);
    #[cfg(feature = "fixture-services")]
    cmd.arg("-F").arg(
        std::path::PathBuf::from(
            std::env::var_os("HOME").context("Fixture SSH requires an isolated HOME")?,
        )
        .join(".ssh/config"),
    );
    cmd.args([
        "-T",
        "-oConnectTimeout=10",
        "-oControlMaster=no",
        "-oControlPath=none",
        "-oServerAliveInterval=10",
        "-oServerAliveCountMax=3",
    ]);
    ASKPASS.with(|helper| {
        if let Some(helper) = helper.borrow().as_ref() {
            cmd.args(["-oBatchMode=no", "-oStrictHostKeyChecking=ask"]);
            cmd.env("SSH_ASKPASS", helper)
                .env("SSH_ASKPASS_REQUIRE", "force")
                .env("DISPLAY", ":0");
        } else {
            cmd.args(["-oBatchMode=yes", "-oStrictHostKeyChecking=yes"]);
        }
    });
    Ok(cmd)
}
struct Tunnel {
    process: Child,
    socket: PathBuf,
}
impl Tunnel {
    fn is_alive(&mut self) -> Result<bool> {
        Ok(self.process.try_wait()?.is_none()
            && fs::symlink_metadata(&self.socket)
                .is_ok_and(|metadata| metadata.file_type().is_socket()))
    }
}
impl Drop for Tunnel {
    fn drop(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
        let _ = fs::remove_file(&self.socket);
        let _ = fs::remove_file(self.socket.with_extension("log"));
    }
}
static TUNNELS: OnceLock<Mutex<HashMap<String, Tunnel>>> = OnceLock::new();
fn tunnels() -> &'static Mutex<HashMap<String, Tunnel>> {
    TUNNELS.get_or_init(|| Mutex::new(HashMap::new()))
}
// A process owns its listeners. A shared endpoint pathname would let a short
// CLI invocation unlink the GUI's active forwarding socket during connect.
static LOCAL_DIRECTORY: OnceLock<PathBuf> = OnceLock::new();
extern "C" fn cleanup_local_directory() {
    if let Some(directory) = LOCAL_DIRECTORY.get() {
        // Normal process exit does not drop Rust statics. Closing the process's
        // inherited stdin also ends SSH; remove only this process's namespace.
        let _ = fs::remove_dir_all(directory);
    }
}
fn local_directory() -> Result<&'static PathBuf> {
    if let Some(directory) = LOCAL_DIRECTORY.get() {
        return Ok(directory);
    }
    let parent = PathBuf::from("/tmp").join(format!("jam-remote-{}", unsafe { libc::geteuid() }));
    server::private_dir(&parent)?;
    let directory = loop {
        let nonce = uuid::Uuid::new_v4().simple().to_string();
        let candidate = parent.join(format!("p{}-{}", std::process::id(), &nonce[..12]));
        match fs::DirBuilder::new().mode(0o700).create(&candidate) {
            Ok(()) => break candidate,
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    };
    if LOCAL_DIRECTORY.set(directory.clone()).is_ok() {
        unsafe { libc::atexit(cleanup_local_directory) };
    } else {
        fs::remove_dir(&directory)?;
    }
    Ok(LOCAL_DIRECTORY.get().unwrap())
}
pub fn local_socket(id: &str) -> Result<PathBuf> {
    let endpoint = uuid::Uuid::parse_str(id).context("Invalid remote identifier")?;
    Ok(local_directory()?.join(format!("{endpoint}.sock")))
}
pub fn disconnect(id: &str) {
    tunnels().lock().unwrap().remove(id);
}
pub fn connect(id: &str) -> Result<PathBuf> {
    let remote = list()?
        .into_iter()
        .find(|r| r.id == id)
        .context("Unknown remote")?;
    let path = local_socket(id)?;
    static LOCKS: OnceLock<Mutex<HashMap<String, std::sync::Arc<Mutex<()>>>>> = OnceLock::new();
    let lock = LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .entry(id.into())
        .or_default()
        .clone();
    let _guard = lock.lock().unwrap();
    let mut active = tunnels().lock().unwrap();
    if let Some(tunnel) = active.get_mut(id)
        && tunnel.is_alive()?
    {
        return Ok(path);
    }
    active.remove(id);
    drop(active);
    let output = ssh(&remote.destination)?
        .arg(&remote.destination)
        .arg("printf '%s\\n' \"${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/jam-$(id -u)/jamd.sock\"")
        .output()?;
    if !output.status.success() {
        bail!(
            "SSH connection failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        )
    }
    let target = String::from_utf8(output.stdout)?.trim().to_owned();
    if !target.starts_with('/') || target.contains(['\n', '\r', ':']) {
        bail!("Invalid remote socket path")
    }
    if path.exists() {
        fs::remove_file(&path)?;
    }
    let log_path = path.with_extension("log");
    let log = fs::File::create(&log_path)?;
    let process = ssh(&remote.destination)?
        .args([
            "-oExitOnForwardFailure=yes",
            "-oStreamLocalBindUnlink=yes",
            "-L",
        ])
        .arg(format!("{}:{target}", path.display()))
        .arg(&remote.destination)
        .arg("cat >/dev/null")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(log)
        .spawn()?;
    let mut active = tunnels().lock().unwrap();
    active.insert(
        id.into(),
        Tunnel {
            process,
            socket: path.clone(),
        },
    );
    for _ in 0..100 {
        if path.exists() {
            return Ok(path);
        }
        if active.get_mut(id).unwrap().process.try_wait()?.is_some() {
            let error = fs::read_to_string(&log_path).unwrap_or_default();
            active.remove(id);
            bail!("SSH forwarding failed: {}", error.trim());
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    active.remove(id);
    bail!("SSH forwarding timed out")
}
fn lifecycle_lock(id: &str) -> std::sync::Arc<Mutex<()>> {
    static LOCKS: OnceLock<Mutex<HashMap<String, std::sync::Arc<Mutex<()>>>>> = OnceLock::new();
    LOCKS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap()
        .entry(id.into())
        .or_default()
        .clone()
}
pub fn install(id: &str, archive: &std::path::Path) -> Result<()> {
    let lock = lifecycle_lock(id);
    let _guard = lock.lock().unwrap();
    if install_unlocked(id, archive, false, None, None)? {
        Ok(())
    } else {
        bail!("Running sessions prevent the server update")
    }
}
pub fn install_mode(id: &str, archive: &std::path::Path, force: bool) -> Result<bool> {
    let lock = lifecycle_lock(id);
    let _guard = lock.lock().unwrap();
    install_unlocked(id, archive, force, Some(crate::SERVER_BUILD), None)
}
/// Install a release archive already authenticated by the shared updater.
pub fn install_release(
    id: &str,
    archive: &std::path::Path,
    approval: &serde_json::Value,
) -> Result<bool> {
    let lock = lifecycle_lock(id);
    let _guard = lock.lock().unwrap();
    install_unlocked(id, archive, true, None, Some(approval))
}
fn install_unlocked(
    id: &str,
    archive: &std::path::Path,
    force: bool,
    expected: Option<&str>,
    approval: Option<&serde_json::Value>,
) -> Result<bool> {
    let remote = list()?
        .into_iter()
        .find(|r| r.id == id)
        .context("Unknown remote")?;
    let mode = if force {
        "--restart-active"
    } else {
        "--defer-if-busy"
    };
    let expected = expected
        .map(|build| format!(" --expected-build={build}"))
        .unwrap_or_default();
    let approved = if let Some(value) = approval {
        use base64::Engine;
        format!(
            " --approved-state={}",
            base64::engine::general_purpose::STANDARD.encode(serde_json::to_vec(value)?)
        )
    } else {
        String::new()
    };
    let output = ssh(&remote.destination)?.arg(&remote.destination).arg(format!("set -eu; d=$(mktemp -d); trap 'rm -rf \"$d\"' EXIT; tar -xz -C \"$d\"; python3 \"$d/install.py\" \"$d\" {mode}{expected}{approved}")).stdin(fs::File::open(archive)?).output()?;
    if output.status.code() == Some(75) {
        return Ok(false);
    }
    if !output.status.success() {
        bail!(
            "Remote setup: {}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
    }
    Ok(true)
}
pub fn server_arch(id: &str) -> Result<String> {
    let remote = list()?
        .into_iter()
        .find(|r| r.id == id)
        .context("Unknown remote")?;
    let output = ssh(&remote.destination)?
        .arg(&remote.destination)
        .arg("uname -m")
        .output()?;
    if !output.status.success() {
        bail!(
            "Could not inspect remote: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let arch = String::from_utf8(output.stdout)?.trim().to_owned();
    if !["aarch64", "x86_64"].contains(&arch.as_str()) {
        bail!("Unsupported server architecture");
    }
    Ok(arch)
}
pub fn restart_server(id: &str) -> Result<()> {
    let lock = lifecycle_lock(id);
    let _guard = lock.lock().unwrap();
    let remote = list()?
        .into_iter()
        .find(|r| r.id == id)
        .context("Unknown remote")?;
    let output = ssh(&remote.destination)?
        .arg(&remote.destination)
        .arg("systemctl --user restart jamd.service")
        .output()?;
    if !output.status.success() {
        bail!(
            "Server restart failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn destination_is_one_argument() {
        for bad in ["", "-Fbad", "edge@host\nexec", "host -oProxyCommand=x"] {
            assert!(validate_destination(bad).is_err());
        }
        for good in ["dev@dev-server", "my-alias", "user@2001:db8::1"] {
            assert!(validate_destination(good).is_ok());
        }
    }
    #[test]
    #[ignore = "private subprocess transport fixture"]
    fn tunnel_process_fixture() {
        let directory = PathBuf::from(std::env::var("JAM_TUNNEL_TEST_DIR").unwrap());
        let endpoint = std::env::var("JAM_TUNNEL_TEST_ENDPOINT").unwrap();
        let role = std::env::var("JAM_TUNNEL_TEST_ROLE").unwrap();
        TEST_SSH.with(|command| *command.borrow_mut() = Some(directory.join("ssh-fixture")));
        let socket = connect(&endpoint).unwrap();
        fs::write(
            directory.join(format!("{role}.path")),
            socket.to_str().unwrap(),
        )
        .unwrap();
        if role == "gui" {
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
            while !directory.join("gui.stop").exists() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "fixture control timed out"
                );
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
        } else {
            // A live SSH child is insufficient when its pathname disappeared.
            // Reconnect must replace only this process's own forwarding child.
            fs::remove_file(&socket).unwrap();
            assert_eq!(connect(&endpoint).unwrap(), socket);
        }
        disconnect(&endpoint);
        assert!(!socket.exists());
        assert!(!socket.with_extension("log").exists());
    }
    #[test]
    fn independent_cli_tunnel_preserves_gui_forwarding() {
        use std::io::Read;
        use std::os::unix::{fs::PermissionsExt, net::UnixStream};
        struct KillOnDrop(Child);
        impl Drop for KillOnDrop {
            fn drop(&mut self) {
                let _ = self.0.kill();
                let _ = self.0.wait();
            }
        }
        let fixture = tempfile::tempdir().unwrap();
        let directory = fixture.path();
        let state = directory.join("state");
        server::private_dir(&state).unwrap();
        let endpoint = crate::id();
        fs::write(
            state.join("remotes.json"),
            serde_json::to_vec(&vec![Remote {
                id: endpoint.clone(),
                name: "fixture".into(),
                destination: "fixture-host".into(),
            }])
            .unwrap(),
        )
        .unwrap();
        let executable = directory.join("ssh-fixture");
        fs::write(
            &executable,
            r#"#!/usr/bin/env python3
import os, select, socket, sys
if '-L' not in sys.argv:
    print('/tmp/jam-transport-fixture-target.sock')
    sys.exit(0)
path = sys.argv[sys.argv.index('-L') + 1].split(':', 1)[0]
listener = socket.socket(socket.AF_UNIX)
listener.bind(path)
listener.listen(10)
try:
    while True:
        ready, _, _ = select.select([listener, sys.stdin], [], [], 1)
        if sys.stdin in ready and not sys.stdin.read(1):
            break
        if listener in ready:
            connection, _ = listener.accept()
            with connection:
                connection.sendall(b'forwarded\n')
finally:
    listener.close()
    if os.path.exists(path):
        os.unlink(path)
"#,
        )
        .unwrap();
        fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
        let command = |role: &str| {
            let mut command = Command::new(std::env::current_exe().unwrap());
            command
                .args([
                    "--exact",
                    "remote::tests::tunnel_process_fixture",
                    "--ignored",
                    "--nocapture",
                ])
                .env("JAM_STATE_DIR", &state)
                .env("JAM_TUNNEL_TEST_DIR", directory)
                .env("JAM_TUNNEL_TEST_ENDPOINT", &endpoint)
                .env("JAM_TUNNEL_TEST_ROLE", role);
            command
        };
        let mut gui = KillOnDrop(
            command("gui")
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .spawn()
                .unwrap(),
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        let report = directory.join("gui.path");
        while !report.exists() {
            assert!(
                gui.0.try_wait().unwrap().is_none(),
                "GUI transport fixture exited early"
            );
            assert!(
                std::time::Instant::now() < deadline,
                "GUI fixture did not connect"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let gui_socket = PathBuf::from(fs::read_to_string(report).unwrap());
        let forwarded = || {
            let mut text = String::new();
            let mut connection = UnixStream::connect(&gui_socket).unwrap();
            connection
                .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                .unwrap();
            connection.read_to_string(&mut text).unwrap();
            assert_eq!(text, "forwarded\n");
        };
        forwarded();
        let cli = command("cli").output().unwrap();
        assert!(
            cli.status.success(),
            "CLI fixture: {}",
            String::from_utf8_lossy(&cli.stderr)
        );
        let cli_socket = PathBuf::from(fs::read_to_string(directory.join("cli.path")).unwrap());
        assert_ne!(
            cli_socket, gui_socket,
            "Processes must never share a forwarding pathname"
        );
        assert!(gui_socket.as_os_str().len() < 100);
        assert!(cli_socket.as_os_str().len() < 100);
        assert!(!cli_socket.exists());
        assert!(
            !cli_socket.parent().unwrap().exists(),
            "Normal CLI exit cleans its private namespace"
        );
        forwarded();
        assert!(gui.0.try_wait().unwrap().is_none());
        fs::write(directory.join("gui.stop"), "stop").unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            if let Some(status) = gui.0.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "GUI fixture did not stop"
            );
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(!gui_socket.parent().unwrap().exists());
    }
    #[test]
    fn identifiers_cannot_escape_socket_directory() {
        assert!(local_socket("../../somewhere").is_err());
        assert!(local_socket(&crate::id()).unwrap().as_os_str().len() < 100);
    }
}

/// Qualify only object references, never user text, paths, titles or names.
pub fn scope(value: &mut serde_json::Value, endpoint: &str, encode: bool) -> Result<()> {
    fn walk(v: &mut serde_json::Value, key: &str, endpoint: &str, encode: bool) -> Result<()> {
        match v {
            serde_json::Value::Object(map) => {
                for (key, value) in map {
                    walk(value, key, endpoint, encode)?;
                }
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    walk(value, key, endpoint, encode)?;
                }
            }
            serde_json::Value::String(text)
                if [
                    "id",
                    "pane",
                    "workspace",
                    "tab",
                    "Leaf",
                    "active_pane",
                    "parent",
                    "workspaces",
                    "agent",
                    "owner",
                    "to",
                    "from",
                    "task",
                    "assignee",
                    "root",
                    "review_of",
                    "cancelled_with",
                ]
                .contains(&key) =>
            {
                if encode && uuid::Uuid::parse_str(text).is_ok() {
                    *text = format!("{endpoint}|{text}");
                } else if !encode && let Some((prefix, id)) = text.split_once('|') {
                    if prefix != endpoint {
                        bail!("Cannot move terminals between machines");
                    }
                    uuid::Uuid::parse_str(id)?;
                    *text = id.into();
                }
            }
            _ => {}
        }
        Ok(())
    }
    walk(value, "", endpoint, encode)
}
pub fn request_endpoint(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => map.iter().find_map(|(key, value)| {
            if [
                "id",
                "pane",
                "workspace",
                "tab",
                "Leaf",
                "active_pane",
                "agent",
                "owner",
                "task",
                "to",
            ]
            .contains(&key.as_str())
            {
                value
                    .as_str()
                    .and_then(|s| s.split_once('|'))
                    .map(|(host, _)| host.into())
                    .or_else(|| request_endpoint(value))
            } else {
                request_endpoint(value)
            }
        }),
        serde_json::Value::Array(values) => values.iter().find_map(request_endpoint),
        _ => None,
    }
}

/// Setup is attempted only after SSH succeeds and the daemon socket is absent.
static PERSISTENCE: std::sync::LazyLock<Mutex<HashMap<String, bool>>> =
    std::sync::LazyLock::new(|| Mutex::new(HashMap::new()));
pub fn ensure(id: &str, artifacts: &std::path::Path) -> Result<(PathBuf, bool)> {
    let lock = lifecycle_lock(id);
    let _guard = lock.lock().unwrap();

    let remote = list()?
        .into_iter()
        .find(|r| r.id == id)
        .context("Unknown remote")?;
    let script = "set -eu; uname -s; uname -m; s=\"${XDG_RUNTIME_DIR:-/run/user/$(id -u)}/jam-$(id -u)/jamd.sock\"; if test -S \"$s\"; then echo installed; else echo missing; fi; loginctl show-user \"$(id -u)\" -p Linger --value; p=\"${XDG_DATA_HOME:-$HOME/.local/share}/jam/servers/current/jamd\"; if test -x \"$p\"; then \"$p\" --build-info; else echo null; fi; m=\"${s%/*}/release.json\"; if test -f \"$m\"; then cat \"$m\"; echo; else echo null; fi";
    let output = ssh(&remote.destination)?
        .arg(&remote.destination)
        .arg(script)
        .output()?;
    if !output.status.success() {
        bail!(
            "SSH setup check: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    let text = String::from_utf8(output.stdout)?;
    let fields: Vec<_> = text.lines().collect();
    if fields.len() != 6 || fields[0] != "Linux" || !["x86_64", "aarch64"].contains(&fields[1]) {
        bail!("This remote requires Linux x86_64 or aarch64 with systemd");
    }
    let installed: serde_json::Value = serde_json::from_str(fields[4])
        .context("Invalid remote release metadata; server left unchanged")?;
    if !installed.is_null() {
        crate::release_version::require_client_at_least(
            installed["release_version"].as_str(),
            crate::RELEASE_VERSION,
        )?;
    } else if fields[2] != "missing" {
        anyhow::bail!("Running server has no release metadata; explicit migration is required");
    }
    let running: serde_json::Value = serde_json::from_str(fields[5])
        .context("Invalid running server metadata; server left unchanged")?;
    if !running.is_null() {
        crate::release_version::require_client_at_least(
            running["release_version"].as_str(),
            crate::RELEASE_VERSION,
        )?;
    }
    if fields[2] == "missing" {
        let archive = artifacts.join(format!("{}.tar.gz", fields[1]));
        if !archive.is_file() {
            bail!(
                "Missing {} server artifact; run just remote-build and rebuild the app",
                fields[1]
            );
        }
        if let Err(error) = install_unlocked(id, &archive, false, None, None).and_then(|done| {
            if done {
                Ok(())
            } else {
                bail!("Running sessions prevent the server update")
            }
        }) && !error
            .to_string()
            .contains("Server installed. Persistence needs:")
        {
            return Err(error);
        }
    }
    let persistent = fields[3] == "yes";
    let path = connect(id)?;
    let mut probe = std::os::unix::net::UnixStream::connect(&path)?;
    probe.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    if !matches!(read_frame::<Message>(&mut probe)?, Message::Hello(ref version) if version == VERSION)
    {
        bail!(
            "Server protocol differs from this client; explicitly upgrade the older component. Server left unchanged"
        );
    }
    PERSISTENCE.lock().unwrap().insert(id.into(), persistent);
    Ok((path, persistent))
}

#[cfg(test)]
mod routing_tests {
    use super::*;
    #[test]
    fn initial_tab_request_and_response_keep_remote_identity() {
        let host = crate::id();
        let workspace = crate::id();
        let agent = crate::id();
        let request = Request::CreateWorkspaceWithTab {
            location: WorkspaceLocation::Worktree {
                workspace: workspace.clone(),
                branch: "feature/test".into(),
                base: "HEAD".into(),
                path: Some("/tmp/checkouts/test".into()),
            },
            tab: InitialTab::Agent {
                provider: "claude".into(),
                role: "worker".into(),
                complex: false,
                parent: Some(agent.clone()),
                name: Some("Implementation".into()),
            },
        };
        let original = serde_json::to_value(request).unwrap();
        let mut scoped = original.clone();
        scope(&mut scoped, &host, true).unwrap();
        assert_eq!(
            scoped["CreateWorkspaceWithTab"]["location"]["Worktree"]["workspace"],
            format!("{host}|{workspace}")
        );
        assert_eq!(
            scoped["CreateWorkspaceWithTab"]["tab"]["Agent"]["parent"],
            format!("{host}|{agent}")
        );
        scope(&mut scoped, &host, false).unwrap();
        assert_eq!(scoped, original);
        let reply = Message::CreatedWorkspace {
            workspace,
            tab: Some(crate::id()),
            pane: crate::id(),
            agent: Some(agent),
        };
        let original = serde_json::to_value(reply).unwrap();
        let mut scoped = original.clone();
        scope(&mut scoped, &host, true).unwrap();
        for key in ["workspace", "tab", "pane", "agent"] {
            assert_eq!(
                scoped["CreatedWorkspace"][key],
                format!(
                    "{host}|{}",
                    original["CreatedWorkspace"][key].as_str().unwrap()
                )
            );
        }
        scope(&mut scoped, &host, false).unwrap();
        assert_eq!(scoped, original);
    }
    #[test]
    fn round_trip_and_cross_host_rejection() {
        let id = crate::id();
        let host = crate::id();
        let original = serde_json::json!({"State":{"workspaces":[{"id":id,"parent":id,"name":id,"path":id,"tabs":[{"id":id,"root":{"Leaf":id}}]}]}});
        let mut value = original.clone();
        scope(&mut value, &host, true).unwrap();
        assert_eq!(value["State"]["workspaces"][0]["name"], id);
        assert_eq!(value["State"]["workspaces"][0]["path"], id);
        assert_eq!(
            value["State"]["workspaces"][0]["parent"],
            format!("{host}|{id}")
        );
        assert_eq!(
            value["State"]["workspaces"][0]["id"],
            format!("{host}|{id}")
        );
        assert!(scope(&mut value.clone(), "another-host", false).is_err());
        scope(&mut value, &host, false).unwrap();
        assert_eq!(value, original);
    }
}

pub fn enable_persistence(id: &str) -> Result<()> {
    let remote = list()?
        .into_iter()
        .find(|r| r.id == id)
        .context("Unknown remote")?;
    let result = ssh(&remote.destination)?
        .arg(&remote.destination)
        .arg("loginctl --no-ask-password enable-linger $(id -u)")
        .output()?;
    if !result.status.success() {
        bail!(
            "An administrator must enable lingering for this user: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        );
    }
    PERSISTENCE.lock().unwrap().insert(id.into(), true);
    Ok(())
}

pub fn rename(id: &str, name: String) -> Result<()> {
    let _lock = registry_lock()?;
    let mut entries = list()?;
    if name.trim().is_empty() || entries.iter().any(|r| r.id != id && r.name == name) {
        bail!("Remote names must be nonempty and unique");
    }
    entries
        .iter_mut()
        .find(|r| r.id == id)
        .context("Unknown remote")?
        .name = name;
    save(&entries)
}
