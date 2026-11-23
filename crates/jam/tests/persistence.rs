use jam_core::*;
use std::{
    os::unix::{fs::PermissionsExt, net::UnixStream},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};
struct Daemon {
    child: Child,
    dir: tempfile::TempDir,
    socket: std::path::PathBuf,
}
impl Daemon {
    fn start() -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = dir.path().join("jamd.sock");
        let child = Self::launch(&socket, dir.path());
        let d = Self { child, dir, socket };
        d.until(|| {
            matches!(
                call_at(&d.socket, Request::HostInfo),
                Ok(Message::HostInfo { .. })
            )
        });
        d
    }
    fn launch(socket: &std::path::Path, dir: &std::path::Path) -> Child {
        Command::new(env!("CARGO_BIN_EXE_jamd"))
            .env("JAM_SOCKET", socket)
            .env("JAM_STATE_DIR", dir.join("state"))
            .env("TMPDIR", dir)
            .env("SHELL", "/bin/sh")
            .spawn()
            .unwrap()
    }
    #[track_caller]
    fn until(&self, mut check: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !check() {
            assert!(Instant::now() < deadline, "daemon operation timed out");
            thread::sleep(Duration::from_millis(20));
        }
    }
    fn connect(&self) -> UnixStream {
        connect_to(&self.socket).unwrap()
    }
    fn request(&self, r: Request) -> Message {
        let mut s = self.connect();
        write_frame(&mut s, &r).unwrap();
        read_frame(&mut s).unwrap_or_else(|e| panic!("request {r:?}: {e}"))
    }
    fn state(&self) -> State {
        match self.request(Request::State) {
            Message::State(s) => s,
            m => panic!("{m:?}"),
        }
    }
    fn send(&self, pane: &str, text: &str) {
        assert!(matches!(
            self.request(Request::Send {
                pane: pane.into(),
                data: text.as_bytes().to_vec()
            }),
            Message::Ok
        ));
    }
    fn capture(&self, pane: &str) -> String {
        match self.request(Request::Capture { pane: pane.into() }) {
            Message::Capture(s) => s,
            m => panic!("{m:?}"),
        }
    }
    fn create(&self) -> State {
        match self.request(Request::CreateWorkspace {
            path: self.dir.path().display().to_string(),
            name: "Integration".into(),
        }) {
            Message::State(s) => s,
            m => panic!("{m:?}"),
        }
    }
    fn pid(&self, pane: &str) -> u32 {
        self.send(pane, "printf '\\120\\111\\104:%s\\n' \"$$\"\r");
        let mut pid = None;
        self.until(|| {
            pid = self
                .capture(pane)
                .lines()
                .filter_map(|l| l.split_once("PID:")?.1.trim().parse::<u32>().ok())
                .next_back();
            pid.is_some()
        });
        pid.unwrap()
    }
}
impl Drop for Daemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[test]
fn unresponsive_local_server_reports_inspection_failure_without_takeover() {
    let daemon = Daemon::start();
    let state = daemon.create();
    let Message::HostInfo { incarnation, .. } = daemon.request(Request::HostInfo) else {
        panic!("expected the initial daemon identity");
    };
    let pid = daemon.child.id().to_string();
    let signal = |name| {
        assert!(
            Command::new("/bin/kill")
                .args([name, &pid])
                .status()
                .unwrap()
                .success()
        );
    };
    signal("-STOP");
    let request = serde_json::json!({"action":"status", "daemon":env!("CARGO_BIN_EXE_jamd")});
    let result =
        server_control::manage_at(&request, &daemon.socket, &daemon.dir.path().join("state"));
    signal("-CONT");
    let status = result.unwrap();
    assert_eq!(status["state"], "inspection_failed", "{status}");
    assert_eq!(status["management_available"], false, "{status}");
    assert_eq!(status["failure"]["retryable"], true, "{status}");
    assert_ne!(status["can_takeover"], true, "{status}");
    assert!(
        status["error"]
            .as_str()
            .unwrap()
            .contains("server is not responding")
    );
    let Message::HostInfo {
        incarnation: resumed,
        ..
    } = daemon.request(Request::HostInfo)
    else {
        panic!("expected the resumed daemon identity");
    };
    assert_eq!(resumed, incarnation);
    let resumed = daemon.state();
    assert_eq!(resumed.panes[0].id, state.panes[0].id);
    assert_eq!(
        resumed.panes[0].session_generation,
        state.panes[0].session_generation
    );
    assert!(!resumed.panes[0].exited);
}

#[test]
fn a_blocked_coordination_write_does_not_block_health_or_terminal_input() {
    assert!(rusqlite::version_number() >= 3_051_003);
    let daemon = Daemon::start();
    let pane = daemon.create().panes[0].id.clone();
    let connection =
        rusqlite::Connection::open(daemon.dir.path().join("state/coordination.sqlite")).unwrap();
    let owner = id();
    let agent =
        serde_json::json!({"id": owner, "role": "coordinator", "provider": "codex", "root": id()});
    connection
        .execute(
            "INSERT INTO records(kind, id, value) VALUES ('agent', ?1, ?2)",
            rusqlite::params![owner, agent.to_string()],
        )
        .unwrap();
    connection.execute_batch("BEGIN IMMEDIATE").unwrap();
    let mut pending = daemon.connect();
    write_request(
        &mut pending,
        &Request::Coordination(
            serde_json::json!({"action":"task","owner":owner,"objective":"test"}),
        ),
    )
    .unwrap();
    pending
        .set_read_timeout(Some(Duration::from_millis(150)))
        .unwrap();
    assert!(
        read_frame::<Message>(&mut pending)
            .unwrap_err()
            .downcast_ref::<std::io::Error>()
            .is_some_and(|e| matches!(
                e.kind(),
                std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
            ))
    );
    let start = Instant::now();
    assert!(matches!(
        daemon.request(Request::HostInfo),
        Message::HostInfo { .. }
    ));
    assert_eq!(daemon.state().panes.len(), 1);
    daemon.send(&pane, "printf '%s%s\\n' JAM_ READY\r");
    daemon.until(|| {
        daemon
            .capture(&pane)
            .lines()
            .any(|line| line.trim() == "JAM_READY")
    });
    assert!(start.elapsed() < Duration::from_secs(1));
    connection.execute_batch("ROLLBACK").unwrap();
    pending
        .set_read_timeout(Some(Duration::from_secs(3)))
        .unwrap();
    let reply = read_frame::<Message>(&mut pending).unwrap();
    assert!(
        matches!(&reply, Message::Coordination(task) if task["state"] == "pending" && task["owner"] == owner),
        "{reply:?}"
    );
}

#[test]
fn retrying_workspace_creation_returns_the_same_workspace_and_pane() {
    let daemon = Daemon::start();
    let Message::HostInfo { incarnation, .. } = daemon.request(Request::HostInfo) else {
        panic!()
    };
    let token = id();
    let request = Request::WorkspaceCreation {
        id: token.clone(),
        incarnation,
        request: Box::new(Request::NewWorkspace {
            path: daemon.dir.path().display().to_string(),
        }),
    };
    let mut lost = daemon.connect();
    write_request(&mut lost, &request).unwrap();
    drop(lost);
    daemon.until(|| matches!(daemon.request(request.clone()), Message::Operation { workspace: Some(id), done: true, error: None, .. } if id == token));
    let state = daemon.state();
    assert_eq!(state.workspaces.len(), 1);
    assert_eq!(state.panes.len(), 1);
    assert_eq!(state.workspaces[0].id, token);
}

#[test]
fn clipboard_images_are_stored_on_the_target_host_before_pasting() {
    let local = Daemon::start();
    let proxy = Daemon::start();
    let png = b"\x89PNG\r\n\x1a\nfixture";
    let encoded = "iVBORw0KGgpmaXh0dXJl";
    for (daemon, endpoint) in [(&local, ""), (&proxy, "image-proxy")] {
        let state = daemon.create();
        let pane = &state.panes[0].id;
        let scoped_pane = if endpoint.is_empty() {
            pane.clone()
        } else {
            format!("{endpoint}|{pane}")
        };
        let mut upload = serde_json::json!({"StoreClipboardImage": {
            "pane": scoped_pane, "png": encoded
        }});
        if !endpoint.is_empty() {
            assert_eq!(remote::request_endpoint(&upload).as_deref(), Some(endpoint));
            remote::scope(&mut upload, endpoint, false).unwrap();
        }
        let path = match daemon.request(serde_json::from_value(upload).unwrap()) {
            Message::ClipboardImage { path } => std::path::PathBuf::from(path),
            response => panic!("Unexpected upload reply: {response:?}"),
        };
        assert!(
            path.starts_with(daemon.dir.path()),
            "Image was saved on the wrong host"
        );
        assert_eq!(std::fs::read(&path).unwrap(), png);
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        daemon.send(pane, "test -r ");
        assert!(matches!(
            daemon.request(Request::Paste {
                pane: pane.clone(),
                text: format!("'{}' ", path.display())
            }),
            Message::Ok
        ));
        // A slow login shell can print its first prompt after the PTY echoes our
        // input. Start the probe output on its own line in either order.
        daemon.send(pane, "&& printf '\\n\\111\\115\\101\\107\\105_OK\\n'\r");
        daemon.until(|| {
            daemon
                .capture(pane)
                .lines()
                .any(|line| line.trim() == "IMAGE_OK")
        });
        assert!(matches!(
            daemon.request(Request::StoreClipboardImage {
                pane: "missing-pane".into(),
                png: encoded.into()
            }),
            Message::Error(_)
        ));
    }
}

