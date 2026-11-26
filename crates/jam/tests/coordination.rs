use jam_core::*;
use serde_json::{Value, json};
use std::{
    os::unix::fs::PermissionsExt,
    process::{Child, Command},
    time::{Duration, Instant},
};
struct Fixture {
    child: Child,
    dir: tempfile::TempDir,
    socket: std::path::PathBuf,
}

#[test]
fn cancelled_codex_does_not_receive_queued_work_after_relaunch() {
    let directory = tempfile::tempdir().unwrap();
    let result = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "cancelled_codex_delivery_child",
            "--ignored",
            "--nocapture",
        ])
        .env("JAM_STATE_DIR", directory.path())
        .env("HOME", directory.path())
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin", directory.path().display()),
        )
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
#[ignore = "controlled child with an isolated profile and fake provider"]
fn cancelled_codex_delivery_child() {
    let root = std::path::PathBuf::from(std::env::var_os("JAM_STATE_DIR").unwrap());
    let transcript = root.join("conversation.jsonl");
    std::fs::write(
        &transcript,
        "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n",
    )
    .unwrap();
    let provider = root.join("codex");
    std::fs::write(
        &provider,
        "#!/bin/sh\nprintf '%s\\n' \"$*\" > \"$HOME/accepted-message\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&provider, std::fs::Permissions::from_mode(0o700)).unwrap();
    let id = jam_core::id();
    let mut agent = json!({"id":id,"pane":"fixture-pane","workspace":"fixture","root":"fixture","provider":"codex","role":"worker","verified_launch":true,"activity":"idle","run_id":"run","session":jam_core::id(),"transcript_path":transcript,"path":root});
    let mut board = coordination::Board::open().unwrap();
    board.put("agent", &agent).unwrap();
    // Only this child test runs; no other test or provider shares its environment.
    unsafe {
        std::env::set_var("JAM_AGENT_RUN_ID", "run");
    }
    recovery::record_hook(
        "fixture-pane",
        &json!({"hook_event_name":"SessionEnd","reason":"prompt_input_exit"}),
    );
    agent = board.get(&id).unwrap();
    board
        .action(json!({"action":"send","agent":id,"to":id,"text":"held assignment"}))
        .unwrap();
    let generation = recovery::begin_launch(&mut agent).unwrap();
    assert_eq!(agent["activity"], "cancelled");
    unsafe {
        std::env::set_var("JAM_AGENT_RUN_ID", generation);
    }
    coordination::start_delivery().unwrap();
    agent = board.get(&id).unwrap();
    agent["ready"] = json!(true);
    board.put("agent", &agent).unwrap();
    std::thread::sleep(Duration::from_millis(4500));
    assert!(
        !root.join("accepted-message").exists(),
        "Explicit exit must hold queued work across launch"
    );
    recovery::record_hook(
        "fixture-pane",
        &json!({"hook_event_name":"UserPromptSubmit"}),
    );
    recovery::record_hook("fixture-pane", &json!({"hook_event_name":"Stop"}));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !root.join("accepted-message").exists() {
        assert!(
            Instant::now() < deadline,
            "A new user turn should release queued work"
        );
        std::thread::sleep(Duration::from_millis(25));
    }
}
impl Fixture {
    fn launch(dir: &std::path::Path) -> Child {
        let mut command = Command::new(dir.join("Jam.app/Contents/MacOS/jamd"));
        if dir.join("test-shell").exists() {
            command.env("SHELL", dir.join("test-shell"));
        }
        command
            .env("HOME", dir)
            .env("JAM_SOCKET", dir.join("socket"))
            .env("JAM_STATE_DIR", dir.join("state"))
            .env(
                "PATH",
                format!(
                    "{}:{}:/usr/bin:/bin",
                    dir.display(),
                    std::path::Path::new(env!("CARGO_BIN_EXE_jam"))
                        .parent()
                        .unwrap()
                        .display()
                ),
            )
            .env("FAKE_LOG", dir.join("launches"))
            .spawn()
            .unwrap()
    }
    fn new() -> Self {
        Self::with_test_shell(false)
    }
    fn with_test_shell(test_shell: bool) -> Self {
        let dir = tempfile::tempdir().unwrap();
        std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700)).unwrap();
        let contents = dir.path().join("Jam.app/Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        std::fs::create_dir_all(contents.join("Resources/bin")).unwrap();
        std::fs::copy(env!("CARGO_BIN_EXE_jamd"), contents.join("MacOS/jamd")).unwrap();
        std::fs::copy(
            env!("CARGO_BIN_EXE_jam"),
            contents.join("Resources/bin/jam"),
        )
        .unwrap();
        let decoy = contents.join("MacOS/Jam");
        std::fs::write(&decoy, "#!/bin/sh\ntouch \"$HOME/wrong-gui-launched\"\n").unwrap();
        std::fs::set_permissions(decoy, std::fs::Permissions::from_mode(0o700)).unwrap();
        let fake = dir.path().join("claude");
        std::fs::write(&fake,"#!/bin/sh\nprintf '%s\\n' \"$*\" >> \"$FAKE_LOG\"\nmkdir -p \"$HOME/.claude/projects/fixture\"\nprintf 'fixture transcript' > \"$HOME/.claude/projects/fixture/$JAM_AGENT_ID.jsonl\"\nprintf '{\"session_id\":\"%s\",\"permission_mode\":\"bypassPermissions\",\"hook_event_name\":\"SessionStart\"}' \"$JAM_AGENT_ID\" | jam skill context --agent claude\nprintf '\\nREADY\\n'\nwhile read -r line; do :; done\n").unwrap();
        std::fs::set_permissions(fake, std::fs::Permissions::from_mode(0o700)).unwrap();
        if test_shell {
            let shell = dir.path().join("test-shell");
            std::fs::write(&shell, "#!/bin/sh\nprintf '%s\\n' \"$JAM_PANE_ID\" >> \"$HOME/shell-launches\"\nPS1='JAM_TEST_READY> ' exec /bin/sh\n").unwrap();
            std::fs::set_permissions(shell, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        let child = Self::launch(dir.path());
        let socket = dir.path().join("socket");
        let f = Self { child, dir, socket };
        f.wait(|| connect_to(&f.socket).is_ok());
        f
    }
    #[track_caller]
    fn wait(&self, mut f: impl FnMut() -> bool) {
        let until = Instant::now() + Duration::from_secs(10);
        while !f() {
            assert!(Instant::now() < until, "timed out");
            std::thread::sleep(Duration::from_millis(30));
        }
    }
    #[track_caller]
    fn rpc(&self, r: Request) -> Message {
        let description = match &r {
            Request::Coordination(value) => format!(
                "Coordination action={} agent={}",
                value["action"], value["agent"]
            ),
            other => format!("{other:?}"),
        };
        call_at(&self.socket, r).unwrap_or_else(|error| panic!("{description} failed: {error:#}"))
    }
    fn coord(&self, v: Value) -> Value {
        let Message::Coordination(v) = self.rpc(Request::Coordination(v)) else {
            panic!("bad response")
        };
        v
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = call_at(&self.socket, Request::Stop);
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
#[test]
fn restored_shell_agent_returns_to_a_usable_prompt_on_interrupt() {
    let mut f = Fixture::with_test_shell(true);
    let dir = f.dir.path();
    let session = jam_core::id();
    std::fs::write(dir.join("session"), &session).unwrap();
    std::fs::write(dir.join("claude"), r#"#!/bin/sh
session=$(cat "$HOME/session")
mkdir -p "$HOME/.claude/projects/test"
echo '{}' > "$HOME/.claude/projects/test/$session.jsonl"
printf '{"hook_event_name":"SessionStart","session_id":"%s","permission_mode":"bypassPermissions"}' "$session" | jam skill context --agent claude >/dev/null
echo "$*" >> "$FAKE_LOG"
echo PROVIDER_READY
exec /bin/cat
"#).unwrap();
    let Message::State(state) = f.rpc(Request::CreateWorkspace {
        path: dir.to_str().unwrap().into(),
        name: "test".into(),
    }) else {
        panic!("missing workspace")
    };
    let pane = &state.panes[0].id;
    let send = |f: &Fixture, data: &[u8]| {
        f.rpc(Request::Send {
            pane: pane.clone(),
            data: data.to_vec(),
        });
    };
    send(&f, b"claude --dangerously-skip-permissions\r");
    f.wait(|| matches!(f.rpc(Request::Capture { pane: pane.clone() }), Message::Capture(text) if text.contains("PROVIDER_READY")));
    let record = f.coord(json!({"action":"list"}))["agents"][0].clone();
    assert_eq!(record["shell_launched"], true);
    assert_eq!(record["session"], session);
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    f.child = Fixture::launch(dir);
    f.wait(|| connect_to(&f.socket).is_ok());
    f.wait(|| {
        std::fs::read_to_string(dir.join("launches"))
            .unwrap()
            .lines()
            .nth(1)
            .is_some_and(|args| args.contains(&format!("--resume {session}")))
    });
    let Message::State(restored) = f.rpc(Request::State) else {
        panic!()
    };
    send(&f, b"\x03");
    f.wait(|| matches!(f.rpc(Request::Capture { pane: pane.clone() }), Message::Capture(text) if text.contains("JAM_TEST_READY>")));
    send(&f, b"pwd > shell-cwd\r");
    let expected_cwd = dir.canonicalize().unwrap();
    f.wait(|| {
        std::fs::read_to_string(dir.join("shell-cwd"))
            .is_ok_and(|cwd| std::path::Path::new(cwd.trim()) == expected_cwd)
    });
    let Message::State(state) = f.rpc(Request::State) else {
        panic!()
    };
    assert_eq!(
        state.panes[0].session_generation,
        restored.panes[0].session_generation
    );
    assert!(!state.panes[0].exited);
    assert!(state.panes[0].agent_role.is_none() && state.panes[0].agent_provider.is_none());
    assert!(
        f.coord(json!({"action":"list"}))["agents"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // A new foreground job can claim this terminal. This provider intentionally
    // sends no SessionEnd hook, including on this second, shell-owned launch.
    send(&f, b"claude\r");
    f.wait(|| {
        std::fs::read_to_string(dir.join("launches"))
            .unwrap()
            .lines()
            .count()
            == 3
    });
    send(&f, b"\x03");
    f.wait(|| {
        f.coord(json!({"action":"list"}))["agents"]
            .as_array()
            .unwrap()
            .is_empty()
    });
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    f.child = Fixture::launch(dir);
    f.wait(|| connect_to(&f.socket).is_ok());
    f.rpc(Request::Restart { pane: pane.clone() });
    f.wait(|| matches!(f.rpc(Request::Capture { pane: pane.clone() }), Message::Capture(text) if text.contains("JAM_TEST_READY>")));
    assert_eq!(
        std::fs::read_to_string(dir.join("launches"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    let dedicated = f.coord(json!({"action":"start","workspace":state.workspaces[0].id,"provider":"claude","role":"worker"}));
    let dedicated_pane = dedicated["agent"]["pane"].as_str().unwrap();
    f.wait(|| matches!(f.rpc(Request::Capture { pane: dedicated_pane.into() }), Message::Capture(text) if text.contains("PROVIDER_READY")));
    f.rpc(Request::Send {
        pane: dedicated_pane.into(),
        data: vec![3],
    });
    f.wait(|| matches!(f.rpc(Request::State), Message::State(state) if state.panes.iter().any(|p| p.id == dedicated_pane && p.exited && p.agent_resume.is_some())));
}

#[test]
fn takeover_commit_resumes_saved_managed_panes_only() {
    let mut f = Fixture::new();
    let Message::State(state) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "test".into(),
    }) else {
        panic!("missing workspace");
    };
    let shell = state.panes[0].id.clone();
    let workspace = &state.workspaces[0].id;
    let start = |role| {
        f.coord(json!({"action":"start","workspace":workspace,"provider":"claude","role":role}))["agent"]["pane"].as_str().unwrap().to_owned()
    };
    let running = start("coordinator");
    let not_in_checkpoint = start("worker");
    f.wait(|| {
        f.coord(json!({"action":"list"}))["agents"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["ready"] == true && a["session"].is_string())
    });
    f.rpc(Request::Stop);
    f.child.wait().unwrap();

    let token = jam_core::id();
    let marker = f.dir.path().join("state/.upgrade-boot.json");
    std::fs::write(
        &marker,
        serde_json::to_vec(&json!({"token":token,"resume_panes":[running,shell]})).unwrap(),
    )
    .unwrap();
    std::fs::set_permissions(&marker, std::fs::Permissions::from_mode(0o600)).unwrap();
    f.child = Fixture::launch(f.dir.path());
    f.wait(|| connect_to(&f.socket).is_ok());
    assert!(
        matches!(f.rpc(Request::State), Message::State(state) if state.panes.iter().all(|p| p.exited))
    );
    assert!(matches!(
        f.rpc(Request::CommitUpgrade { token }),
        Message::Ok
    ));
    f.wait(|| matches!(f.rpc(Request::Capture { pane: running.clone() }), Message::Capture(text) if text.contains("READY")));
    assert!(
        matches!(f.rpc(Request::State), Message::State(state) if state.panes.iter().filter(|p| !p.exited).map(|p| &p.id).eq([&running]))
    );
    assert!(
        matches!(f.rpc(Request::State), Message::State(state) if state.panes.iter().any(|p| p.id == not_in_checkpoint && p.exited))
    );
}

#[test]
fn coordinator_launch_registration_and_permission_preserving_restart() {
    let mut f = Fixture::new();
    let Message::State(s) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "test".into(),
    }) else {
        panic!()
    };
    let workspace = s.workspaces[0].id.clone();
    let started = f.coord(json!({"action":"start","workspace":workspace,"provider":"claude"}));
    let pane = started["agent"]["pane"].as_str().unwrap().to_owned();
    let metadata = started["State"]["panes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["id"] == pane)
        .unwrap();
    assert_eq!(metadata["agent_provider"], "claude");
    assert_eq!(metadata["agent_role"], "coordinator");
    assert_eq!(metadata["agent_resume"]["verified_launch"], true);
    f.wait(|| f.coord(json!({"action":"list"}))["agents"][0]["ready"] == true);
    assert_eq!(
        f.coord(json!({"action":"list"}))["agents"][0]["bypass"],
        true
    );
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    f.child = Fixture::launch(f.dir.path());
    f.wait(|| connect_to(&f.socket).is_ok());
    f.wait(|| {
        std::fs::read_to_string(f.dir.path().join("launches"))
            .unwrap_or_default()
            .lines()
            .count()
            >= 2
    });
    let launches = std::fs::read_to_string(f.dir.path().join("launches")).unwrap();
    assert!(
        launches
            .lines()
            .next()
            .unwrap()
            .contains("--dangerously-skip-permissions"),
        "Claude coordinators must start unattended on their first launch"
    );
    assert!(
        launches
            .lines()
            .nth(1)
            .unwrap()
            .contains("--dangerously-skip-permissions")
    );
    assert!(
        !launches.lines().next().unwrap().contains("You are a Jam"),
        "Role instructions belong in hook context, not a user message"
    );
    assert!(launches.lines().nth(1).unwrap().contains("--resume"));
    assert!(
        !launches.lines().nth(1).unwrap().contains("You are a Jam"),
        "Resuming must not submit another startup turn"
    );
    f.wait(|| matches!(f.rpc(Request::Capture{pane:pane.clone()}),Message::Capture(text) if text.contains("READY")));
    let peer_path = f.dir.path().join("peer");
    let listener = std::os::unix::net::UnixListener::bind(&peer_path).unwrap();
    listener.set_nonblocking(true).unwrap();
    let agent = started["agent"]["id"].as_str().unwrap();
    f.coord(json!({"action":"register","pane":pane,"session":agent,"permission_mode":"bypassPermissions","ready":true,"socket":peer_path,"token":"test-token"}));
    let message = f.coord(json!({"action":"send","agent":agent,"to":agent,"text":"fixture task"}));
    let deadline = Instant::now() + Duration::from_secs(10);
    let stream = loop {
        if let Ok((stream, _)) = listener.accept() {
            break stream;
        };
        assert!(Instant::now() < deadline);
        std::thread::sleep(Duration::from_millis(20));
    };
    let mut stream = stream;
    stream.set_nonblocking(true).unwrap();
    use std::io::Read;
    let mut data = Vec::new();
    while data.iter().filter(|b| **b == b'\n').count() < 2 {
        let mut bytes = [0u8; 4096];
        match stream.read(&mut bytes) {
            Ok(0) => panic!("peer closed before sending its frames"),
            Ok(n) => data.extend_from_slice(&bytes[..n]),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => panic!("peer read failed: {e}"),
        }
    }
    let lines: Vec<Value> = data
        .split(|b| *b == b'\n')
        .filter(|l| !l.is_empty())
        .map(|l| serde_json::from_slice(l).unwrap())
        .collect();
    assert_eq!(lines[0]["type"], "auth");
    assert!(
        lines[1]["message"]["content"]
            .as_str()
            .unwrap()
            .contains(message["id"].as_str().unwrap())
    );
    f.wait(|| f.coord(json!({"action":"inbox","agent":agent}))[0]["state"] == "transport_accepted");
    f.coord(json!({"action":"ack","agent":agent,"message":message["id"]}));
    assert!(
        f.coord(json!({"action":"inbox","agent":agent}))
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert!(!f.dir.path().join("wrong-gui-launched").exists());
    let Message::State(state) = f.rpc(Request::State) else {
        panic!()
    };
    assert_eq!(
        state.workspaces[0].tabs.len(),
        1,
        "Coordinator must not add a tab"
    );
    assert_eq!(
        state.workspaces[0].coordinator.as_ref().unwrap()["pane"],
        pane
    );
    f.rpc(Request::Close { pane: pane.clone() });
    assert!(
        f.coord(json!({"action":"list"}))["agents"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        f.coord(json!({"action":"list", "include_archived":true}))["agents"][0]["paused"],
        true
    );
    let Message::State(closed) = f.rpc(Request::State) else {
        panic!()
    };
    assert!(closed.workspaces[0].coordinator.is_none());
    // Upgrade a legacy closed tab, which was previously only marked paused.
    let board = coordination::Board::at(&f.dir.path().join("state/coordination.sqlite")).unwrap();
    let mut legacy = board.get(agent).unwrap();
    legacy.as_object_mut().unwrap().remove("closed");
    board.put("agent", &legacy).unwrap();
    let Message::State(migrated) = f.rpc(Request::State) else {
        panic!()
    };
    assert!(migrated.workspaces[0].coordinator.is_none());
    // State may return its cached layout while the database worker finishes
    // marking the legacy coordinator closed.
    f.wait(|| board.get(agent).unwrap()["closed"] == true);

    assert!(
        call_at(
            &f.socket,
            Request::Coordination(json!({"action":"open","workspace":workspace}))
        )
        .unwrap_err()
        .to_string()
        .contains("No coordinator")
    );
    let next = f.coord(json!({"action":"start","workspace":workspace,"provider":"claude"}));
    assert_ne!(next["agent"]["id"], agent);
    assert_eq!(
        next["State"]["workspaces"][0]["coordinator"]["pane"],
        next["agent"]["pane"]
    );
    let history = f.coord(json!({"action":"list", "include_archived":true}));
    assert_eq!(history["agents"][0]["closed"], true);
    assert!(history["agents"][0]["session"].is_string());
    let next_pane = next["agent"]["pane"].as_str().unwrap().to_owned();
    f.wait(|| matches!(f.rpc(Request::Capture {pane:next_pane.clone()}), Message::Capture(text) if text.contains("READY")));
    assert!(
        call_at(
            &f.socket,
            Request::Split {
                pane: next_pane.clone(),
                vertical: false,
            }
        )
        .unwrap_err()
        .to_string()
        .contains("cannot be split")
    );
    let Message::State(remaining) = f.rpc(Request::Close { pane: next_pane }) else {
        panic!()
    };
    assert!(remaining.workspaces[0].coordinator.is_none());
    assert!(
        remaining.workspaces[0]
            .tabs
            .iter()
            .all(|t| t.agent_role.as_deref() != Some("coordinator"))
    );
}

#[test]
fn configured_integrations_and_worker_limits_are_enforced() {
    let f = Fixture::new();
    let mut settings = config::HostSettings::from(config::Config::default());
    settings.max_agents_per_coordinator = 1;
    assert!(matches!(
        f.rpc(Request::Configure {
            settings: settings.clone()
        }),
        Message::Settings(_)
    ));
    let Message::State(s) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "limits".into(),
    }) else {
        panic!()
    };
    let workspace = &s.workspaces[0].id;
    let owner = f.coord(json!({"action":"start","workspace":workspace,"provider":"claude"}));
    let worker = f.coord(json!({"action":"start","workspace":workspace,"provider":"claude","role":"worker","parent":owner["agent"]["id"]}));
    f.wait(|| {
        f.coord(json!({"action":"list"}))["agents"]
            .as_array()
            .unwrap()
            .iter()
            .all(|a| a["session"].is_string())
    });
    assert!(
        matches!(call_at(&f.socket, Request::Coordination(json!({"action":"start","workspace":workspace,"provider":"claude","role":"worker"}))).map_err(|e| e.to_string()), Err(e) if e.contains("limit"))
    );
    settings.enabled_agents.clear();
    f.rpc(Request::Configure { settings });
    assert!(
        matches!(call_at(&f.socket, Request::Coordination(json!({"action":"start","workspace":workspace,"provider":"claude"}))).map_err(|e| e.to_string()), Err(e) if e.contains("disabled"))
    );
    let Message::State(state) = f.rpc(Request::State) else {
        panic!()
    };
    let pane = worker["agent"]["pane"].as_str().unwrap();
    assert!(
        !state.panes.iter().find(|p| p.id == pane).unwrap().exited,
        "Disabling integration must not kill running agents"
    );
    assert!(
        matches!(
            f.rpc(Request::NewTab {
                workspace: workspace.clone()
            }),
            Message::State(_)
        ),
        "Ordinary shells remain available"
    );
}

fn install_fake_codex(f: &Fixture) {
    let script = r#"#!/usr/bin/python3
import json, os, sys, uuid
from pathlib import Path
home = Path(os.environ['HOME'])
log = home / 'codex-log'
sessions = home / 'codex-sessions.json'
def record(value):
    with log.open('a') as out: out.write(json.dumps(value)+'\n')
def saved():
    return json.loads(sessions.read_text()) if sessions.exists() else {}
if 'app-server' in sys.argv:
    if (home / 'fail-helper').exists():
        print('Invalid Codex configuration: fixture diagnostic', file=sys.stderr, flush=True); sys.exit(2)
    record({'helper': True, 'inherited_thread': os.environ.get('CODEX_THREAD_ID')})
    for line in sys.stdin:
        request = json.loads(line)
        if 'id' not in request: continue
        method, params = request['method'], request.get('params', {})
        record({'method': method, 'params': params})
        result, error = {}, None
        if method == 'config/read': result = {'config': {'developer_instructions': 'User custom instructions'}}
        elif method in ['thread/start', 'thread/resume']:
            identifier = params.get('threadId', str(uuid.uuid4()))
            if method == 'thread/resume' and identifier not in saved(): error = {'message': 'saved conversation missing'}
            result = {'thread': {'id': identifier}, 'model': 'fixture-model'}
        elif method == 'thread/name/set':
            data = saved(); data[params['threadId']] = {'id': params['threadId'], 'name': params['name'], 'cwd': os.getcwd()}
            sessions.write_text(json.dumps(data))
        elif method == 'thread/read':
            if params['threadId'] not in saved(): error = {'message': 'thread is not durable'}
            result = {'thread': saved().get(params['threadId'], {})}
        elif method == 'thread/list': result = {'data': list(saved().values())}
        response = {'id': request['id']}
        response['error' if error else 'result'] = error or result
        print(json.dumps(response), flush=True)
else:
    record({'argv': sys.argv[1:], 'inherited_thread': os.environ.get('CODEX_THREAD_ID')})
    assert 'resume' in sys.argv and sys.argv[-1] in saved(), 'TUI needs an already durable identity'
    assert '--no-daemon' in sys.argv
    assert os.environ.get('CODEX_THREAD_ID') is None
    assert any(a.startswith('developer_instructions=') and 'User custom instructions' in a and any(role in a for role in ['Jam coordinator', 'Jam worker', 'Jam reviewer']) for a in sys.argv)
    print('CODEX READY', flush=True)
    for line in sys.stdin: pass
"#;
    let path = f.dir.path().join("codex");
    std::fs::write(&path, script).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
}

#[test]
fn codex_identity_is_durable_before_first_turn_and_recovers_without_prompts() {
    let mut f = Fixture::new();
    install_fake_codex(&f);
    let Message::State(s) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "codex".into(),
    }) else {
        panic!()
    };
    let workspace = s.workspaces[0].id.clone();
    let first = f.coord(json!({"action":"start","workspace":workspace,"provider":"codex"}));
    let pane = first["agent"]["pane"].as_str().unwrap().to_owned();
    let agent = first["agent"]["id"].as_str().unwrap().to_owned();
    f.wait(|| matches!(f.rpc(Request::Capture {pane:pane.clone()}), Message::Capture(s) if s.contains("CODEX READY")));
    let board = f.coord(json!({"action":"list"}));
    let session = board["agents"][0]["session"].as_str().unwrap().to_owned();
    assert_eq!(board["agents"][0]["model"], "fixture-model");
    let Message::State(s) = f.rpc(Request::State) else {
        panic!()
    };
    assert_eq!(s.workspaces[0].coordinator.as_ref().unwrap()["id"], agent);
    assert_eq!(s.workspaces[0].tabs.len(), 1);
    let ordinary_tab = s.workspaces[0].tabs[0].id.clone();
    assert!(
        call_at(
            &f.socket,
            Request::Coordination(
                json!({"action":"start","workspace":workspace,"provider":"claude"})
            )
        )
        .unwrap_err()
        .to_string()
        .contains("already has a coordinator")
    );
    assert!(
        call_at(
            &f.socket,
            Request::MovePane {
                pane: pane.clone(),
                destination: PaneDestination::Workspace {
                    workspace: workspace.clone()
                }
            }
        )
        .unwrap_err()
        .to_string()
        .contains("own surface")
    );
    let Message::State(s) = f.rpc(Request::MoveTab {
        tab: ordinary_tab.clone(),
        workspace: workspace.clone(),
        index: 0,
    }) else {
        panic!()
    };
    assert_eq!(s.workspaces[0].tabs[0].id, ordinary_tab);
    assert_eq!(s.workspaces[0].coordinator.as_ref().unwrap()["pane"], pane);
    assert!(
        call_at(
            &f.socket,
            Request::Coordination(json!({"action":"recover","agent":agent,"session":null}))
        )
        .unwrap_err()
        .to_string()
        .contains("Stop the coordinator")
    );
    let choices = f.coord(json!({"action":"sessions","agent":agent}));
    assert_eq!(choices[0]["session"], session);
    assert!(
        choices[0].get("id").is_none(),
        "Provider session IDs must not be qualified as Jam IDs on remotes"
    );
    // Previously saved sandbox/approval flags must not undo unattended mode.
    let persisted =
        coordination::Board::at(&f.dir.path().join("state/coordination.sqlite")).unwrap();
    let mut legacy = persisted.get(&agent).unwrap();
    legacy["bypass"] = json!(false);
    legacy["launch_flags"] = json!([
        "--sandbox",
        "workspace-write",
        "--ask-for-approval",
        "on-request"
    ]);
    persisted.put("agent", &legacy).unwrap();
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    f.child = Fixture::launch(f.dir.path());
    f.wait(|| connect_to(&f.socket).is_ok());
    f.wait(|| matches!(f.rpc(Request::Capture {pane:pane.clone()}), Message::Capture(s) if s.contains("CODEX READY")));
    assert_eq!(
        f.coord(json!({"action":"list"}))["agents"][0]["session"],
        session
    );
    let log = std::fs::read_to_string(f.dir.path().join("codex-log")).unwrap();
    let launches: Vec<Value> = log
        .lines()
        .map(|s| serde_json::from_str(s).unwrap())
        .filter(|v: &Value| v["argv"].is_array())
        .collect();
    assert_eq!(launches.len(), 2);
    for call in log
        .lines()
        .map(|s| serde_json::from_str::<Value>(s).unwrap())
        .filter(|v| v["method"] == "thread/start" || v["method"] == "thread/resume")
    {
        assert_eq!(call["params"]["approvalPolicy"], "never");
        assert_eq!(call["params"]["sandbox"], "danger-full-access");
    }
    for launch in launches {
        let args = launch["argv"].as_array().unwrap();
        assert!(args.contains(&json!("--yolo")));
        assert!(!args.contains(&json!("--sandbox")));
        assert!(!args.contains(&json!("--ask-for-approval")));
        assert_eq!(args.last().unwrap(), &session);
        assert!(launch["inherited_thread"].is_null());
        assert_eq!(
            args.iter()
                .filter(|a| a
                    .as_str()
                    .is_some_and(|s| s.starts_with("developer_instructions=")))
                .count(),
            1
        );
    }
    // A missing provider conversation remains visible with its full diagnostic.
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    std::fs::remove_file(f.dir.path().join("codex-sessions.json")).unwrap();
    f.child = Fixture::launch(f.dir.path());
    f.wait(|| connect_to(&f.socket).is_ok());
    f.wait(|| matches!(f.rpc(Request::State), Message::State(s) if s.panes.iter().any(|p|p.id==pane && p.launch_error.as_deref().is_some_and(|e|e.contains("saved conversation missing")))));
    f.coord(json!({"action":"recover","agent":agent,"session":null}));
    f.wait(|| matches!(f.rpc(Request::Capture {pane:pane.clone()}), Message::Capture(s) if s.contains("CODEX READY")));
    let recovered = f.coord(json!({"action":"list"}));
    assert_ne!(recovered["agents"][0]["session"], session);
    assert_eq!(recovered["agents"][0]["previous_sessions"][0], session);
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    std::fs::write(f.dir.path().join("fail-helper"), "").unwrap();
    f.child = Fixture::launch(f.dir.path());
    f.wait(|| connect_to(&f.socket).is_ok());
    f.wait(|| matches!(f.rpc(Request::State), Message::State(s) if s.panes.iter().any(|p|p.id==pane && p.launch_error.as_deref().is_some_and(|e|e.contains("Invalid Codex configuration: fixture diagnostic")))));
    // Startup can fail before the daemon socket is ready. The diagnostic is
    // still durable, rather than relying on an RPC back to the parent daemon.
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    let board_path = f.dir.path().join("state/coordination.sqlite");
    let board = coordination::Board::at(&board_path).unwrap();
    let mut record = board.get(&agent).unwrap();
    record["launch_error"] = Value::Null;
    board.put("agent", &record).unwrap();
    let output = Command::new(f.dir.path().join("Jam.app/Contents/Resources/bin/jam"))
        .args(["coordinate", "run", &agent])
        .env("HOME", f.dir.path())
        .env("JAM_SOCKET", f.dir.path().join("unavailable-socket"))
        .env("JAM_STATE_DIR", f.dir.path().join("state"))
        .env("PATH", format!("{}:/usr/bin:/bin", f.dir.path().display()))
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        board.get(&agent).unwrap()["launch_error"]
            .as_str()
            .unwrap()
            .contains("Invalid Codex configuration: fixture diagnostic")
    );
}

#[test]
fn a_blank_worker_receives_and_acknowledges_its_first_assignment() {
    let mut f = Fixture::new();
    let fake = r#"#!/usr/bin/python3
import json, os, socket, subprocess
from pathlib import Path
home = Path(os.environ['HOME'])
agent = os.environ['JAM_AGENT_ID']
sock = home / ('peer-' + agent)
if sock.exists(): sock.unlink()
listener = socket.socket(socket.AF_UNIX)
listener.bind(str(sock)); listener.listen()
os.environ['CLAUDE_CODE_MESSAGING_SOCKET'] = str(sock)
os.environ['CLAUDE_CODE_MESSAGING_TOKEN'] = 'fixture-only'
def hook(event):
    subprocess.run(['jam','skill','context','--agent','claude'],input=json.dumps({'hook_event_name':event,'source':'startup','session_id':agent}).encode(),stdout=subprocess.DEVNULL,check=True)
def call(*args):
    return json.loads(subprocess.check_output(['jam','--json','coordinate',*args]))['Coordination']
hook('SessionStart')
print('WAITING FOR FIRST TASK', flush=True)
while True:
    connection, _ = listener.accept()
    with connection:
        stream = connection.makefile()
        auth = json.loads(stream.readline())
        assert auth['token'] == 'fixture-only'
        payload = json.loads(stream.readline())
        assert payload['type'] == 'user'
        hook('UserPromptSubmit')
        for message in call('inbox','--agent',agent):
            call('ack',message['id'],'--agent',agent)
        hook('Stop')
        print('ASSIGNMENT RECEIVED', flush=True)
"#;
    let path = f.dir.path().join("claude");
    std::fs::write(&path, fake).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let Message::State(s) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "delivery".into(),
    }) else {
        panic!()
    };
    let w = &s.workspaces[0].id;
    let owner =
        f.coord(json!({"action":"start","workspace":w,"provider":"claude"}))["agent"].clone();
    let worker = f.coord(json!({"action":"start","workspace":w,"provider":"claude","role":"worker","parent":owner["id"]}))["agent"].clone();
    let task = f.coord(json!({"action":"task","owner":owner["id"],"objective":"first task without an initial user prompt"}));
    let assigned = f.coord(json!({"action":"assign","task":task["id"],"agent":worker["id"]}));
    assert!(assigned["delivery_status"].is_string());
    f.wait(|| {
        f.coord(json!({"action":"list","include_archived":true}))["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["task"] == task["id"] && m["state"] == "acknowledged")
    });
    let list = f.coord(json!({"action":"list"}));
    assert_eq!(list["tasks"][0]["delivery_status"], "acknowledged");
    f.wait(|| matches!(f.rpc(Request::Capture {pane:worker["pane"].as_str().unwrap().into()}), Message::Capture(text) if text.contains("ASSIGNMENT RECEIVED")));
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    f.child = Fixture::launch(f.dir.path());
    f.wait(|| connect_to(&f.socket).is_ok());
    let second = f.coord(json!({"action":"send","agent":owner["id"],"to":worker["id"],"text":"second delivery after Stop"}));
    f.wait(|| {
        f.coord(json!({"action":"list","include_archived":true}))["messages"]
            .as_array()
            .unwrap()
            .iter()
            .any(|m| m["id"] == second["id"] && m["state"] == "acknowledged")
    });
}

#[test]
fn create_with_initial_agent_never_starts_a_placeholder_shell() {
    let f = Fixture::with_test_shell(true);
    let directory = f.dir.path().join("repo");
    std::fs::create_dir(&directory).unwrap();
    for args in [
        vec!["init", "-b", "main"],
        vec![
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.invalid",
            "commit",
            "--allow-empty",
            "-m",
            "initial",
        ],
    ] {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(&directory)
                .output()
                .unwrap()
                .status
                .success()
        );
    }
    let Message::State(root) = f.rpc(Request::CreateWorkspace {
        path: directory.to_string_lossy().into(),
        name: "repo".into(),
    }) else {
        panic!()
    };
    f.wait(|| f.dir.path().join("shell-launches").exists());
    let initial_shells = std::fs::read_to_string(f.dir.path().join("shell-launches")).unwrap();
    assert_eq!(initial_shells.lines().count(), 1);
    let root_id = &root.workspaces[0].id;
    let owner =
        f.coord(json!({"action":"start","workspace":root_id,"provider":"claude"}))["agent"].clone();
    let destination = f.dir.path().join("first-worker");
    let output = Command::new(f.dir.path().join("Jam.app/Contents/Resources/bin/jam"))
        .args([
            "--json",
            "worktree",
            "create",
            "--workspace",
            root_id,
            "--branch",
            "first-worker",
            "--path",
            destination.to_str().unwrap(),
            "--agent",
            "claude",
            "--parent",
            owner["id"].as_str().unwrap(),
            "--tab-name",
            "Implementation",
        ])
        .env("HOME", f.dir.path())
        .env("JAM_SOCKET", &f.socket)
        .env("JAM_STATE_DIR", f.dir.path().join("state"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let created: Value = serde_json::from_slice(&output.stdout).unwrap();
    let created = &created["CreatedWorkspace"];
    let workspace = created["workspace"].as_str().unwrap();
    let Message::State(state) = f.rpc(Request::State) else {
        panic!()
    };
    let child = state.workspaces.iter().find(|w| w.id == workspace).unwrap();
    assert_eq!(child.parent.as_deref(), Some(root_id.as_str()));
    assert_eq!(child.tabs.len(), 1);
    assert_eq!(child.tabs[0].name, "Implementation");
    assert_eq!(child.tabs[0].agent_role.as_deref(), Some("worker"));
    assert_eq!(
        state
            .panes
            .iter()
            .filter(|p| p.workspace == workspace)
            .count(),
        1
    );
    assert_eq!(child.tabs[0].id, created["tab"]);
    let agent = f.coord(json!({"action":"list"}))["agents"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["id"] == created["agent"])
        .unwrap()
        .clone();
    assert_eq!(agent["pane"], created["pane"]);
    assert_eq!(agent["parent"], owner["id"]);
    assert_eq!(agent["root"], *root_id);
    f.wait(|| matches!(f.rpc(Request::Capture {pane:created["pane"].as_str().unwrap().into()}),Message::Capture(text) if text.contains("READY")));
    // Check the standalone workspace path as well, with the same directory already open.
    let Message::Operation { id, .. } = f.rpc(Request::CreateWorkspaceWithTab {
        location: WorkspaceLocation::Directory {
            path: directory.to_string_lossy().into(),
            name: "Agent only".into(),
        },
        tab: InitialTab::Agent {
            provider: "claude".into(),
            role: "coordinator".into(),
            complex: false,
            parent: None,
            name: None,
        },
    }) else {
        panic!()
    };
    f.wait(|| {
        matches!(
            f.rpc(Request::Operation { id: id.clone() }),
            Message::CreatedWorkspace { .. }
        )
    });
    let Message::CreatedWorkspace { workspace, .. } = f.rpc(Request::Operation { id }) else {
        panic!()
    };
    let Message::State(state) = f.rpc(Request::State) else {
        panic!()
    };
    let independent = state.workspaces.iter().find(|w| w.id == workspace).unwrap();
    assert!(independent.tabs.is_empty());
    assert!(independent.coordinator.is_some());
    assert_eq!(independent.parent, None);

    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("shell-launches")).unwrap(),
        initial_shells,
        "Agent-only creation must never start a placeholder shell"
    );

    // A named ordinary terminal is also a single initial tab.
    let output = Command::new(env!("CARGO_BIN_EXE_jam"))
        .args([
            "--json",
            "workspace",
            "create",
            directory.to_str().unwrap(),
            "--tab-name",
            "Server",
        ])
        .env("JAM_SOCKET", &f.socket)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let created: Value = serde_json::from_slice(&output.stdout).unwrap();
    let created = &created["CreatedWorkspace"];
    assert!(created["agent"].is_null());
    let Message::State(state) = f.rpc(Request::State) else {
        panic!()
    };
    let terminal = state
        .workspaces
        .iter()
        .find(|w| w.id == created["workspace"])
        .unwrap();
    assert_eq!(terminal.tabs.len(), 1);
    assert_eq!(terminal.tabs[0].name, "Server");
    assert_eq!(terminal.tabs[0].agent_role, None);
    f.wait(|| {
        std::fs::read_to_string(f.dir.path().join("shell-launches"))
            .unwrap()
            .lines()
            .count()
            == 2
    });

    // Validation precedes creating any checkout or branch.
    let mut settings = config::HostSettings::from(config::Config::default());
    settings.enabled_agents.clear();
    f.rpc(Request::Configure { settings });
    let invalid = f.dir.path().join("must-not-exist");
    assert!(
        call_at(
            &f.socket,
            Request::CreateWorkspaceWithTab {
                location: WorkspaceLocation::Worktree {
                    name: String::new(),
                    workspace: root_id.clone(),
                    branch: "must-not-exist".into(),
                    base: "HEAD".into(),
                    path: Some(invalid.to_string_lossy().into())
                },
                tab: InitialTab::Agent {
                    provider: "claude".into(),
                    role: "worker".into(),
                    complex: false,
                    parent: Some(owner["id"].as_str().unwrap().into()),
                    name: None
                },
            }
        )
        .unwrap_err()
        .to_string()
        .contains("disabled")
    );
    assert!(!invalid.exists());
    assert!(
        !Command::new("git")
            .args(["show-ref", "--verify", "refs/heads/must-not-exist"])
            .current_dir(&directory)
            .output()
            .unwrap()
            .status
            .success()
    );
}

#[test]
fn role_models_names_and_existing_sessions_follow_saved_policy() {
    let f = Fixture::new();
    let Message::State(state) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "models".into(),
    }) else {
        panic!()
    };
    let workspace = &state.workspaces[0].id;
    let mut settings = config::HostSettings::from(config::Config::default());
    settings.agent_models.insert(
        "claude".into(),
        config::AgentModels {
            coordinator: "coordinator-model".into(),
            worker: "worker-model".into(),
            reviewer: "review-model".into(),
            complex_work: "complex-model".into(),
        },
    );
    f.rpc(Request::Configure {
        settings: settings.clone(),
    });
    let owner =
        f.coord(json!({"action":"start","workspace":workspace,"provider":"claude"}))["agent"]
            .clone();
    assert_eq!(owner["model"], "coordinator-model");
    for (role, complex, model, name) in [
        ("worker", false, "worker-model", "Implement README"),
        ("worker", true, "complex-model", "Refactor parser"),
        ("reviewer", false, "review-model", "Review README"),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_jam"))
            .args([
                "--json",
                "coordinate",
                "start",
                "--workspace",
                workspace,
                "--provider",
                "claude",
                "--role",
                role,
                "--parent",
                owner["id"].as_str().unwrap(),
                "--tab-name",
                name,
            ])
            .args(if complex { vec!["--complex"] } else { vec![] })
            .env("JAM_SOCKET", &f.socket)
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        let reply: Value = serde_json::from_slice(&result.stdout).unwrap();
        let created = &reply["Coordination"];
        assert_eq!(created["agent"]["model"], model);
        assert_eq!(created["agent"]["role"], role);
        let pane = created["agent"]["pane"].as_str().unwrap();
        let tab = created["State"]["workspaces"][0]["tabs"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["root"]["Leaf"] == pane)
            .unwrap();
        assert_eq!(tab["name"], name);
        f.wait(|| matches!(f.rpc(Request::Capture {pane:pane.into()}),Message::Capture(text) if text.contains("READY")));
    }
    let launches = std::fs::read_to_string(f.dir.path().join("launches")).unwrap();
    for model in [
        "coordinator-model",
        "worker-model",
        "complex-model",
        "review-model",
    ] {
        assert!(launches.contains(&format!("--model {model}")));
    }
    settings.agent_models.clear();
    f.rpc(Request::Configure { settings });
    let prior = f.coord(json!({"action":"list"}));
    assert_eq!(prior["agents"][0]["model"], "coordinator-model");
    let blank = f
        .coord(json!({"action":"start","workspace":workspace,"provider":"claude","role":"worker"}));
    assert!(
        blank["agent"]["model"].is_null(),
        "A default worker must not inherit its coordinator's model"
    );
    let pane = blank["agent"]["pane"].as_str().unwrap();
    let tab = blank["State"]["workspaces"][0]["tabs"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["root"]["Leaf"] == pane)
        .unwrap();
    assert_eq!(tab["name"], "Worker 1");
    let blank2 = f
        .coord(json!({"action":"start","workspace":workspace,"provider":"claude","role":"worker"}));
    assert!(
        blank2["State"]["workspaces"][0]["tabs"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["name"] == "Worker 2")
    );
    assert!(call_at(&f.socket, Request::Coordination(json!({"action":"start","workspace":workspace,"provider":"claude","role":"reviewer","complex":true}))).is_err());
}

