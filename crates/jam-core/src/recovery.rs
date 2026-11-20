//! Durable managed-agent lifecycle. Delivery readiness is deliberately independent.
use crate::coordination::Board;
use anyhow::Result;
use serde_json::{Value, json};
use std::time::{SystemTime, UNIX_EPOCH};

static STOPPING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
pub fn stopping() {
    STOPPING.store(true, std::sync::atomic::Ordering::Release);
}

const CONTINUE: &str = "Continue the interrupted work from this conversation. Check current files, task state, and results before proceeding; do not repeat actions whose outcome is uncertain.";
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn eligible(a: &Value) -> bool {
    a["paused"] != true
        && a["closed"] != true
        && a["verified_launch"] == true
        && crate::config::agent_enabled(a["provider"].as_str().unwrap_or(""))
}
pub fn lease(agent: &str) -> Result<std::fs::File> {
    use fs2::FileExt;
    use std::os::unix::fs::OpenOptionsExt;
    let name = uuid::Uuid::parse_str(agent)?;
    let file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(crate::state_dir().join(format!("agent-{name}.lock")))?;
    file.try_lock_exclusive().map_err(|_| {
        anyhow::anyhow!("The previous agent process is still running. Stop it before resuming.")
    })?;
    use std::os::fd::AsRawFd;
    // Keep ownership across exec into the provider, but not across unrelated spawns.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_SETFD, 0) } == -1 {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(file)
}
pub fn begin_launch(a: &mut Value) -> Result<String> {
    let generation = crate::id();
    a["run_id"] = json!(generation);
    a["ready"] = json!(false);
    reconcile(a);
    if a["activity"] == "working" && eligible(a) {
        if !crate::config::host_settings().resume_interrupted_work
            && a["recovery"]["manual"] != true
        {
            a["cancel_hold"] = json!(true);
            a["activity"] = json!("cancelled");
            a["recovery"] = json!({"state":"suppressed"});
        } else if a["recovery"]["attempted"] == true && a["recovery"]["manual"] != true {
            a["activity"] = json!("failed");
            a["recovery"]["state"] = json!("failed");
            a["recovery"]["error"] =
                json!("Recovery was interrupted. Resume explicitly to try again.");
        } else {
            a["recovery"] =
                json!({"id":crate::id(),"state":"pending","attempted":true,"deadline":now()+60});
        }
    }
    Board::open()?.put("agent", a)?;
    Ok(generation)
}
pub fn manual_action(a: &mut Value, resume: bool) {
    if resume && ["working", "cancelled", "failed"].contains(&a["activity"].as_str().unwrap_or(""))
    {
        a["cancel_hold"] = json!(false);
        a["activity"] = json!("working");
        a["recovery"] = json!({"id":crate::id(),"state":"pending","manual":true,"attempted":true,"deadline":now()+60});
    } else if !resume {
        a["cancel_hold"] = json!(true);
        a["activity"] = json!("cancelled");
        a["recovery"] = json!({"state":"suppressed"});
    }
}
fn apply_hook(a: &mut Value, input: &Value, run: &str) {
    if a["run_id"] != run || input["agent_id"].is_string() {
        return;
    }
    if input["session_id"].is_string()
        && a["session"].is_string()
        && input["session_id"] != a["session"]
    {
        return;
    }
    if let Some(path) = input["transcript_path"].as_str() {
        a["transcript_path"] = json!(path);
    }
    let event = input["hook_event_name"].as_str().unwrap_or("");
    if !["UserPromptSubmit", "SessionStart"].contains(&event)
        && input["turn_id"].is_string()
        && a["turn"].is_string()
        && input["turn_id"] != a["turn"]
    {
        return;
    }
    match event {
        "SessionStart" => {
            if (a["activity"].is_null() || a["activity"] == "unknown")
                && input["source"] != "compact"
            {
                a["activity"] = json!("idle");
            }
        }
        "UserPromptSubmit" => {
            a["cancel_hold"] = json!(false);
            if input["prompt"]
                .as_str()
                .is_some_and(|p| p.contains(CONTINUE))
                && ["dispatching", "accepted"]
                    .contains(&a["recovery"]["state"].as_str().unwrap_or(""))
            {
                a["recovery"]["state"] = json!("running");
                a["recovery"]["manual"] = json!(false);
            } else {
                a["recovery"] = Value::Null;
            }
            a["activity"] = json!("working");
            a["turn"] = input["turn_id"]
                .as_str()
                .map(|s| json!(s))
                .unwrap_or_else(|| json!(crate::id()));
        }
        "PreToolUse" | "PostToolUse" => {
            if a["cancel_hold"] != true {
                a["activity"] = json!(if ["AskUserQuestion", "request_user_input"]
                    .contains(&input["tool_name"].as_str().unwrap_or(""))
                    && event == "PreToolUse"
                {
                    "waiting_for_input"
                } else {
                    "working"
                });
            }
        }
        "PermissionRequest" => a["activity"] = json!("waiting_for_input"),
        "Notification" if input["notification_type"] == "permission_prompt" => {
            a["activity"] = json!("waiting_for_input")
        }
        "Interrupt" => {
            a["cancel_hold"] = json!(true);
            a["cancel_hold"] = json!(true);
            a["activity"] = json!("cancelled");
            a["recovery"] = json!({"state":"suppressed"});
        }
        "Stop" => {
            a["activity"] = json!(if a["cancel_hold"] == true {
                "cancelled"
            } else {
                "idle"
            });
            a["recovery"] = Value::Null;
        }
        "StopFailure" => {
            a["activity"] = json!("failed");
        }
        "SessionEnd"
            if ["prompt_input_exit", "logout", "clear", "resume"]
                .contains(&input["reason"].as_str().unwrap_or("")) =>
        {
            a["cancel_hold"] = json!(true);
            a["activity"] = json!("cancelled");
            a["recovery"] = json!({"state":"suppressed"});
        }
        _ => {}
    }
    a["activity_event"] = json!(event);
}
pub fn record_hook(pane: &str, input: &Value) {
    let Ok(run) = std::env::var("JAM_AGENT_RUN_ID") else {
        return;
    };
    let _ =
        Board::open().and_then(|mut b| b.update_agent_pane(pane, |a| apply_hook(a, input, &run)));
}
/// Persist intent before forwarding a cancellation key, including through input streams.
pub fn cancel_input(pane: &str) -> Result<()> {
    Board::open()?.update_agent_pane(pane, |a| {
        if ["working", "waiting_for_input"].contains(&a["activity"].as_str().unwrap_or("")) {
            a["cancel_hold"] = json!(true);
            a["activity"] = json!("cancelled");
            a["recovery"] = json!({"state":"suppressed"});
        }
    })
}
pub fn should_restart(a: &Value) -> bool {
    eligible(a)
        && a["activity"] == "working"
        && a["recovery"]["state"] != "failed"
        && crate::config::host_settings().resume_interrupted_work
}
/// A persisted dispatch is never blindly resent after an ambiguous transport result.
pub fn deliver() -> Result<()> {
    if STOPPING.load(std::sync::atomic::Ordering::Acquire) {
        return Ok(());
    }
    let mut board = Board::open()?;
    for agent in board.agents()? {
        let id = agent["id"].as_str().unwrap_or("");
        if agent["recovery"]["state"] == "pending"
            && agent["recovery"]["manual"] != true
            && !crate::config::host_settings().resume_interrupted_work
        {
            board.update_agent(id, |a| {
                a["activity"] = json!("cancelled");
                a["recovery"]["state"] = json!("suppressed");
            })?;
            continue;
        }
        if !eligible(&agent) {
            continue;
        }
        if ["pending", "dispatching", "accepted"]
            .contains(&agent["recovery"]["state"].as_str().unwrap_or(""))
            && agent["recovery"]["deadline"].as_u64().unwrap_or(0) <= now()
        {
            board.update_agent(id, |a| {
                a["recovery"]["state"] = json!("failed");
                a["recovery"]["error"] =
                    json!("Could not confirm recovery. Resume explicitly to try again.");
                a["activity"] = json!("failed");
            })?;
            continue;
        }
        if agent["recovery"]["state"] != "pending" || agent["ready"] != true {
            continue;
        }
        let mut claimed = false;
        board.update_agent(id, |a| {
            if a["recovery"] == agent["recovery"]
                && a["run_id"] == agent["run_id"]
                && eligible(a)
                && a["recovery"]["state"] == "pending"
                && a["ready"] == true
            {
                a["recovery"]["state"] = json!("dispatching");
                a["ready"] = json!(false);
                a["ready_version"] =
                    json!(a["ready_version"].as_u64().unwrap_or(0).wrapping_add(1));
                claimed = true;
            }
        })?;
        if !claimed {
            continue;
        }
        let result = crate::coordination::notify_agent(&agent, CONTINUE);
        board.update_agent(id, |a| {
            if a["recovery"]["state"] != "dispatching"
                || a["recovery"]["id"] != agent["recovery"]["id"]
            {
                return;
            }
            match result {
                Ok(()) => a["recovery"]["state"] = json!("accepted"),
                Err(ref e) => {
                    a["recovery"]["state"] = json!("failed");
                    a["recovery"]["error"] =
                        json!(format!("Could not resume interrupted work: {e:#}"));
                    a["activity"] = json!("failed");
                }
            }
        })?;
    }
    Ok(())
}