#[test]
fn unverified_legacy_server_is_never_replaced_by_force() {
    let mut d = Daemon::start();
    let initial = d.create();
    let pane = initial.panes[0].id.clone();
    d.send(&pane, "cd /; printf '\\033]7;file://localhost/\\007'\r");
    d.until(|| d.state().panes[0].cwd == "/");
    let saved = d.state();
    assert!(matches!(d.request(Request::Stop), Message::Ok));
    assert!(d.child.wait().unwrap().success());

    // A wire-level v6 fixture exposes only the historical Hello/Stop/Ok
    // contract. Normal calls and unconfirmed updates must leave it untouched.
    let _ = std::fs::remove_file(&d.socket);
    let listener = std::os::unix::net::UnixListener::bind(&d.socket).unwrap();
    let socket = d.socket.clone();
    let legacy = thread::spawn(move || {
        loop {
            let (mut stream, _) = listener.accept().unwrap();
            if write_frame(
                &mut stream,
                &Message::Hello("jam/6:ghostty/483ea3d:text".into()),
            )
            .is_err()
            {
                continue; // Liveness probes close without reading a greeting.
            }
            match read_frame::<Request>(&mut stream) {
                Err(_) => continue,
                Ok(Request::Stop) => {
                    drop(listener);
                    std::fs::remove_file(socket).unwrap();
                    write_frame(&mut stream, &Message::Ok).unwrap();
                    return;
                }
                Ok(request) => {
                    panic!("Legacy daemon received an incompatible request: {request:?}")
                }
            }
        }
    });
    let mut request = serde_json::json!({"action":"status", "daemon":env!("CARGO_BIN_EXE_jamd")});
    let manage = |request: &serde_json::Value| {
        server_control::manage_at(request, &d.socket, &d.dir.path().join("state"))
    };
    let status = manage(&request).unwrap();
    assert_eq!(status["running"], true);
    assert_eq!(status["compatible"], false);
    assert_eq!(status["update_available"], true);
    request["action"] = "update".into();
    assert!(
        manage(&request).is_err(),
        "Unconfirmed update stopped the legacy daemon"
    );
    assert!(UnixStream::connect(&d.socket).is_ok());
    request["force"] = true.into();
    assert!(
        manage(&request).is_err(),
        "Force must not bypass ownership checks"
    );
    assert!(UnixStream::connect(&d.socket).is_ok());
    assert!(matches!(stop_at(&d.socket).unwrap(), Message::Ok));
    legacy.join().unwrap();
    let _ = saved;
}

#[test]
fn server_management_detects_builds_defers_active_sessions_and_restarts_explicitly() {
    let d = Daemon::start();
    let initial = d.create();
    let pane = initial.panes[0].id.clone();
    let pid = d.pid(&pane);
    // Management reads the same environment as a GUI; isolate its endpoint.
    let wrapper = d.dir.path().join("new-server");
    std::fs::write(
        &wrapper,
        "#!/bin/sh\nprintf '{\"build\":\"future-build\"}\\n'\n",
    )
    .unwrap();
    std::fs::set_permissions(&wrapper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut request = serde_json::json!({"action":"status","endpoint":"","daemon":wrapper});
    let manage = |request: &serde_json::Value| {
        jam_core::server_control::manage_at(request, &d.socket, &d.dir.path().join("state"))
            .unwrap()
    };
    let status = manage(&request);
    assert_eq!(status["compatible"], true);
    assert_eq!(
        status["update_available"], true,
        "compatible old builds must be detected"
    );
    assert_eq!(status["terminals"], 1);
    request["action"] = "update".into();
    assert!(
        jam_core::server_control::manage_at(&request, &d.socket, &d.dir.path().join("state"))
            .is_err(),
        "A different installation needs explicit takeover"
    );
    assert_eq!(d.pid(&pane), pid, "deferred update stopped a terminal");
    request["action"] = "restart".into();
    request["force"] = true.into();
    request["daemon"] = env!("CARGO_BIN_EXE_jamd").into();
    assert_eq!(manage(&request)["ok"], true);
    let restored = d.state();
    assert_eq!(initial.workspaces[0].id, restored.workspaces[0].id);
    assert_eq!(restored.panes[0].id, pane);
    assert!(
        restored.panes[0].exited,
        "server restart must not replay a command"
    );
    let _ = d.request(Request::Stop);
}

#[test]
fn worktree_membership_survives_directory_changes_and_is_container_scoped() {
    let mut d = Daemon::start();
    let home = d.dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    let repositories: Vec<_> = ["a", "b"]
        .into_iter()
        .map(|name| {
            let path = d.dir.path().join(name);
            std::fs::create_dir(&path).unwrap();
            for args in [
                vec!["init", "-b", "main"],
                vec![
                    "-c",
                    "user.name=Test",
                    "-c",
                    "user.email=test@example.invalid",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "initial",
                ],
            ] {
                assert!(
                    Command::new("git")
                        .arg("-C")
                        .arg(&path)
                        .args(args)
                        .output()
                        .unwrap()
                        .status
                        .success()
                );
            }
            std::fs::canonicalize(path).unwrap()
        })
        .collect();
    let Message::State(state) = d.request(Request::NewWorkspace {
        path: home.to_string_lossy().into_owned(),
    }) else {
        panic!("workspace")
    };
    let root = state.workspaces[0].id.clone();
    let pane = state.panes[0].id.clone();
    let cd = |path: &std::path::Path| {
        d.send(
            &pane,
            &format!(
                "cd '{}'; printf '\\033]7;file://localhost{}\\007'\r",
                path.display(),
                path.display()
            ),
        );
        d.until(|| {
            d.state()
                .workspaces
                .iter()
                .find(|w| w.id == root)
                .and_then(|w| w.context.as_ref())
                .is_some_and(|c| c.ready && c.path == path.to_string_lossy())
        });
    };
    let operation = |request| {
        let Message::Operation { id, .. } = d.request(request) else {
            panic!("operation")
        };
        let mut result = None;
        d.until(|| match d.request(Request::Operation { id: id.clone() }) {
            Message::Operation {
                done: true,
                error,
                workspace,
                ..
            } => {
                result = Some((workspace, error));
                true
            }
            _ => false,
        });
        result.unwrap()
    };
    let mut children = Vec::new();
    let mut paths = Vec::new();
    for (index, repo) in repositories.iter().enumerate() {
        cd(repo);
        let target = d.dir.path().join(format!("checkout-{index}"));
        let (child, error) = operation(Request::WorktreeCreate {
            name: String::new(),
            workspace: root.clone(),
            branch: "feature".into(),
            base: "HEAD".into(),
            path: Some(target.to_string_lossy().into_owned()),
        });
        assert!(error.is_none(), "{error:?}");
        children.push(child.unwrap());
        paths.push(target);
    }
    cd(&home);
    let state = d.state();
    assert!(
        state.workspaces[0]
            .context
            .as_ref()
            .unwrap()
            .repository
            .is_none()
    );
    for child in &children {
        assert_eq!(
            state
                .workspaces
                .iter()
                .find(|w| &w.id == child)
                .unwrap()
                .parent
                .as_deref(),
            Some(root.as_str())
        );
    }
    let first_pane = state
        .panes
        .iter()
        .find(|p| p.workspace == children[0])
        .unwrap()
        .id
        .clone();
    let first_pid = d.pid(&first_pane);
    let Message::State(other) = d.request(Request::NewWorkspace {
        path: repositories[0].to_string_lossy().into_owned(),
    }) else {
        panic!("other")
    };
    let other = other.workspaces.last().unwrap().id.clone();
    let (copy, error) = operation(Request::WorktreeOpen {
        path: paths[0].to_string_lossy().into_owned(),
        workspace: Some(other.clone()),
    });
    assert!(error.is_none());
    let copy = copy.unwrap();
    assert_ne!(copy, children[0]);
    let state = d.state();
    let copy_pane = state
        .panes
        .iter()
        .find(|p| p.workspace == copy)
        .unwrap()
        .id
        .clone();
    assert_ne!(d.pid(&copy_pane), first_pid);
    let (same, error) = operation(Request::WorktreeOpen {
        path: paths[0].to_string_lossy().into_owned(),
        workspace: Some(other.clone()),
    });
    assert!(error.is_none());
    assert_eq!(same.as_deref(), Some(copy.as_str()));
    let (_, error) = operation(Request::WorktreeRemove {
        workspace: children[0].clone(),
        force: true,
    });
    assert!(error.unwrap().contains("another workspace"));
    assert_eq!(d.pid(&first_pane), first_pid);
    assert!(matches!(
        d.request(Request::WorkspaceClose {
            workspace: other,
            group: true
        }),
        Message::State(_)
    ));
    assert_eq!(d.state().workspaces.len(), 3);
    let _ = d.request(Request::Stop);
    d.child.wait().unwrap();
    let _ = std::fs::remove_file(&d.socket);
    d.child = Daemon::launch(&d.socket, d.dir.path());
    d.until(|| std::os::unix::net::UnixStream::connect(&d.socket).is_ok());
    let state = d.state();
    assert_eq!(state.workspace_model, 1);
    assert_eq!(state.workspaces.len(), 3);
    assert_eq!(
        state
            .workspaces
            .iter()
            .filter(|w| w.parent.as_ref() == Some(&root))
            .count(),
        2
    );
    assert!(paths.iter().all(|p| p.exists()));
}
#[test]
fn persistence_layout_and_multiple_gui_clients() {
    let mut d = Daemon::start();
    let state = d.create();
    let pane = state.panes[0].id.clone();
    let pid = d.pid(&pane);
    let mut gui = d.connect();
    write_frame(&mut gui, &Request::AcquireGui).unwrap();
    assert!(matches!(
        read_frame::<Message>(&mut gui).unwrap(),
        Message::Ok
    ));
    assert!(matches!(d.request(Request::AcquireGui), Message::Ok));
    for _ in 0..2 {
        let mut s = d.connect();
        write_frame(&mut s, &Request::Attach { pane: pane.clone() }).unwrap();
        assert!(matches!(
            read_frame::<Message>(&mut s).unwrap(),
            Message::Snapshot { .. }
        ));
    }
    assert_eq!(pid, d.pid(&pane));
    drop(gui);
    assert!(matches!(
        d.request(Request::Status {
            pane: pane.clone(),
            state: "blocked".into()
        }),
        Message::Ok
    ));
    assert_eq!(d.state().panes[0].status, "blocked");
    let Message::State(split) = d.request(Request::Split {
        pane: pane.clone(),
        vertical: false,
    }) else {
        panic!()
    };
    let tab = &split.workspaces[0].tabs[0].id;
    assert!(matches!(
        d.request(Request::Ratio {
            tab: tab.clone(),
            path: vec![],
            ratio: 0.7
        }),
        Message::State(_)
    ));
    assert!(matches!(
        d.request(Request::Rename {
            id: tab.clone(),
            name: "Saved tab".into()
        }),
        Message::State(_)
    ));
    assert!(matches!(
        d.request(Request::Close {
            pane: split.panes[1].id.clone()
        }),
        Message::State(_)
    ));
    assert_eq!(d.state().panes.len(), 1);
    assert!(matches!(
        d.state().workspaces[0].tabs[0].root,
        Node::Leaf(_)
    ));
    assert!(matches!(d.request(Request::Stop), Message::Ok));
    d.child.wait().unwrap();
    // Simulate a v1 layout on disk; v2 must preserve IDs and add metadata safely.
    let layout = d.dir.path().join("state/layout.json");
    let mut old: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&layout).unwrap()).unwrap();
    for workspace in old["workspaces"].as_array_mut().unwrap() {
        let w = workspace.as_object_mut().unwrap();
        w.remove("repository");
        w.remove("main_path");
        w.remove("branch");
    }
    std::fs::write(&layout, serde_json::to_vec(&old).unwrap()).unwrap();
    d.child = Daemon::launch(&d.socket, d.dir.path());
    d.until(|| connect_to(&d.socket).is_ok());
    let restored = d.state();
    assert!(d.dir.path().join("state/layout-v1.backup.json").exists());
    assert!(restored.panes[0].exited);
    assert_eq!(restored.workspaces[0].tabs[0].name, "Saved tab");
    assert!(matches!(
        d.request(Request::Restart { pane: pane.clone() }),
        Message::State(_)
    ));
    assert_ne!(pid, d.pid(&pane));
    let _ = d.request(Request::Stop);
}
#[test]
fn slow_client_does_not_block_terminal_and_queries_have_one_responder() {
    let d = Daemon::start();
    let pane = d.create().panes[0].id.clone();
    let mut slow = d.connect();
    write_frame(&mut slow, &Request::Attach { pane: pane.clone() }).unwrap();
    let _: Message = read_frame(&mut slow).unwrap();
    d.send(&pane,"python3 -c 'import sys;sys.stdout.write(\"streaming\\n\"*200000);print(\"FINISHED\"+\"_SLOW\")'\r");
    d.until(|| d.capture(&pane).contains("FINISHED_SLOW"));
    let mut fast = d.connect();
    write_frame(&mut fast, &Request::Attach { pane: pane.clone() }).unwrap();
    assert!(matches!(
        read_frame::<Message>(&mut fast).unwrap(),
        Message::Snapshot { .. }
    ));
    // Raw terminal queries bypass line discipline. Any second answer becomes EXTRA data.
    d.send(&pane,"python3 -c 'import os,tty,termios,select,time;old=termios.tcgetattr(0);tty.setraw(0);os.write(1,b\"\\x1b[6n\");a=os.read(0,64);time.sleep(.1);extra=os.read(0,64) if select.select([0],[],[],.1)[0] else b\"\";termios.tcsetattr(0,termios.TCSANOW,old);print(\"ANSWER_\"+a.hex()+\"_EXTRA_\"+extra.hex()+\"_END\")'\r");
    d.until(|| {
        d.capture(&pane)
            .lines()
            .any(|l| l.trim().starts_with("ANSWER_1b5b") && l.trim().ends_with("_EXTRA__END"))
    });
    let _ = d.request(Request::Stop);
}
#[test]
fn rejects_insecure_runtime_directory() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o755)).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_jamd"))
        .env("JAM_SOCKET", dir.path().join("socket"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("private directory"));
}