#[test]
fn restart_disconnects_old_terminal_subscribers() {
    let f = Fixture::with_test_shell(true);
    let Message::State(state) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "restart".into(),
    }) else {
        panic!()
    };
    let pane = state.panes[0].id.clone();
    let mut previous = connect_to(&f.socket).unwrap();
    previous
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write_frame(&mut previous, &Request::Attach { pane: pane.clone() }).unwrap();
    assert!(matches!(
        read_frame::<Message>(&mut previous).unwrap(),
        Message::Snapshot { .. }
    ));
    f.wait(|| matches!(f.rpc(Request::Capture { pane:pane.clone() }), Message::Capture(text) if text.contains("JAM_TEST_READY>")));
    f.rpc(Request::Send {
        pane: pane.clone(),
        data: b"exit\r".to_vec(),
    });
    f.wait(|| matches!(f.rpc(Request::State), Message::State(s) if s.panes.iter().any(|p| p.id == pane && p.exited)));
    f.rpc(Request::Restart { pane: pane.clone() });
    // An EOF tells existing mirrors to reconnect. A timeout leaves the old exit banner forever.
    loop {
        match read_frame::<Message>(&mut previous) {
            Ok(_) => (),
            Err(error) => {
                assert!(
                    !error.to_string().contains("timed out")
                        && !error.to_string().contains("temporarily unavailable"),
                    "Old subscriber was never disconnected: {error}"
                );
                break;
            }
        }
    }
    let mut replacement = connect_to(&f.socket).unwrap();
    replacement
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    write_frame(&mut replacement, &Request::Attach { pane: pane.clone() }).unwrap();
    assert!(matches!(
        read_frame::<Message>(&mut replacement).unwrap(),
        Message::Snapshot { .. }
    ));
    f.wait(|| matches!(f.rpc(Request::Capture { pane:pane.clone() }), Message::Capture(text) if text.contains("JAM_TEST_READY>")));
    f.rpc(Request::Send {
        pane: pane.clone(),
        data: b"printf 'RESUMED\\n'\r".to_vec(),
    });
    f.wait(|| matches!(f.rpc(Request::Capture {pane:pane.clone()}), Message::Capture(text) if text.contains("RESUMED")));
}

