use std::process::Command;

#[test]
fn cli_bootstrap_runs_without_a_daemon_and_hook_is_quiet_outside_jam() {
    let temp = tempfile::tempdir().unwrap();
    let binary = env!("CARGO_BIN_EXE_jam");
    let command = || {
        let mut command = Command::new(binary);
        command
            .env("HOME", temp.path())
            .env("CLAUDE_CONFIG_DIR", temp.path().join("claude"))
            .env("CODEX_HOME", temp.path().join("codex"))
            .env("JAM_SOCKET", temp.path().join("absent.sock"))
            .env_remove("JAM_PANE_ID");
        command
    };
    let result = command()
        .args([
            "skill", "setup", "--agent", "claude", "--agent", "codex", "--json",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let result = command()
        .args(["skill", "status", "--json"])
        .output()
        .unwrap();
    let reports: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    for report in reports.as_array().unwrap() {
        assert_eq!(report["skill"], "current");
        assert_eq!(report["hook"], "current");
    }
    let settings: serde_json::Value =
        serde_json::from_slice(&std::fs::read(temp.path().join("codex/hooks.json")).unwrap())
            .unwrap();
    let hook = settings["hooks"]["SessionStart"][0]["hooks"][0]["command"]
        .as_str()
        .unwrap();
    let shell = || {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", hook])
            .env("PATH", std::path::Path::new(binary).parent().unwrap())
            .env_remove("JAM_PANE_ID")
            .env_remove("JAM_SOCKET");
        command
    };
    let quiet = shell().output().unwrap();
    assert!(quiet.status.success());
    assert!(quiet.stdout.is_empty() && quiet.stderr.is_empty());
    let inside = shell()
        .env("JAM_PANE_ID", "ae394218-602b-4ecf-adb8-cfb1386baaed")
        .env("JAM_SOCKET", temp.path().join("absent.sock"))
        .output()
        .unwrap();
    assert!(
        inside.status.success(),
        "{}",
        String::from_utf8_lossy(&inside.stderr)
    );
    assert!(String::from_utf8_lossy(&inside.stdout).contains("jam skill show"));
    let mut stop = command()
        .args(["skill", "context", "--agent", "codex"])
        .env("JAM_PANE_ID", "ae394218-602b-4ecf-adb8-cfb1386baaed")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    use std::io::Write;
    stop.stdin
        .take()
        .unwrap()
        .write_all(br#"{"hook_event_name":"Stop","session_id":"fixture"}"#)
        .unwrap();
    let output = stop.wait_with_output().unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["continue"], true);
    let removed = command()
        .args(["skill", "remove", "--json"])
        .output()
        .unwrap();
    assert!(removed.status.success());
    assert!(!temp.path().join("claude/skills/jam/SKILL.md").exists());
    assert!(!temp.path().join(".agents/skills/jam/SKILL.md").exists());
}
