//! Exercise real planner/helper call paths in separate managed processes with
//! private Unix-kernel fixtures. No SSH host, install or running user daemon.
use super::*;
use std::os::unix::{fs::PermissionsExt, net::UnixListener};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

#[test]
#[ignore = "private managed updater subprocess fixture"]
fn managed_remote_fixture() {
    let directory = PathBuf::from(std::env::var("JAM_UPGRADE_REMOTE_FIXTURE").unwrap());
    let role = std::env::var("JAM_UPGRADE_REMOTE_ROLE").unwrap();
    let endpoint = std::env::var("JAM_UPGRADE_REMOTE_ENDPOINT").unwrap();
    crate::remote::with_test_ssh(directory.join("ssh-proxy"), || {
        assert!(!crate::remote::local_socket(&endpoint).unwrap().exists());
        if role == "gui" {
            let socket = crate::remote::connect(&endpoint).unwrap();
            fs::write(directory.join("gui.path"), socket.to_str().unwrap()).unwrap();
            let deadline = std::time::Instant::now() + Duration::from_secs(60);
            while !directory.join("gui.stop").exists() {
                assert!(std::time::Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            }
        } else if role == "planner" {
            let staged: Staged =
                serde_json::from_slice(&fs::read(directory.join("staged.json")).unwrap()).unwrap();
            // Public plan() runs managed-install and free-space checks too.
            let planned = plan(&staged).unwrap();
            assert_eq!(
                planned.targets.len(),
                1,
                "Reachable remote must not be marked offline in a fresh CLI"
            );
            assert!(planned.offline.is_empty());
            assert_eq!(planned.targets[0].endpoint, endpoint);
            fs::write(
                directory.join("plan.json"),
                serde_json::to_vec(&planned).unwrap(),
            )
            .unwrap();
        } else if role == "preapply-offline" {
            let mut planned: Plan =
                serde_json::from_slice(&fs::read(directory.join("plan.json")).unwrap()).unwrap();
            revalidate_plan_targets(&mut planned).unwrap();
            assert!(planned.targets.is_empty());
            assert_eq!(planned.offline, vec!["Fixture remote"]);
            fs::write(
                directory.join("offline-plan.json"),
                serde_json::to_vec(&planned).unwrap(),
            )
            .unwrap();
        } else if role == "restore-after-offline" {
            let planned: Plan =
                serde_json::from_slice(&fs::read(directory.join("offline-plan.json")).unwrap())
                    .unwrap();
            let staged: Staged =
                serde_json::from_slice(&fs::read(directory.join("staged.json")).unwrap()).unwrap();
            let journal = Journal {
                schema: 1,
                token: crate::id(),
                staged,
                plan: planned,
                phase: "local_verified".into(),
                error: None,
                environment: Default::default(),
            };
            let before = fs::read(directory.join("ssh.log")).unwrap();
            let pending = restore_remotes(&journal);
            assert_eq!(pending.len(), 1);
            assert!(pending[0].contains("Offline machines require confirmation"));
            assert_eq!(
                fs::read(directory.join("ssh.log")).unwrap(),
                before,
                "A newly-online remote removed from consent must not reconnect or install during restore"
            );
        } else {
            let planned: Plan =
                serde_json::from_slice(&fs::read(directory.join("plan.json")).unwrap()).unwrap();
            let mut approved = planned.targets[0].clone();
            if role == "preapply-changed" {
                let error = revalidate_remote_target(&mut approved).unwrap_err();
                assert!(error.to_string().contains("Sessions changed"));
            } else if role == "preapply" {
                let path = revalidate_remote_target(&mut approved)
                    .unwrap()
                    .expect("Fresh helper must establish its own forwarding socket");
                assert_eq!(path, crate::remote::local_socket(&endpoint).unwrap());
            } else {
                assert_eq!(role, "restore");
                // These are the same functions used by restore_inner before
                // admitting a remote installation; stop before mutation.
                let (_, current) =
                    connect_remote_target(&approved.endpoint, &approved.name).unwrap();
                revalidate_target(&mut approved, &current).unwrap();
            }
            assert_eq!(approved.panes.len(), 1);
        }
        let path = crate::remote::local_socket(&endpoint).unwrap();
        assert_eq!(
            path.exists(),
            !matches!(role.as_str(), "preapply-offline" | "restore-after-offline")
        );
        fs::write(
            directory.join(format!("{role}.path")),
            path.to_str().unwrap(),
        )
        .unwrap();
        crate::remote::disconnect(&endpoint);
    });
}

struct ChildGuard(std::process::Child);
impl Drop for ChildGuard {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct KernelGuard {
    stop: Arc<AtomicBool>,
    worker: Option<std::thread::JoinHandle<()>>,
}
impl Drop for KernelGuard {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        self.worker.take().unwrap().join().unwrap();
    }
}

#[test]
fn managed_planner_and_helpers_connect_independently_before_probing() {
    let fixture = tempfile::Builder::new()
        .prefix("jam-upgrade-rpc-")
        .tempdir_in("/tmp")
        .unwrap();
    let directory = fixture.path();
    let state = directory.join("state");
    crate::server::private_dir(&state).unwrap();
    let endpoint = crate::id();
    fs::write(
        state.join("remotes.json"),
        serde_json::to_vec(&vec![crate::remote::Remote {
            id: endpoint.clone(),
            name: "Fixture remote".into(),
            destination: "fixture-only".into(),
        }])
        .unwrap(),
    )
    .unwrap();
    let generation = Arc::new(AtomicU64::new(1));
    let requests = Arc::new(Mutex::new(Vec::new()));
    let stop = Arc::new(AtomicBool::new(false));
    let kernel = directory.join("kernel.sock");
    let listener = UnixListener::bind(&kernel).unwrap();
    listener.set_nonblocking(true).unwrap();
    let server_generation = generation.clone();
    let recorded = requests.clone();
    let server_stop = stop.clone();
    let _kernel = KernelGuard {
        stop,
        worker: Some(std::thread::spawn(move || {
            while !server_stop.load(Ordering::SeqCst) {
                let (mut stream, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(5));
                        continue;
                    }
                    Err(error) => panic!("Fixture accept: {error}"),
                };
                stream.set_nonblocking(false).unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                if crate::write_frame(
                    &mut stream,
                    &crate::Message::Hello("jam/999:read-only-fixture".into()),
                )
                .is_err()
                {
                    continue;
                }
                let request = match crate::read_frame::<crate::Request>(&mut stream) {
                    Ok(value) => value,
                    Err(_) => continue,
                };
                recorded.lock().unwrap().push(format!("{request:?}"));
                let reply = match request {
                    crate::Request::HostInfo => crate::Message::HostInfo {
                        home: "/fixture".into(),
                        protocol: "jam/999:read-only-fixture".into(),
                        build: "remote-fixture-build".into(),
                        engine: "fixture".into(),
                        incarnation: "remote-fixture-incarnation".into(),
                        release_version: Some(crate::RELEASE_VERSION.into()),
                    },
                    crate::Request::State => crate::Message::State(crate::State {
                        workspace_model: 1,
                        revision: 1,
                        workspaces: vec![],
                        panes: vec![crate::Pane {
                            id: "fixture-pane".into(),
                            session_generation: Some(format!(
                                "session-{}",
                                server_generation.load(Ordering::SeqCst)
                            )),
                            title: "Fixture".into(),
                            cwd: "/fixture".into(),
                            status: "idle".into(),
                            exited: false,
                            workspace: "fixture-workspace".into(),
                            reports: vec![],
                            launch_error: None,
                            agent_provider: None,
                            agent_source: None,
                            agent_role: None,
                        }],
                    }),
                    other => panic!("Unapproved fixture mutation: {other:?}"),
                };
                crate::write_frame(&mut stream, &reply).unwrap();
            }
        })),
    };
    let proxy = directory.join("ssh-proxy");
    fs::write(
        &proxy,
        r#"#!/usr/bin/env python3
import json, os, select, socket, sys, threading
with open(os.environ['JAM_UPGRADE_REMOTE_LOG'], 'a') as log:
    log.write(json.dumps(sys.argv[1:]) + '\n')
if os.environ.get('JAM_UPGRADE_REMOTE_ROLE') == 'preapply-offline':
    sys.exit(255)
if '-L' not in sys.argv:
    print(os.environ['JAM_UPGRADE_REMOTE_KERNEL'])
    sys.exit(0)
local, target = sys.argv[sys.argv.index('-L') + 1].split(':', 1)
listener = socket.socket(socket.AF_UNIX)
listener.bind(local)
listener.listen(16)
def forward(client):
    remote = socket.socket(socket.AF_UNIX)
    try:
        remote.connect(target)
        while True:
            ready, _, _ = select.select([client, remote], [], [], 5)
            for source in ready:
                data = source.recv(65536)
                if not data:
                    return
                (remote if source is client else client).sendall(data)
    finally:
        client.close()
        remote.close()
try:
    while True:
        ready, _, _ = select.select([listener, sys.stdin], [], [], 1)
        if sys.stdin in ready and not sys.stdin.read(1):
            break
        if listener in ready:
            client, _ = listener.accept()
            threading.Thread(target=forward, args=(client,), daemon=True).start()
finally:
    listener.close()
    if os.path.exists(local):
        os.unlink(local)
"#,
    )
    .unwrap();
    fs::set_permissions(&proxy, fs::Permissions::from_mode(0o700)).unwrap();
    let executable = if cfg!(target_os = "macos") {
        let app = directory.join("Jam.app");
        let cli = app.join("Contents/Resources/bin/jam");
        fs::create_dir_all(cli.parent().unwrap()).unwrap();
        fs::create_dir_all(app.join("Contents/MacOS")).unwrap();
        fs::copy(std::env::current_exe().unwrap(), &cli).unwrap();
        fs::copy("/usr/bin/true", app.join("Contents/MacOS/Jam")).unwrap();
        fs::write(app.join("Contents/Info.plist"), format!(r#"<?xml version="1.0"?><plist version="1.0"><dict><key>CFBundleIdentifier</key><string>dev.jam.remote-upgrade-fixture</string><key>CFBundleExecutable</key><string>Jam</string><key>CFBundlePackageType</key><string>APPL</string><key>CFBundleVersion</key><string>1</string><key>CFBundleShortVersionString</key><string>{}</string></dict></plist>"#, crate::RELEASE_VERSION)).unwrap();
        fs::write(
            app.join("Contents/Resources/release.json"),
            serde_json::to_vec(&serde_json::json!({"version":crate::RELEASE_VERSION})).unwrap(),
        )
        .unwrap();
        for target in [&cli, &app.join("Contents/MacOS/Jam"), &app] {
            let output = Command::new("/usr/bin/codesign")
                .args(["--force", "--sign", "-"])
                .arg(target)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        assert!(
            Command::new("/usr/bin/codesign")
                .args(["--verify", "--deep", "--strict"])
                .arg(&app)
                .status()
                .unwrap()
                .success()
        );
        cli
    } else {
        let base = directory.join("data/jam/servers");
        let build = base.join("a".repeat(24));
        fs::create_dir_all(&build).unwrap();
        std::os::unix::fs::symlink(build.file_name().unwrap(), base.join("current")).unwrap();
        let cli = build.join("jam");
        fs::copy(std::env::current_exe().unwrap(), &cli).unwrap();
        cli
    };
    let staged_directory = directory.join("stage");
    fs::create_dir_all(staged_directory.join("extracted")).unwrap();
    let staged = Staged {
        directory: staged_directory.clone(),
        archive: staged_directory.join("unused.zip"),
        release: Release {
            schema: 1,
            version: "99.0.0".into(),
            protocol: crate::VERSION.into(),
            min_macos: "14.0".into(),
            assets: vec![],
        },
    };
    fs::write(
        directory.join("staged.json"),
        serde_json::to_vec(&staged).unwrap(),
    )
    .unwrap();
    let command = |role: &str| {
        let mut command = Command::new(&executable);
        command
            .args([
                "--exact",
                "upgrade::remote_tests::managed_remote_fixture",
                "--ignored",
                "--nocapture",
            ])
            .env("JAM_STATE_DIR", &state)
            .env("JAM_SOCKET", directory.join("unused-local.sock"))
            .env("HOME", directory)
            .env("XDG_DATA_HOME", directory.join("data"))
            .env("JAM_UPGRADE_REMOTE_FIXTURE", directory)
            .env("JAM_UPGRADE_REMOTE_ROLE", role)
            .env("JAM_UPGRADE_REMOTE_ENDPOINT", &endpoint)
            .env("JAM_UPGRADE_REMOTE_KERNEL", &kernel)
            .env("JAM_UPGRADE_REMOTE_LOG", directory.join("ssh.log"));
        command
    };
    let mut gui = ChildGuard(
        command("gui")
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let deadline = std::time::Instant::now() + Duration::from_secs(15);
    while !directory.join("gui.path").exists() {
        assert!(
            gui.0.try_wait().unwrap().is_none(),
            "GUI fixture exited before connection"
        );
        assert!(
            std::time::Instant::now() < deadline,
            "GUI transport fixture timed out"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
    let gui_socket = PathBuf::from(fs::read_to_string(directory.join("gui.path")).unwrap());
    let initial_probe = probe(&gui_socket, crate::Request::HostInfo);
    assert!(
        initial_probe.is_ok(),
        "GUI probe: {initial_probe:?}; SSH log: {}",
        fs::read_to_string(gui_socket.with_extension("log")).unwrap_or_default()
    );
    let mut owned_paths = vec![gui_socket.clone()];
    for role in [
        "planner",
        "preapply",
        "preapply-changed",
        "restore",
        "preapply-offline",
        "restore-after-offline",
    ] {
        generation.store(
            if role == "preapply-changed" { 2 } else { 1 },
            Ordering::SeqCst,
        );
        let output = command(role).output().unwrap();
        assert!(
            output.status.success(),
            "{role}: {}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        let own_path =
            PathBuf::from(fs::read_to_string(directory.join(format!("{role}.path"))).unwrap());
        assert!(!owned_paths.contains(&own_path));
        assert!(
            !own_path.exists(),
            "Helper exit must clean only its private forwarding socket"
        );
        owned_paths.push(own_path);
        assert!(
            probe(&gui_socket, crate::Request::HostInfo).is_ok(),
            "{role} disturbed GUI forwarding"
        );
    }
    fs::write(directory.join("gui.stop"), "stop").unwrap();
    assert!(gui.0.wait().unwrap().success());
    assert!(!gui_socket.exists());
    let operations = requests.lock().unwrap();
    assert!(
        operations
            .iter()
            .filter(|request| request.as_str() == "State")
            .count()
            >= 4
    );
    assert!(
        operations
            .iter()
            .all(|request| ["State", "HostInfo"].contains(&request.as_str()))
    );
    for line in fs::read_to_string(directory.join("ssh.log"))
        .unwrap()
        .lines()
    {
        let arguments: Vec<String> = serde_json::from_str(line).unwrap();
        let text = arguments.join(" ");
        for forbidden in [
            "install.py",
            "prepare-upgrade",
            "stop-upgrade",
            "--restart-active",
            "tar -xz",
        ] {
            assert!(
                !text.contains(forbidden),
                "Planning/revalidation performed an unapproved remote mutation: {text}"
            );
        }
    }
}