#[test]
fn child_agents_inherit_their_coordinator_provider() {
    let f = Fixture::new();
    install_fake_codex(&f);
    let Message::State(state) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "same-provider".into(),
    }) else {
        panic!()
    };
    let workspace = &state.workspaces[0].id;
    let parent =
        f.coord(json!({"action":"start","workspace":workspace,"provider":"codex"}))["agent"]
            .clone();
    f.wait(|| matches!(f.rpc(Request::Capture {pane:parent["pane"].as_str().unwrap().into()}), Message::Capture(s) if s.contains("CODEX READY")));
    let result = Command::new(env!("CARGO_BIN_EXE_jam"))
        .args([
            "--json",
            "coordinate",
            "start",
            "--workspace",
            workspace,
            "--role",
            "worker",
            "--parent",
            parent["id"].as_str().unwrap(),
            "--tab-name",
            "Implement feature",
        ])
        .env("JAM_SOCKET", &f.socket)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let data: Value = serde_json::from_slice(&result.stdout).unwrap();
    let agent = &data["Coordination"]["agent"];
    assert_eq!(agent["provider"], "codex");
    assert_eq!(
        agent["bypass"], true,
        "Workers inherit unattended execution"
    );
    f.wait(|| matches!(f.rpc(Request::Capture {pane:agent["pane"].as_str().unwrap().into()}), Message::Capture(s) if s.contains("CODEX READY")));
}