#[test]
fn keys_paste_and_mouse_follow_application_modes() {
    let d = Daemon::start();
    let pane = d.create().panes[0].id.clone();
    let expected = b"\x1b[200~hi\x1b[201~\x1bOA\x03\x1b[Z\x1b[<0;2;2M";
    let command = format!(
        "python3 -c 'import os,tty,termios;old=termios.tcgetattr(0);tty.setraw(0);os.write(1,b\"\\x1b[?2004h\\x1b[?1h\\x1b[?1000h\\x1b[?1006h\");print(\"READY\"+\"_INPUT\",flush=True);a=b\"\";exec(\"while len(a)<{}: a+=os.read(0,64)\");os.write(1,b\"\\x1b[?2004l\\x1b[?1l\\x1b[?1000l\\x1b[?1006l\");termios.tcsetattr(0,termios.TCSANOW,old);print(\"INPUT_\"+a.hex())'\r",
        expected.len()
    );
    d.send(&pane, &command);
    d.until(|| d.capture(&pane).contains("READY_INPUT"));
    assert!(matches!(
        d.request(Request::Paste {
            pane: pane.clone(),
            text: "hi".into()
        }),
        Message::Ok
    ));
    assert!(matches!(
        d.request(Request::Key {
            release: false,
            pane: pane.clone(),
            key: 78,
            text: String::new(),
            mods: 0
        }),
        Message::Ok
    ));
    assert!(matches!(
        d.request(Request::Key {
            release: false,
            pane: pane.clone(),
            key: 22,
            text: "c".into(),
            mods: 2
        }),
        Message::Ok
    ));
    assert!(matches!(
        d.request(Request::Key {
            release: false,
            pane: pane.clone(),
            key: 64,
            text: String::new(),
            mods: 1
        }),
        Message::Ok
    ));
    assert!(matches!(
        d.request(Request::Mouse {
            pane: pane.clone(),
            action: 0,
            button: 0,
            x: 15.0,
            y: 15.0,
            cell_width: 10,
            cell_height: 10,
            mods: 0
        }),
        Message::Ok
    ));
    let expected = format!(
        "INPUT_{}",
        expected
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>()
    );
    d.until(|| d.capture(&pane).contains(&expected));
    let _ = d.request(Request::Stop);
}

#[test]
fn worktree_operations_are_async_and_close_preserves_checkout() {
    let d = Daemon::start();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(d.dir.path())
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
    git(&["init", "-b", "main"]);
    git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "--allow-empty",
        "-m",
        "initial",
    ]);
    let state = d.create();
    let root = state.workspaces[0].id.clone();
    let target = d.dir.path().join("linked checkout");
    let Message::Operation { id, .. } = d.request(Request::WorktreeCreate {
        name: "Fix link flap".into(),
        workspace: root.clone(),
        branch: "feature/test".into(),
        base: "HEAD".into(),
        path: Some(target.to_string_lossy().into_owned()),
    }) else {
        panic!("expected job")
    };
    let mut child = None;
    d.until(|| match d.request(Request::Operation { id: id.clone() }) {
        Message::Operation {
            done: true,
            error,
            workspace,
            ..
        } => {
            assert!(error.is_none(), "{error:?}");
            child = workspace;
            true
        }
        _ => false,
    });
    let child = child.unwrap();
    let state = d.state();
    assert_eq!(state.workspaces.len(), 2);
    assert_eq!(state.workspaces[1].name, "Fix link flap");
    assert_eq!(state.workspaces[1].branch.as_deref(), Some("feature/test"));
    assert_eq!(
        state.workspaces[0].repository,
        state.workspaces[1].repository
    );
    assert!(matches!(
        d.request(Request::WorkspaceClose {
            workspace: child,
            group: false
        }),
        Message::State(_)
    ));
    assert!(target.exists());
    assert_eq!(d.state().workspaces.len(), 1);
    let _ = d.request(Request::Stop);
}