// Read only the bounded tail of the provider's recorded conversation. This
// catches completion/cancellation written after our last successful hook.
fn reconcile(a: &mut Value) {
    use std::io::{Read, Seek, SeekFrom};
    // Transcript events describe the old turn. Explicit cancellation made
    // afterwards remains authoritative until Resume or a new user prompt.
    if a["cancel_hold"] == true {
        return;
    }
    let Some(path) = a["transcript_path"].as_str() else {
        return;
    };
    let Ok(mut file) = std::fs::File::open(path) else {
        return;
    };
    let Ok(meta) = file.metadata() else {
        return;
    };
    let offset = meta.len().saturating_sub(256 * 1024);
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return;
    }
    let mut tail = String::new();
    if file.take(256 * 1024).read_to_string(&mut tail).is_err() {
        return;
    }
    for line in tail.lines().rev() {
        let Ok(event) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if a["provider"] == "codex" && event["type"] == "event_msg" {
            match event["payload"]["type"].as_str() {
                Some("task_complete" | "task_completed") => {
                    if a["activity"] == "working" {
                        a["activity"] = json!("idle");
                    }
                    return;
                }
                Some("turn_aborted") => {
                    // Jam may itself have stopped this process during restart.
                    if a["activity"] != "working" {
                        a["activity"] = json!("cancelled");
                    }
                    return;
                }
                Some("task_started") => return,
                _ => {}
            }
        }
        if a["provider"] == "claude" && event["type"] == "assistant" {
            if event["message"]["stop_reason"] == "end_turn" && a["activity"] == "working" {
                a["activity"] = json!("idle");
            }
            return;
        }
        if event["type"] == "user" {
            return;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn completed_transcript_cannot_release_explicit_exit() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("conversation.jsonl");
        for completion in ["task_complete", "task_completed"] {
            std::fs::write(
                &path,
                json!({"type":"event_msg","payload":{"type":completion}}).to_string(),
            )
            .unwrap();
            let mut agent = json!({"provider":"codex","run_id":"r","activity":"working","transcript_path":path});
            apply_hook(
                &mut agent,
                &json!({"hook_event_name":"SessionEnd","reason":"prompt_input_exit"}),
                "r",
            );
            reconcile(&mut agent);
            assert_eq!(agent["activity"], "cancelled");
            assert_eq!(agent["cancel_hold"], true);
            assert_eq!(agent["recovery"]["state"], "suppressed");
            manual_action(&mut agent, true);
            assert_eq!(agent["cancel_hold"], false);
            assert_eq!(agent["recovery"]["manual"], true);
        }
        for activity in ["cancelled", "failed", "working"] {
            let mut agent = json!({"provider":"codex","activity":activity,"transcript_path":path});
            reconcile(&mut agent);
            assert_eq!(
                agent["activity"],
                if activity == "working" {
                    "idle"
                } else {
                    activity
                }
            );
        }
    }
    #[test]
    fn lifecycle_preserves_intent_and_rejects_stale_events() {
        let mut a = json!({"run_id":"new","activity":"working","session":"s"});
        apply_hook(
            &mut a,
            &json!({"hook_event_name":"Stop","session_id":"s"}),
            "old",
        );
        assert_eq!(a["activity"], "working");
        apply_hook(
            &mut a,
            &json!({"hook_event_name":"Interrupt","session_id":"s"}),
            "new",
        );
        assert_eq!(a["activity"], "cancelled");
        apply_hook(
            &mut a,
            &json!({"hook_event_name":"PostToolUse","session_id":"s"}),
            "new",
        );
        assert_eq!(a["activity"], "cancelled");
        apply_hook(
            &mut a,
            &json!({"hook_event_name":"UserPromptSubmit","session_id":"s"}),
            "new",
        );
        assert_eq!(a["activity"], "working");
        apply_hook(
            &mut a,
            &json!({"hook_event_name":"PermissionRequest","session_id":"s"}),
            "new",
        );
        assert_eq!(a["activity"], "waiting_for_input");
        apply_hook(
            &mut a,
            &json!({"hook_event_name":"Stop","session_id":"s"}),
            "new",
        );
        assert_eq!(a["activity"], "idle");
    }
    #[test]
    fn recovery_turn_keeps_its_retry_budget() {
        let mut a = json!({"run_id":"r","recovery":{"state":"dispatching","attempted":true}});
        apply_hook(
            &mut a,
            &json!({"hook_event_name":"UserPromptSubmit","prompt":CONTINUE}),
            "r",
        );
        assert_eq!(a["recovery"]["state"], "running");
        assert_eq!(a["recovery"]["attempted"], true);
        manual_action(&mut a, false);
        assert_eq!(a["activity"], "cancelled");
        manual_action(&mut a, true);
        assert_eq!(a["activity"], "working");
        assert_eq!(a["recovery"]["state"], "pending");
    }
}