#[test]
fn interrupted_work_continues_once_and_idle_restart_stays_quiet() {
    let mut f = Fixture::new();
    let fake = r#"#!/usr/bin/python3
import json, os, socket, subprocess, sys
from pathlib import Path
home=Path(os.environ['HOME'])
agent=os.environ['JAM_AGENT_ID']
session=sys.argv[-1]
log=home/'recovery-launches'
with log.open('a') as out: out.write(session+'\n')
first=len(log.read_text().splitlines())==1
path=home/('peer-'+agent)
if path.exists(): path.unlink()
listener=socket.socket(socket.AF_UNIX); listener.bind(str(path)); listener.listen()
os.environ['CLAUDE_CODE_MESSAGING_SOCKET']=str(path)
os.environ['CLAUDE_CODE_MESSAGING_TOKEN']='fixture'
def hook(event, prompt=''):
    subprocess.run(['jam','skill','context','--agent','claude'],input=json.dumps({'hook_event_name':event,'source':'startup','session_id':session,'prompt':prompt}).encode(),stdout=subprocess.DEVNULL,check=True)
folder=home/'.claude/projects/fixture'; folder.mkdir(parents=True,exist_ok=True)
(folder/(session+'.jsonl')).write_text('{}\n')
hook('SessionStart')
if first: hook('UserPromptSubmit','Do the fixture task')
print('RECOVERY READY',flush=True)
while True:
    c,_=listener.accept()
    with c:
        stream=c.makefile(); json.loads(stream.readline()); data=json.loads(stream.readline())
        prompt=data['message']['content']
        with (home/'recovery-messages').open('a') as out: out.write(prompt+'\n')
        hook('UserPromptSubmit',prompt)
        hook('Stop')
"#;
    let fake_path = f.dir.path().join("claude");
    std::fs::write(&fake_path, fake).unwrap();
    std::fs::set_permissions(&fake_path, std::fs::Permissions::from_mode(0o700)).unwrap();
    let Message::State(state) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "recovery".into(),
    }) else {
        panic!()
    };
    let started =
        f.coord(json!({"action":"start","workspace":state.workspaces[0].id,"provider":"claude"}));
    let agent = started["agent"]["id"].as_str().unwrap().to_owned();
    f.wait(|| f.coord(json!({"action":"list"}))["agents"][0]["activity"] == "working");
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    f.child = Fixture::launch(f.dir.path());
    f.wait(|| connect_to(&f.socket).is_ok());
    f.wait(|| f.coord(json!({"action":"list"}))["agents"][0]["activity"] == "idle");
    let messages = std::fs::read_to_string(f.dir.path().join("recovery-messages")).unwrap();
    assert_eq!(messages.lines().count(), 1);
    assert!(messages.starts_with("Continue the interrupted work"));
    assert!(!messages.contains("You are a Jam"));
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    f.child = Fixture::launch(f.dir.path());
    f.wait(|| connect_to(&f.socket).is_ok());
    f.wait(|| {
        std::fs::read_to_string(f.dir.path().join("recovery-launches"))
            .unwrap()
            .lines()
            .count()
            == 3
    });
    std::thread::sleep(Duration::from_secs(3));
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("recovery-messages")).unwrap(),
        messages
    );
    assert_eq!(f.coord(json!({"action":"list"}))["agents"][0]["id"], agent);
    // A second loss in the same work sequence must not create another turn.
    let board = coordination::Board::at(&f.dir.path().join("state/coordination.sqlite")).unwrap();
    let mut record = board.get(&agent).unwrap();
    record["activity"] = json!("working");
    record["recovery"] = json!({"state":"running","attempted":true});
    board.put("agent", &record).unwrap();
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    f.child = Fixture::launch(f.dir.path());
    f.wait(|| connect_to(&f.socket).is_ok());
    f.wait(|| f.coord(json!({"action":"list"}))["agents"][0]["recovery"]["state"] == "failed");
    assert_eq!(
        std::fs::read_to_string(f.dir.path().join("recovery-messages")).unwrap(),
        messages
    );
    f.coord(json!({"action":"resume","agent":agent}));
    f.wait(|| {
        std::fs::read_to_string(f.dir.path().join("recovery-messages"))
            .unwrap()
            .lines()
            .count()
            == 2
    });
}