#[test]
fn failed_restart_keeps_pane_stopped_and_retry_preserves_id() {
    let d = Daemon::start();
    let directory = d.dir.path().join("restart-test");
    std::fs::create_dir(&directory).unwrap();
    let Message::State(state) = d.request(Request::CreateWorkspace {
        path: directory.to_string_lossy().into_owned(),
        name: "Restart".into(),
    }) else {
        panic!("expected workspace")
    };
    let pane = state.panes[0].id.clone();
    d.send(&pane, "exit\r");
    d.until(|| d.state().panes[0].exited);
    std::fs::remove_dir(&directory).unwrap();
    assert!(matches!(
        d.request(Request::Restart { pane: pane.clone() }),
        Message::Error(_)
    ));
    assert!(d.state().panes[0].exited);
    std::fs::create_dir(&directory).unwrap();
    let Message::State(restarted) = d.request(Request::Restart { pane: pane.clone() }) else {
        panic!("expected restart")
    };
    assert_eq!(restarted.panes[0].id, pane);
    assert!(!restarted.panes[0].exited);
    assert_eq!(restarted.workspaces[0].tabs.len(), 1);
    let _ = d.request(Request::Stop);
}

#[test]
fn tab_moves_preserve_live_processes_and_validate_destination() {
    let d = Daemon::start();
    let source = d.create();
    let workspace = source.workspaces[0].id.clone();
    let tab = source.workspaces[0].tabs[0].id.clone();
    let pane = source.panes[0].id.clone();
    let pid = d.pid(&pane);
    let target_path = d.dir.path().join("target-workspace");
    std::fs::create_dir(&target_path).unwrap();
    let Message::State(created) = d.request(Request::CreateWorkspace {
        path: target_path.to_string_lossy().into_owned(),
        name: "Target".into(),
    }) else {
        panic!()
    };
    let target = created.workspaces[1].id.clone();
    assert!(matches!(
        d.request(Request::MoveTab {
            tab: tab.clone(),
            workspace: target.clone(),
            index: 8
        }),
        Message::Error(_)
    ));
    assert_eq!(d.state().workspaces[0].tabs.len(), 1);
    let Message::State(moved) = d.request(Request::MoveTab {
        tab: tab.clone(),
        workspace: target.clone(),
        index: 0,
    }) else {
        panic!()
    };
    assert!(moved.workspaces[0].tabs.is_empty());
    assert_eq!(moved.workspaces[1].tabs[0].id, tab);
    assert_eq!(
        moved.panes.iter().find(|p| p.id == pane).unwrap().workspace,
        target
    );
    assert_eq!(d.pid(&pane), pid);
    let Message::State(reordered) = d.request(Request::MoveTab {
        tab: tab.clone(),
        workspace: target.clone(),
        index: 1,
    }) else {
        panic!()
    };
    assert_eq!(reordered.workspaces[1].tabs[1].id, tab);
    let Message::State(returned) = d.request(Request::MoveTab {
        tab: tab.clone(),
        workspace,
        index: 0,
    }) else {
        panic!()
    };
    assert_eq!(returned.workspaces[0].tabs[0].id, tab);
    assert_eq!(d.pid(&pane), pid);
    let _ = d.request(Request::Stop);
}

#[test]
fn independent_workspaces_have_fluid_names_and_persistent_overrides() {
    let d = Daemon::start();
    let path = d.dir.path().to_string_lossy().into_owned();
    let Message::State(first) = d.request(Request::NewWorkspace { path: path.clone() }) else {
        panic!()
    };
    let workspace = first.workspaces[0].id.clone();
    let pane = first.panes[0].id.clone();
    let Message::State(second) = d.request(Request::NewWorkspace { path: path.clone() }) else {
        panic!()
    };
    assert_eq!(second.workspaces.len(), 2);
    assert_ne!(second.workspaces[0].id, second.workspaces[1].id);
    let Message::State(opened) = d.request(Request::CreateWorkspace {
        path,
        name: String::new(),
    }) else {
        panic!()
    };
    assert_eq!(opened.workspaces.len(), 2);
    let child = d.dir.path().join("fluid-name");
    std::fs::create_dir(&child).unwrap();
    d.send(
        &pane,
        &format!(
            "printf '\\033]7;file://localhost{}\\007'\r",
            child.display()
        ),
    );
    d.until(|| d.state().workspaces[0].name == "fluid-name");
    let _ = d.request(Request::Rename {
        id: workspace.clone(),
        name: "Pinned".into(),
    });
    let next = d.dir.path().join("next-name");
    std::fs::create_dir(&next).unwrap();
    d.send(
        &pane,
        &format!("printf '\\033]7;file://localhost{}\\007'\r", next.display()),
    );
    d.until(|| {
        d.state()
            .panes
            .iter()
            .any(|p| p.id == pane && p.cwd == next.to_string_lossy())
    });
    assert_eq!(d.state().workspaces[0].name, "Pinned");
    let _ = d.request(Request::Rename {
        id: workspace.clone(),
        name: String::new(),
    });
    assert_eq!(d.state().workspaces[0].name, "next-name");
    let Message::State(split) = d.request(Request::Split {
        pane: pane.clone(),
        vertical: false,
    }) else {
        panic!()
    };
    let other = split
        .panes
        .iter()
        .find(|p| p.workspace == workspace && p.id != pane)
        .unwrap()
        .id
        .clone();
    d.send(
        &other,
        &format!(
            "printf '\\033]7;file://localhost{}\\007'\r",
            child.display()
        ),
    );
    d.until(|| {
        d.state()
            .panes
            .iter()
            .any(|p| p.id == other && p.cwd == child.to_string_lossy())
    });
    let _ = d.request(Request::FocusPane { pane: other });
    assert_eq!(d.state().workspaces[0].name, "fluid-name");
    let _ = d.request(Request::FocusPane { pane });
    assert_eq!(d.state().workspaces[0].name, "next-name");
    let _ = d.request(Request::Stop);
}

#[test]
fn workspace_order_is_a_validated_permutation() {
    let d = Daemon::start();
    let path = d.dir.path().to_string_lossy().into_owned();
    let Message::State(first) = d.request(Request::NewWorkspace { path: path.clone() }) else {
        panic!()
    };
    let first_id = first.workspaces[0].id.clone();
    let pane = first.panes[0].id.clone();
    let pid = d.pid(&pane);
    let Message::State(second) = d.request(Request::NewWorkspace { path }) else {
        panic!()
    };
    let second_id = second.workspaces[1].id.clone();
    assert!(matches!(
        d.request(Request::ReorderWorkspaces {
            workspaces: vec![first_id.clone(), first_id.clone()]
        }),
        Message::Error(_)
    ));
    assert_eq!(d.state().workspaces[0].id, first_id);
    let Message::State(reordered) = d.request(Request::ReorderWorkspaces {
        workspaces: vec![second_id.clone(), first_id.clone()],
    }) else {
        panic!()
    };
    assert_eq!(reordered.workspaces[0].id, second_id);
    assert_eq!(reordered.workspaces[1].id, first_id);
    assert_eq!(d.pid(&pane), pid);
    assert!(matches!(
        d.request(Request::ReorderWorkspaces {
            workspaces: vec![second_id]
        }),
        Message::Error(_)
    ));
    let _ = d.request(Request::Stop);
}

#[test]
fn pane_snap_moves_and_swaps_preserve_all_processes() {
    let d = Daemon::start();
    let first = d.create();
    let source_workspace = first.workspaces[0].id.clone();
    let source_tab = first.workspaces[0].tabs[0].id.clone();
    let a = first.panes[0].id.clone();
    let a_pid = d.pid(&a);
    let Message::State(split) = d.request(Request::Split {
        pane: a.clone(),
        vertical: false,
    }) else {
        panic!()
    };
    let b = split.panes.iter().find(|p| p.id != a).unwrap().id.clone();
    let b_pid = d.pid(&b);
    let Message::State(target) = d.request(Request::NewWorkspace {
        path: d.dir.path().to_string_lossy().into_owned(),
    }) else {
        panic!()
    };
    let target_workspace = target.workspaces[1].id.clone();
    let c = target
        .panes
        .iter()
        .find(|p| p.workspace == target_workspace)
        .unwrap()
        .id
        .clone();
    let c_pid = d.pid(&c);
    for (placement, vertical, before) in [
        (PanePlacement::Left, false, true),
        (PanePlacement::Right, false, false),
        (PanePlacement::Above, true, true),
        (PanePlacement::Below, true, false),
    ] {
        let Message::State(moved) = d.request(Request::MovePane {
            pane: a.clone(),
            destination: PaneDestination::Pane {
                pane: b.clone(),
                placement,
            },
        }) else {
            panic!("move failed")
        };
        let Node::Split {
            vertical: actual,
            a: left,
            b: right,
            ..
        } = &moved.workspaces[0].tabs[0].root
        else {
            panic!()
        };
        assert_eq!(*actual, vertical);
        assert!(matches!(&**left,Node::Leaf(id) if id == if before {&a} else {&b}));
        assert!(matches!(&**right,Node::Leaf(id) if id == if before {&b} else {&a}));
    }
    let Message::State(swapped) = d.request(Request::MovePane {
        pane: a.clone(),
        destination: PaneDestination::Pane {
            pane: c.clone(),
            placement: PanePlacement::Swap,
        },
    }) else {
        panic!()
    };
    assert_eq!(
        swapped.panes.iter().find(|p| p.id == a).unwrap().workspace,
        target_workspace
    );
    assert_eq!(
        swapped.panes.iter().find(|p| p.id == c).unwrap().workspace,
        source_workspace
    );
    let Message::State(moved_tab) = d.request(Request::MovePane {
        pane: a.clone(),
        destination: PaneDestination::Tab { tab: source_tab },
    }) else {
        panic!()
    };
    assert!(moved_tab.workspaces[1].tabs.is_empty());
    let Message::State(moved_workspace) = d.request(Request::MovePane {
        pane: b.clone(),
        destination: PaneDestination::Workspace {
            workspace: target_workspace.clone(),
        },
    }) else {
        panic!()
    };
    assert_eq!(moved_workspace.workspaces[1].tabs.len(), 1);
    assert_eq!(
        moved_workspace
            .panes
            .iter()
            .find(|p| p.id == b)
            .unwrap()
            .workspace,
        target_workspace
    );
    assert_eq!(d.pid(&a), a_pid);
    assert_eq!(d.pid(&b), b_pid);
    assert_eq!(d.pid(&c), c_pid);
    assert!(matches!(
        d.request(Request::MovePane {
            pane: a,
            destination: PaneDestination::Pane {
                pane: "missing".into(),
                placement: PanePlacement::Left
            }
        }),
        Message::Error(_)
    ));
    assert_eq!(d.state().panes.len(), 3);
    let _ = d.request(Request::Stop);
}

#[test]
fn tabs_receive_distinct_numbered_default_names() {
    let d = Daemon::start();
    let first = d.create();
    let w = &first.workspaces[0];
    assert_eq!(w.tabs[0].name, "Tab 1");
    let _ = d.request(Request::NewTab {
        workspace: w.id.clone(),
    });
    let _ = d.request(Request::NewTab {
        workspace: w.id.clone(),
    });
    let names = d.state().workspaces[0]
        .tabs
        .iter()
        .map(|t| t.name.clone())
        .collect::<Vec<_>>();
    assert_eq!(names, ["Tab 1", "Tab 2", "Tab 3"]);
}

#[test]
fn only_last_focused_client_changes_terminal_size() {
    let d = Daemon::start();
    let pane = d.create().panes[0].id.clone();
    let client = |name: &str, request| {
        d.request(Request::Client {
            client: name.into(),
            request: Box::new(request),
        })
    };
    client("a", Request::FocusPane { pane: pane.clone() });
    client(
        "a",
        Request::Resize {
            pane: pane.clone(),
            cols: 80,
            rows: 31,
        },
    );
    client(
        "b",
        Request::Resize {
            pane: pane.clone(),
            cols: 120,
            rows: 42,
        },
    );
    d.send(&pane, "stty size\r");
    d.until(|| d.capture(&pane).contains("31 80"));
    client("b", Request::FocusPane { pane: pane.clone() });
    client(
        "b",
        Request::Resize {
            pane: pane.clone(),
            cols: 120,
            rows: 42,
        },
    );
    d.send(&pane, "stty size\r");
    d.until(|| d.capture(&pane).contains("42 120"));
}

#[test]
fn upgrade_guard_rejects_active_panes_and_new_work_after_drain() {
    let d = Daemon::start();
    let state = d.create();
    assert!(matches!(
        d.request(Request::PrepareUpgrade),
        Message::Error(_)
    ));
    for pane in state.panes {
        d.request(Request::Close { pane: pane.id });
    }
    assert!(matches!(d.request(Request::PrepareUpgrade), Message::Ok));
    assert!(matches!(
        d.request(Request::NewWorkspace {
            path: d.dir.path().display().to_string()
        }),
        Message::Error(_)
    ));
}

#[test]
fn stale_layout_edits_leave_authoritative_layout_unchanged() {
    let d = Daemon::start();
    let original = d.create();
    let pane = original.panes[0].id.clone();
    let reply = d.request(Request::Split {
        pane,
        vertical: true,
    });
    let Message::State(updated) = reply else {
        panic!("split failed")
    };
    let root = serde_json::to_value(&updated.workspaces[0].tabs[0].root).unwrap();
    let reply = d.request(Request::Guarded {
        revision: original.revision,
        request: Box::new(Request::Ratio {
            tab: updated.workspaces[0].tabs[0].id.clone(),
            path: vec![],
            ratio: 0.7,
        }),
    });
    assert!(matches!(reply, Message::Error(_)));
    assert_eq!(
        serde_json::to_value(&d.state().workspaces[0].tabs[0].root).unwrap(),
        root
    );
}

#[test]
fn empty_tab_and_workspace_preferences() {
    for close_tab in [false, true] {
        for close_workspace in [false, true] {
            let d = Daemon::start();
            let settings = config::HostSettings {
                close_empty_tabs: close_tab,
                close_empty_workspaces: close_workspace,
                ..config::Config::default().into()
            };
            assert!(matches!(
                d.request(Request::Configure { settings }),
                Message::Settings(_)
            ));
            let Message::State(s) = d.request(Request::NewWorkspace {
                path: d.dir.path().to_string_lossy().into(),
            }) else {
                panic!()
            };
            let workspace = s.workspaces[0].id.clone();
            let tab = s.workspaces[0].tabs[0].id.clone();
            let pane = s.panes[0].id.clone();
            d.request(Request::Close { pane });
            let state = d.state();
            if close_tab && close_workspace {
                assert!(state.workspaces.is_empty());
            } else if close_tab {
                assert!(state.workspaces[0].tabs.is_empty());
            } else {
                assert!(matches!(state.workspaces[0].tabs[0].root, Node::Empty));
                assert!(matches!(
                    d.request(Request::NewPane { tab: tab.clone() }),
                    Message::State(_)
                ));
                assert_eq!(d.state().panes.len(), 1);
                assert_eq!(d.state().workspaces[0].id, workspace);
                assert!(matches!(
                    d.request(Request::CloseTab { tab }),
                    Message::State(_)
                ));
                let state = d.state();
                if close_workspace {
                    assert!(state.workspaces.is_empty());
                } else {
                    assert!(state.workspaces[0].tabs.is_empty());
                }
            }
        }
    }
}
#[test]
fn deletion_preference_blocks_file_removal_but_not_closing() {
    let d = Daemon::start();
    let settings = config::HostSettings {
        allow_workspace_deletion: false,
        ..config::Config::default().into()
    };
    d.request(Request::Configure { settings });
    let Message::State(s) = d.request(Request::NewWorkspace {
        path: d.dir.path().to_string_lossy().into(),
    }) else {
        panic!()
    };
    let workspace = s.workspaces[0].id.clone();
    let result = d.request(Request::WorktreeRemove {
        workspace: workspace.clone(),
        force: true,
    });
    if let Message::Operation { id, .. } = result {
        d.until(|| matches!(d.request(Request::Operation {id:id.clone()}), Message::Operation {done:true,error:Some(e),..} if e.contains("disabled")));
    } else {
        assert!(matches!(result, Message::Error(e) if e.contains("disabled")));
    }
    assert!(d.dir.path().exists());
    assert!(matches!(
        d.request(Request::WorkspaceClose {
            workspace,
            group: false
        }),
        Message::State(_)
    ));
    assert!(d.state().workspaces.is_empty());
}

#[test]
fn configured_worktree_template_controls_checkout_creation() {
    let d = Daemon::start();
    let source = d.dir.path().join("repository");
    std::fs::create_dir(&source).unwrap();
    let git = |args: &[&str]| {
        assert!(
            Command::new("git")
                .arg("-C")
                .arg(&source)
                .args(args)
                .status()
                .unwrap()
                .success()
        );
    };
    git(&["init", "-b", "main"]);
    git(&[
        "-c",
        "user.name=Test",
        "-c",
        "user.email=test@example.invalid",
        "commit",
        "--allow-empty",
        "-m",
        "initial",
    ]);
    let settings = config::HostSettings {
        worktrees_directory: format!(
            "{}/trees/{{{{worktree}}}}/{{{{repo}}}}",
            d.dir.path().display()
        ),
        ..config::Config::default().into()
    };
    assert!(matches!(
        d.request(Request::Configure { settings }),
        Message::Settings(_)
    ));
    let Message::State(state) = d.request(Request::NewWorkspace {
        path: source.to_string_lossy().into(),
    }) else {
        panic!()
    };
    let Message::Operation { id, .. } = d.request(Request::WorktreeCreate {
        name: String::new(),
        workspace: state.workspaces[0].id.clone(),
        branch: "feature/login".into(),
        base: "HEAD".into(),
        path: None,
    }) else {
        panic!()
    };
    d.until(|| match d.request(Request::Operation { id: id.clone() }) {
        Message::Operation {
            done: true, error, ..
        } => {
            assert!(error.is_none(), "{error:?}");
            true
        }
        _ => false,
    });
    let expected = d
        .dir
        .path()
        .join("trees/feature-login/repository")
        .canonicalize()
        .unwrap();
    assert!(
        d.state()
            .workspaces
            .iter()
            .any(|w| std::path::Path::new(&w.path) == expected)
    );
    assert!(expected.join(".git").is_file());
}