#[test]
fn cancel_then_pause_archives_records_and_stops_only_the_worker() {
    let f = Fixture::new();
    let Message::State(state) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "cleanup".into(),
    }) else {
        panic!()
    };
    let c = f.coord(json!({"action":"start","workspace":state.workspaces[0].id,"provider":"claude"}))["agent"].clone();
    let w = f.coord(json!({"action":"start","workspace":state.workspaces[0].id,"provider":"claude","role":"worker","parent":c["id"]}))["agent"].clone();
    f.wait(|| {
        f.coord(json!({"action":"list"}))["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == w["id"] && a["ready"] == true)
    });
    let t = f.coord(json!({"action":"task","owner":c["id"],"objective":"abandoned test"}));
    f.coord(json!({"action":"assign","task":t["id"],"agent":w["id"]}));
    f.coord(json!({"action":"cancel","task":t["id"],"agent":c["id"]}));
    f.coord(json!({"action":"pause","agent":w["id"]}));
    let Message::State(state) = f.rpc(Request::State) else {
        panic!()
    };
    assert!(
        state
            .panes
            .iter()
            .find(|p| p.id == w["pane"])
            .unwrap()
            .exited
    );
    assert!(
        !state
            .panes
            .iter()
            .find(|p| p.id == c["pane"])
            .unwrap()
            .exited
    );
    let active = f.coord(json!({"action":"list"}));
    assert!(active["tasks"].as_array().unwrap().is_empty());
    assert!(
        !active["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == w["id"])
    );
    let output = Command::new(env!("CARGO_BIN_EXE_jam"))
        .args(["--json", "coordinate", "list", "--all"])
        .env("JAM_SOCKET", &f.socket)
        .output()
        .unwrap();
    assert!(output.status.success());
    let history: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        history["Coordination"]["agents"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a["id"] == w["id"] && a["archived"] == true && a["session"].is_string())
    );
    assert!(f.dir.path().exists());
}