#[test]
fn empty_workspace_remembers_focused_directory_across_restart() {
    for close_tab in [false, true] {
        let mut d = Daemon::start();
        d.request(Request::Configure {
            settings: config::HostSettings {
                close_empty_tabs: close_tab,
                close_empty_workspaces: false,
                ..config::Config::default().into()
            },
        });
        let state = d.create();
        let workspace = state.workspaces[0].id.clone();
        let original = state.workspaces[0].path.clone();
        let tab = state.workspaces[0].tabs[0].id.clone();
        let pane = state.panes[0].id.clone();
        let path = d.dir.path().join("last directory");
        std::fs::create_dir(&path).unwrap();
        let path = std::fs::canonicalize(path).unwrap().display().to_string();
        d.request(Request::FocusPane { pane: pane.clone() });
        d.send(
            &pane,
            &format!("cd '{path}'; printf '\\033]7;file://localhost%s\\007' \"$PWD\"\r"),
        );
        d.until(|| d.state().panes[0].cwd == path);
        assert!(matches!(
            d.request(Request::Close { pane }),
            Message::State(_)
        ));
        let empty = d.state();
        assert_eq!(
            empty.workspaces[0].path, original,
            "Workspace identity stays fixed"
        );
        assert_eq!(empty.workspaces[0].last_cwd.as_deref(), Some(path.as_str()));
        d.request(Request::Stop);
        d.child.wait().unwrap();
        d.child = Daemon::launch(&d.socket, d.dir.path());
        d.until(|| UnixStream::connect(&d.socket).is_ok());
        let result = if close_tab {
            d.request(Request::NewTab { workspace })
        } else {
            d.request(Request::NewPane { tab })
        };
        let Message::State(state) = result else {
            panic!("Could not reopen terminal: {result:?}")
        };
        assert_eq!(state.panes[0].cwd, path);
        assert_eq!(state.workspaces[0].path, original);
        for pane in state.panes {
            d.request(Request::Close { pane: pane.id });
        }
    }
}

#[test]
fn explicit_local_stop_recovers_a_frozen_daemon_and_preserves_layout() {
    let mut d = Daemon::start();
    let initial = d.create();
    let workspace = initial.workspaces[0].id.clone();
    let tab = initial.workspaces[0].tabs[0].id.clone();
    assert!(
        Command::new("kill")
            .args(["-STOP", &d.child.id().to_string()])
            .status()
            .unwrap()
            .success()
    );
    // Early daemons used a read-only-for-others lock inside their private 0700
    // socket directory. Recovery must accept that legacy mode as well.
    std::fs::set_permissions(
        d.socket.parent().unwrap().join("server.lock"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let began = Instant::now();
    let output = Command::new(env!("CARGO_BIN_EXE_jam"))
        .args(["daemon", "stop"])
        .env("JAM_SOCKET", &d.socket)
        .env("JAM_STATE_DIR", d.dir.path().join("state"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        began.elapsed() < Duration::from_secs(9),
        "Explicit stop must have a bounded recovery path"
    );
    d.child.wait().unwrap();
    d.child = Daemon::launch(&d.socket, d.dir.path());
    d.until(|| connect_to(&d.socket).is_ok());
    let state = d.state();
    assert_eq!(state.workspaces[0].id, workspace);
    assert_eq!(state.workspaces[0].tabs[0].id, tab);
}

#[test]
fn session_root_exit_stops_owned_background_jobs() {
    let d = Daemon::start();
    let state = d.create();
    let pane = state.panes[0].id.clone();
    let pidfile = d.dir.path().join("background.pid");
    let ready = d.dir.path().join("background.ready");
    // Ignore the normal terminal hangup, and wait until that trap is installed
    // before exiting the session root. Disable job control so this stays in the
    // root's owned process group rather than becoming a separate shell job.
    d.send(&pane, &format!(
        "set +m; (trap '' HUP; echo ready > '{}'; exec sleep 60) & echo $! > '{}'; while ! test -f '{}'; do sleep 0.01; done; exit\r",
        ready.display(), pidfile.display(), ready.display()));
    d.until(|| pidfile.exists());
    let pid: i32 = std::fs::read_to_string(&pidfile)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    struct Cleanup(i32);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = Command::new("kill")
                .args(["-KILL", &self.0.to_string()])
                .output();
        }
    }
    let _cleanup = Cleanup(pid);
    d.until(|| d.state().panes[0].exited);
    d.until(|| {
        // A Linux container's PID 1 may leave an orphan zombie unreaped. A
        // zombie is stopped; kill(pid, 0) alone would misclassify it as running.
        let output = Command::new("ps")
            .args(["-p", &pid.to_string(), "-o", "stat="])
            .output()
            .unwrap();
        !output.status.success()
            || String::from_utf8_lossy(&output.stdout)
                .trim()
                .starts_with('Z')
    });
    assert!(matches!(
        d.request(Request::Close { pane }),
        Message::State(_)
    ));
    assert!(matches!(d.request(Request::Stop), Message::Ok));
}

#[test]
fn ending_a_terminal_does_not_signal_a_detached_session() {
    let d = Daemon::start();
    let pane = d.create().panes[0].id.clone();
    let pidfile = d.dir.path().join("detached.pid");
    // This child deliberately leaves the terminal's session/process group.
    let script = format!(
        "import os,time; os.setsid(); open(\"{}\",\"w\").write(str(os.getpid())); time.sleep(60)",
        pidfile.display()
    );
    d.send(
        &pane,
        &format!(
            "set +m; python3 -c '{}' & while ! test -f '{}'; do sleep 0.01; done; exit\r",
            script,
            pidfile.display()
        ),
    );
    d.until(|| pidfile.exists() && !std::fs::read_to_string(&pidfile).unwrap().is_empty());
    let pid = std::fs::read_to_string(&pidfile).unwrap();
    struct Cleanup(String);
    impl Drop for Cleanup {
        fn drop(&mut self) {
            let _ = Command::new("kill").args(["-KILL", &self.0]).output();
        }
    }
    let _cleanup = Cleanup(pid.clone());
    d.until(|| d.state().panes[0].exited);
    assert!(matches!(
        d.request(Request::Close { pane }),
        Message::State(_)
    ));
    assert!(matches!(d.request(Request::Stop), Message::Ok));
    let output = Command::new("ps")
        .args(["-p", &pid, "-o", "stat="])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert!(
        !String::from_utf8_lossy(&output.stdout)
            .trim()
            .starts_with('Z'),
        "detached session was killed"
    );
}

#[test]
fn approved_upgrade_guard_rejects_changed_sessions_and_owns_shutdown() {
    let d = Daemon::start();
    let initial = d.create();
    let pane = initial.panes[0].id.clone();
    let workspace = initial.workspaces[0].id.clone();
    let Message::HostInfo {
        incarnation, build, ..
    } = d.request(Request::HostInfo)
    else {
        panic!("missing host metadata")
    };
    let token = jam_core::id();
    let prepare = |panes: Vec<String>| {
        let sessions = d
            .state()
            .panes
            .iter()
            .filter(|p| panes.contains(&p.id))
            .map(|p| (p.id.clone(), p.session_generation.clone().unwrap()))
            .collect();
        Request::PrepareUpgradeApproved {
            token: token.clone(),
            incarnation: incarnation.clone(),
            build: build.clone(),
            panes,
            sessions,
        }
    };
    // A pane started after the UI captured consent invalidates that consent.
    let Message::State(changed) = d.request(Request::NewTab {
        workspace: workspace.clone(),
    }) else {
        panic!("missing state")
    };
    assert!(matches!(
        d.request(prepare(vec![pane.clone()])),
        Message::Error(_)
    ));
    assert!(matches!(
        d.request(prepare(
            changed.panes.iter().map(|p| p.id.clone()).collect()
        )),
        Message::Ok
    ));
    assert!(matches!(
        d.request(Request::NewTab {
            workspace: workspace.clone()
        }),
        Message::Error(_)
    ));
    let other = jam_core::id();
    assert!(matches!(
        d.request(Request::AbortUpgrade {
            token: other.clone()
        }),
        Message::Error(_)
    ));
    assert!(matches!(
        d.request(Request::StopUpgrade { token: other }),
        Message::Error(_)
    ));
    assert!(
        matches!(d.request(Request::State), Message::State(_)),
        "wrong owner must not stop daemon"
    );
    assert!(matches!(
        d.request(Request::AbortUpgrade {
            token: token.clone()
        }),
        Message::Ok
    ));
    assert!(matches!(
        d.request(Request::NewTab { workspace }),
        Message::State(_)
    ));
    let approved = d.state().panes.iter().map(|p| p.id.clone()).collect();
    assert!(matches!(d.request(prepare(approved)), Message::Ok));
    assert!(matches!(
        d.request(Request::StopUpgrade { token }),
        Message::Ok
    ));
}

#[test]
fn upgrade_boot_holds_work_until_owned_commit_without_reviving_exited_panes() {
    let mut d = Daemon::start();
    let initial = d.create();
    let workspace = initial.workspaces[0].id.clone();
    assert!(matches!(d.request(Request::Stop), Message::Ok));
    d.child.wait().unwrap();
    let token = jam_core::id();
    let sentinel = d.dir.path().join("state/.upgrade-boot.json");
    std::fs::write(
        &sentinel,
        serde_json::to_vec(&serde_json::json!({"token":token})).unwrap(),
    )
    .unwrap();
    std::fs::set_permissions(&sentinel, std::fs::Permissions::from_mode(0o600)).unwrap();
    d.child = Daemon::launch(&d.socket, d.dir.path());
    d.until(|| UnixStream::connect(&d.socket).is_ok());
    assert!(d.state().panes.iter().all(|p| p.exited));
    assert!(matches!(
        d.request(Request::NewTab {
            workspace: workspace.clone()
        }),
        Message::Error(_)
    ));
    assert!(matches!(
        d.request(Request::CommitUpgrade {
            token: jam_core::id()
        }),
        Message::Error(_)
    ));
    assert!(sentinel.exists());
    let Message::HostInfo {
        incarnation, build, ..
    } = d.request(Request::HostInfo)
    else {
        panic!("missing host info")
    };
    assert!(matches!(
        d.request(Request::PrepareUpgradeApproved {
            token: jam_core::id(),
            incarnation,
            build,
            panes: vec![],
            sessions: Default::default(),
        }),
        Message::Error(_)
    ));
    std::fs::write(
        &sentinel,
        serde_json::to_vec(&serde_json::json!({"token":jam_core::id()})).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        d.request(Request::CommitUpgrade {
            token: token.clone()
        }),
        Message::Error(_)
    ));
    std::fs::write(
        &sentinel,
        serde_json::to_vec(&serde_json::json!({"token":token})).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        d.request(Request::CommitUpgrade {
            token: token.clone()
        }),
        Message::Ok
    ));
    assert!(!sentinel.exists());
    assert!(
        d.state().panes.iter().all(|p| p.exited),
        "Commit must not revive intentionally exited work"
    );
    assert!(matches!(
        d.request(Request::CommitUpgrade { token }),
        Message::Error(_)
    ));
    assert!(matches!(
        d.request(Request::NewTab { workspace }),
        Message::State(_)
    ));
}

#[test]
fn restarted_same_pane_requires_fresh_upgrade_session_consent() {
    let d = Daemon::start();
    let initial = d.create();
    let old = initial.panes[0].clone();
    let Message::HostInfo {
        incarnation, build, ..
    } = d.request(Request::HostInfo)
    else {
        panic!("missing host info")
    };
    let token = jam_core::id();
    let approved = Request::PrepareUpgradeApproved {
        token: token.clone(),
        incarnation: incarnation.clone(),
        build: build.clone(),
        panes: vec![old.id.clone()],
        sessions: [(old.id.clone(), old.session_generation.clone().unwrap())]
            .into_iter()
            .collect(),
    };
    d.send(&old.id, "exit\r");
    d.until(|| d.state().panes[0].exited);
    let Message::State(restarted) = d.request(Request::Restart {
        pane: old.id.clone(),
    }) else {
        panic!("restart failed")
    };
    let new = &restarted.panes[0];
    assert_eq!(new.id, old.id, "pane identity stays stable across restart");
    assert_ne!(
        new.session_generation, old.session_generation,
        "PTY generation must change"
    );
    assert!(
        matches!(d.request(approved), Message::Error(_)),
        "daemon admission must reject the old generation"
    );
    assert!(
        !d.state().panes[0].exited,
        "unapproved new shell must remain running"
    );
    let fresh = Request::PrepareUpgradeApproved {
        token: token.clone(),
        incarnation,
        build,
        panes: vec![new.id.clone()],
        sessions: [(new.id.clone(), new.session_generation.clone().unwrap())]
            .into_iter()
            .collect(),
    };
    assert!(matches!(d.request(fresh), Message::Ok));
    assert!(matches!(
        d.request(Request::StopUpgrade { token }),
        Message::Ok
    ));
}
#[test]
fn boot_held_stop_requires_its_exact_owned_token() {
    let mut d = Daemon::start();
    d.create();
    assert!(matches!(d.request(Request::Stop), Message::Ok));
    d.child.wait().unwrap();
    let token = jam_core::id();
    let sentinel = d.dir.path().join("state/.upgrade-boot.json");
    std::fs::write(
        &sentinel,
        serde_json::to_vec(&serde_json::json!({"token":token})).unwrap(),
    )
    .unwrap();
    std::fs::set_permissions(&sentinel, std::fs::Permissions::from_mode(0o600)).unwrap();
    d.child = Daemon::launch(&d.socket, d.dir.path());
    d.until(|| UnixStream::connect(&d.socket).is_ok());
    assert!(matches!(
        d.request(Request::StopUpgrade {
            token: jam_core::id()
        }),
        Message::Error(_)
    ));
    assert!(matches!(d.request(Request::State), Message::State(_)));
    std::fs::write(
        &sentinel,
        serde_json::to_vec(&serde_json::json!({"token":jam_core::id()})).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        d.request(Request::StopUpgrade {
            token: token.clone()
        }),
        Message::Error(_)
    ));
    std::fs::write(
        &sentinel,
        serde_json::to_vec(&serde_json::json!({"token":token})).unwrap(),
    )
    .unwrap();
    assert!(matches!(
        d.request(Request::StopUpgrade { token }),
        Message::Ok
    ));
    d.child.wait().unwrap();
    assert!(
        sentinel.exists(),
        "stopping must retain the boot gate for recovery"
    );
}

#[test]
fn live_server_inspection_survives_removed_executable_without_restarting_sessions() {
    let mut d = Daemon::start();
    assert!(matches!(d.request(Request::Stop), Message::Ok));
    d.child.wait().unwrap();
    let binary = d.dir.path().join("jamd");
    std::fs::copy(env!("CARGO_BIN_EXE_jamd"), &binary).unwrap();
    d.child = Command::new(&binary)
        .env("JAM_SOCKET", &d.socket)
        .env("JAM_STATE_DIR", d.dir.path().join("state"))
        .env("SHELL", "/bin/sh")
        .spawn()
        .unwrap();
    d.until(|| UnixStream::connect(&d.socket).is_ok());
    let saved = d.create();
    let pane = &saved.panes[0].id;
    let shell_pid = d.pid(pane);
    std::fs::remove_file(binary).unwrap();
    let request = serde_json::json!({"action":"status","daemon":env!("CARGO_BIN_EXE_jamd")});
    let manage = |request: &serde_json::Value| {
        server_control::manage_at(request, &d.socket, &d.dir.path().join("state")).unwrap()
    };
    for _ in 0..3 {
        let status = manage(&request);
        assert_eq!(status["state"], "connected", "{status}");
        assert_eq!(status["compatible"], true);
        assert_eq!(status["management_available"], false);
        assert_eq!(status["can_takeover"], false);
        assert!(status["error"].is_null());
        assert_eq!(d.pid(pane), shell_pid);
    }
    let update =
        serde_json::json!({"action":"update","daemon":env!("CARGO_BIN_EXE_jamd"),"force":true});
    assert!(server_control::manage_at(&update, &d.socket, &d.dir.path().join("state")).is_err());
    assert_eq!(
        d.pid(pane),
        shell_pid,
        "Unknown ownership must not permit destructive recovery"
    );
}

#[test]
fn retired_app_server_is_still_owned_by_its_installation() {
    let mut d = Daemon::start();
    assert!(matches!(d.request(Request::Stop), Message::Ok));
    d.child.wait().unwrap();
    let bundle = d.dir.path().join("Jam.app");
    let binary = bundle.join("Contents/MacOS/jamd");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_jamd"), &binary).unwrap();
    d.child = Command::new(&binary)
        .env("JAM_SOCKET", &d.socket)
        .env("JAM_STATE_DIR", d.dir.path().join("state"))
        .env("SHELL", "/bin/sh")
        .spawn()
        .unwrap();
    d.until(|| UnixStream::connect(&d.socket).is_ok());
    let retired = d.dir.path().join(".Jam.app.retired/bundle-test/Jam.app");
    std::fs::create_dir_all(retired.parent().unwrap()).unwrap();
    std::fs::rename(&bundle, &retired).unwrap();
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::copy(env!("CARGO_BIN_EXE_jamd"), &binary).unwrap();
    let request = serde_json::json!({"action":"status","daemon":binary});
    let status =
        server_control::manage_at(&request, &d.socket, &d.dir.path().join("state")).unwrap();
    assert_eq!(status["state"], "connected", "{status}");
    assert_eq!(status["management_available"], true);
    assert_eq!(status["can_takeover"], false);
    // Genuine inspection failures carry a retryable diagnostic, not a fake takeover requirement.
    let invalid = serde_json::json!({"action":"status","daemon":d.dir.path().join("missing")});
    let status =
        server_control::manage_at(&invalid, &d.socket, &d.dir.path().join("state")).unwrap();
    assert_eq!(status["state"], "inspection_failed", "{status}");
    assert_eq!(status["failure"]["retryable"], true);
}