#[test]
fn legacy_coordinator_tabs_migrate_without_restarting_conversations_or_losing_tabs() {
    let mut f = Fixture::new();
    let Message::State(initial) = f.rpc(Request::CreateWorkspace {
        path: f.dir.path().to_string_lossy().into(),
        name: "Migration".into(),
    }) else {
        panic!()
    };
    let workspace = initial.workspaces[0].id.clone();
    let tab = initial.workspaces[0].tabs[0].id.clone();
    let shell = initial.panes[0].id.clone();
    let started = f.coord(json!({"action":"start", "workspace":workspace, "provider":"claude"}));
    let pane = started["agent"]["pane"].as_str().unwrap().to_owned();
    let agent = started["agent"]["id"].as_str().unwrap().to_owned();
    f.wait(|| matches!(f.rpc(Request::Capture {pane:pane.clone()}),Message::Capture(text) if text.contains("READY")));
    let session = f.coord(json!({"action":"list"}))["agents"][0]["session"].clone();
    f.rpc(Request::Stop);
    f.child.wait().unwrap();
    let layout_path = f.dir.path().join("state/layout.json");
    let mut layout: State = serde_json::from_slice(&std::fs::read(&layout_path).unwrap()).unwrap();
    // Include an old split layout: migrate the coordinator but keep the shell
    // and its tab identity. Repeating the migration must remain harmless.
    layout.workspaces[0].tabs[0].agent_role = Some("coordinator".into());
    layout.workspaces[0].tabs[0].root = Node::Split {
        vertical: false,
        ratio: 0.5,
        a: Box::new(Node::Leaf(pane.clone())),
        b: Box::new(Node::Leaf(shell.clone())),
    };
    layout.workspaces[0].active_pane = Some(pane.clone());
    std::fs::write(&layout_path, serde_json::to_vec(&layout).unwrap()).unwrap();
    f.child = Fixture::launch(f.dir.path());
    f.wait(|| connect_to(&f.socket).is_ok());
    for _ in 0..2 {
        let Message::State(state) = f.rpc(Request::State) else {
            panic!()
        };
        let w = &state.workspaces[0];
        assert_eq!(w.tabs.len(), 1);
        assert_eq!(w.tabs[0].id, tab);
        assert!(w.tabs[0].agent_role.is_none());
        assert!(matches!(&w.tabs[0].root, Node::Leaf(id) if id == &shell));
        assert_eq!(w.coordinator.as_ref().unwrap()["pane"], pane);
        assert!(
            state
                .panes
                .iter()
                .any(|p| p.id == pane && p.workspace == workspace)
        );
    }
    let current = f.coord(json!({"action":"list"}));
    assert_eq!(current["agents"].as_array().unwrap().len(), 1);
    assert_eq!(current["agents"][0]["session"], session);
    assert_eq!(current["agents"][0]["id"], agent);
    // Closing the last ordinary tab must preserve the coordinator, even when
    // the user's preference normally closes empty workspaces.
    let settings = config::HostSettings {
        close_empty_workspaces: true,
        ..config::Config::default().into()
    };
    f.rpc(Request::Configure { settings });
    let Message::State(state) = f.rpc(Request::CloseTab { tab }) else {
        panic!()
    };
    assert_eq!(state.workspaces.len(), 1);
    assert!(state.workspaces[0].tabs.is_empty());
    assert!(state.workspaces[0].coordinator.is_some());
    f.rpc(Request::Close { pane });
    let Message::State(state) = f.rpc(Request::State) else {
        panic!()
    };
    assert!(state.workspaces.is_empty());
    assert!(state.panes.is_empty());
    assert_eq!(
        f.coord(json!({"action":"list","include_archived":true}))["agents"][0]["closed"],
        true
    );
}

#[test]
fn worktree_removal_accepts_already_paused_agents() {
    let f = Fixture::new();
    let repository = f.dir.path().join("repository");
    std::fs::create_dir(&repository).unwrap();
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
            "Initial",
        ],
    ] {
        let output = Command::new("git")
            .arg("-C")
            .arg(&repository)
            .args(args)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let Message::State(state) = f.rpc(Request::CreateWorkspace {
        path: repository.to_string_lossy().into(),
        name: "Repository".into(),
    }) else {
        panic!()
    };
    let root = state.workspaces[0].id.clone();
    let checkout = f.dir.path().join("checkout");
    let Message::Operation { id, .. } = f.rpc(Request::CreateWorkspaceWithTab {
        location: WorkspaceLocation::Worktree {
            name: "EDGE-7400 Fix link flap".into(),
            workspace: root.clone(),
            branch: "removal-test".into(),
            base: "main".into(),
            path: Some(checkout.to_string_lossy().into()),
        },
        tab: InitialTab::Agent {
            provider: "claude".into(),
            role: "worker".into(),
            parent: None,
            complex: false,
            name: None,
        },
    }) else {
        panic!()
    };
    f.wait(|| {
        matches!(
            f.rpc(Request::Operation { id: id.clone() }),
            Message::CreatedWorkspace { .. }
        )
    });
    let Message::CreatedWorkspace {
        workspace,
        pane,
        agent: Some(agent),
        ..
    } = f.rpc(Request::Operation { id })
    else {
        panic!()
    };
    let Message::State(state) = f.rpc(Request::State) else {
        panic!()
    };
    let child = state
        .workspaces
        .iter()
        .find(|item| item.id == workspace)
        .unwrap();
    assert_eq!(child.name, "EDGE-7400 Fix link flap");
    assert_eq!(child.auto_name, Some(false));
    assert_eq!(
        std::path::Path::new(&child.path),
        checkout.canonicalize().unwrap()
    );
    f.wait(|| matches!(f.rpc(Request::Capture {pane:pane.clone()}),Message::Capture(text) if text.contains("READY")));
    f.coord(json!({"action":"pause","agent":agent}));
    assert!(matches!(f.rpc(Request::State), Message::State(s)
        if s.panes.iter().any(|p| p.id == pane && p.exited)));
    let error = call_at(&f.socket, Request::Capture { pane: pane.clone() }).unwrap_err();
    assert_eq!(error.to_string(), "pane is stopped or missing");
    // Pausing twice is idempotent too; neither path may send on a dead actor.
    f.coord(json!({"action":"pause","agent":agent}));
    let Message::Operation { id, .. } = f.rpc(Request::WorktreeRemove {
        workspace: workspace.clone(),
        force: false,
    }) else {
        panic!()
    };
    let mut result = None;
    f.wait(|| {
        if let Message::Operation {
            done: true, error, ..
        } = f.rpc(Request::Operation { id: id.clone() })
        {
            result = Some(error);
            true
        } else {
            false
        }
    });
    assert_eq!(result, Some(None));
    assert!(!checkout.exists());
    let Message::State(state) = f.rpc(Request::State) else {
        panic!()
    };
    assert_eq!(state.workspaces.len(), 1);
    assert_eq!(state.workspaces[0].id, root);
    assert!(!state.panes.iter().any(|p| p.id == pane));
    let history = f.coord(json!({"action":"list","include_archived":true}));
    assert_eq!(history["agents"][0]["closed"], true);
}