#[test]
fn explicit_takeover_preserves_workspace_and_rejects_replayed_confirmation() {
    let mut d = Daemon::start();
    let saved = d.create();
    assert!(matches!(d.request(Request::Stop), Message::Ok));
    d.child.wait().unwrap();
    let original = d.dir.path().join("original");
    std::fs::create_dir(&original).unwrap();
    let binary = original.join("jamd");
    std::fs::copy(env!("CARGO_BIN_EXE_jamd"), &binary).unwrap();
    d.child = Command::new(&binary)
        .env("JAM_SOCKET", &d.socket)
        .env("JAM_STATE_DIR", d.dir.path().join("state"))
        .env("SHELL", "/bin/sh")
        .spawn()
        .unwrap();
    d.until(|| UnixStream::connect(&d.socket).is_ok());
    let mut request = serde_json::json!({"action":"status", "daemon":env!("CARGO_BIN_EXE_jamd")});
    let manage = |request: &serde_json::Value| {
        server_control::manage_at(request, &d.socket, &d.dir.path().join("state"))
    };
    let status = manage(&request).unwrap();
    assert_eq!(status["state"], "takeover_required");
    request["action"] = "prepare_takeover".into();
    let prepared = manage(&request).unwrap();
    assert!(
        UnixStream::connect(&d.socket).is_ok(),
        "Preparation must not stop the server"
    );
    request["action"] = "takeover".into();
    request["token"] = prepared["token"].clone();
    let started = manage(&request).unwrap();
    assert!(manage(&request).is_err(), "Confirmation must be single use");
    let operation = serde_json::json!({"action":"operation", "id":started["id"]});
    d.until(|| manage(&operation).unwrap()["done"] == true);
    assert!(
        manage(&operation).unwrap()["failure"].is_null(),
        "{}",
        format!(
            "{}\n{}",
            manage(&operation).unwrap(),
            std::fs::read_to_string(d.dir.path().join("state/jamd.log")).unwrap_or_default()
        )
    );
    let restored = d.state();
    assert_eq!(saved.workspaces[0].id, restored.workspaces[0].id);
    assert_eq!(
        saved.workspaces[0].tabs[0].id,
        restored.workspaces[0].tabs[0].id
    );
    assert!(restored.panes.iter().all(|pane| pane.exited));
    assert!(matches!(d.request(Request::Stop), Message::Ok));
}

#[test]
fn failed_takeover_restores_previous_server_and_checkpoint() {
    let d = Daemon::start();
    let saved = d.create();
    let directory = d.dir.path().join("replacement");
    std::fs::create_dir(&directory).unwrap();
    let bad = directory.join("jamd");
    let metadata = Command::new(env!("CARGO_BIN_EXE_jamd"))
        .arg("--build-info")
        .output()
        .unwrap();
    let metadata = String::from_utf8(metadata.stdout).unwrap();
    std::fs::write(
        &bad,
        format!(
            "#!/bin/sh\nif [ \"$1\" = --build-info ]; then printf '%s\\n' '{}'; else exit 42; fi\n",
            metadata.trim()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&bad, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut request = serde_json::json!({"action":"prepare_takeover","daemon":bad});
    let manage = |request: &serde_json::Value| {
        server_control::manage_at(request, &d.socket, &d.dir.path().join("state"))
    };
    let prepared = manage(&request).unwrap();
    request["action"] = "takeover".into();
    request["token"] = prepared["token"].clone();
    let started = manage(&request).unwrap();
    let operation = serde_json::json!({"action":"operation","id":started["id"]});
    d.until(|| manage(&operation).unwrap()["done"] == true);
    assert!(
        manage(&operation).unwrap()["failure"]["details"]
            .as_str()
            .unwrap()
            .contains("restored"),
        "{}",
        manage(&operation).unwrap()
    );
    assert_eq!(d.state().workspaces[0].id, saved.workspaces[0].id);
    assert_eq!(
        d.state().workspaces[0].tabs[0].id,
        saved.workspaces[0].tabs[0].id
    );
    assert!(matches!(d.request(Request::Stop), Message::Ok));
}

#[test]
fn newer_local_release_refuses_takeover_and_forced_update() {
    let d = Daemon::start();
    let path = d.socket.parent().unwrap().join("release.json");
    let mut info: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    info["release_version"] = "999.0.0".into();
    std::fs::write(&path, serde_json::to_vec(&info).unwrap()).unwrap();
    let mut request = serde_json::json!({"action":"status","daemon":env!("CARGO_BIN_EXE_jamd")});
    let manage = |request: &serde_json::Value| {
        server_control::manage_at(request, &d.socket, &d.dir.path().join("state"))
    };
    assert_eq!(manage(&request).unwrap()["state"], "client_update_required");
    request["action"] = "prepare_takeover".into();
    assert!(manage(&request).is_err());
    request["action"] = "update".into();
    request["force"] = true.into();
    assert!(manage(&request).is_err());
    assert!(d.child.id() > 0 && UnixStream::connect(&d.socket).is_ok());
}

#[test]
fn interrupted_takeover_before_shutdown_keeps_live_sessions() {
    let d = Daemon::start();
    let saved = d.create();
    let pane = &saved.panes[0].id;
    let pid = d.pid(pane);
    let directory = d.dir.path().join("state/server-actions").join(id());
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(directory.join("transaction.json"), serde_json::to_vec(&serde_json::json!({
        "phase":"stopping", "previous":env!("CARGO_BIN_EXE_jamd"),"replacement":env!("CARGO_BIN_EXE_jamd")
    })).unwrap()).unwrap();
    let mut request = serde_json::json!({"action":"status","daemon":env!("CARGO_BIN_EXE_jamd")});
    let manage = |request: &serde_json::Value| {
        server_control::manage_at(request, &d.socket, &d.dir.path().join("state"))
    };
    assert_eq!(manage(&request).unwrap()["state"], "recovery_required");
    request["action"] = "prepare_takeover".into();
    let token = manage(&request).unwrap()["token"].clone();
    request["action"] = "takeover".into();
    request["token"] = token;
    let operation = serde_json::json!({"action":"operation", "id":manage(&request).unwrap()["id"]});
    d.until(|| manage(&operation).unwrap()["done"] == true);
    assert!(
        manage(&operation).unwrap()["failure"].is_null(),
        "{}",
        manage(&operation).unwrap()
    );
    assert_eq!(pid, d.pid(pane));
    assert_eq!(
        manage(&serde_json::json!({"action":"status","daemon":env!("CARGO_BIN_EXE_jamd")}))
            .unwrap()["state"],
        "connected"
    );
}

#[test]
fn manual_agent_identity_survives_acknowledgement_and_ends_at_shell_return() {
    let d = Daemon::start();
    let initial = d.create();
    let pane = initial.panes[0].id.clone();
    d.send(&pane, "/bin/sh -c 'printf \"\\033]7501;state=working:app=codex\\007\"; read first; printf \"\\033]7501;state=done:app=codex\\007\"; read second'\r");
    d.until(|| d.state().panes[0].agent_provider.as_deref() == Some("codex"));
    assert_eq!(d.state().panes[0].agent_source, Some(AgentSource::Reported));
    assert!(d.state().panes[0].agent_role.is_none());
    d.send(&pane, "\r");
    d.until(|| d.state().panes[0].status == "done");
    assert!(matches!(
        d.request(Request::Acknowledge { pane: pane.clone() }),
        Message::Ok
    ));
    let cleared = d.state();
    assert_eq!(cleared.panes[0].status, "unknown");
    assert_eq!(cleared.panes[0].agent_provider.as_deref(), Some("codex"));
    d.send(&pane, "\r");
    d.until(|| d.state().panes[0].agent_provider.is_none());
    assert!(d.state().panes[0].agent_source.is_none());
}

#[test]
fn reported_agent_identity_is_not_restored_into_a_new_shell() {
    let mut d = Daemon::start();
    let state = d.create();
    let pane = state.panes[0].id.clone();
    d.send(
        &pane,
        "/bin/sh -c 'printf \"\\033]7501;state=idle:app=claude-code\\007\"; read answer'\r",
    );
    d.until(|| d.state().panes[0].agent_provider.as_deref() == Some("claude"));
    assert!(matches!(d.request(Request::Stop), Message::Ok));
    d.child.wait().unwrap();
    d.child = Daemon::launch(&d.socket, d.dir.path());
    d.until(|| UnixStream::connect(&d.socket).is_ok());
    let restored = d.state();
    assert!(restored.panes[0].agent_provider.is_none());
    assert!(restored.panes[0].agent_source.is_none());
    assert!(restored.panes[0].agent_role.is_none());
    assert!(restored.panes[0].reports.is_empty());
}
