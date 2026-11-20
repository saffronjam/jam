//! Codex session setup without creating an artificial user turn.
use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, Mutex},
    time::Duration,
};

struct Rpc {
    child: Child,
    input: Option<ChildStdin>,
    messages: crossbeam_channel::Receiver<Result<Value, String>>,
    next: u64,
    stderr: Arc<Mutex<Vec<u8>>>,
    stderr_done: crossbeam_channel::Receiver<()>,
}
impl Rpc {
    fn start(agent: &Value) -> Result<Self> {
        let executable = crate::coordination::provider_executable("codex")?;
        let mut command = Command::new(executable);
        command
            .args(["app-server", "--listen", "stdio://"])
            .current_dir(agent["path"].as_str().context("Missing agent directory")?)
            .env_remove("CODEX_THREAD_ID")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(home) = agent["config_directory"].as_str() {
            command.env("CODEX_HOME", home);
        }
        let mut child = command
            .spawn()
            .context("Could not start Codex session service")?;
        let mut errors = child.stderr.take().unwrap();
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let buffer = stderr.clone();
        let (finished, stderr_done) = crossbeam_channel::bounded(1);
        std::thread::spawn(move || {
            let mut bytes = [0; 4096];
            while let Ok(n) = errors.read(&mut bytes) {
                if n == 0 {
                    break;
                }
                let mut captured = buffer.lock().unwrap();
                let remaining = (128 * 1024 + 1_usize).saturating_sub(captured.len());
                captured.extend_from_slice(&bytes[..n.min(remaining)]);
            }
            let _ = finished.send(());
        });
        let input = child.stdin.take();
        let output = child.stdout.take().unwrap();
        let (tx, messages) = crossbeam_channel::bounded(64);
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let value = line.map_err(|e| e.to_string()).and_then(|line| {
                    if line.len() > 2 * 1024 * 1024 {
                        return Err("Codex response is too large".into());
                    }
                    serde_json::from_str(&line).map_err(|e| e.to_string())
                });
                if tx.send(value).is_err() {
                    break;
                }
            }
        });
        let mut rpc = Self {
            child,
            input,
            messages,
            next: 0,
            stderr,
            stderr_done,
        };
        rpc.call("initialize", json!({"clientInfo":{"name":"jam","version":env!("CARGO_PKG_VERSION")},"capabilities":{"experimentalApi":true}}))?;
        rpc.write(&json!({"method":"initialized"}))?;
        Ok(rpc)
    }
    fn write(&mut self, value: &Value) -> Result<()> {
        let input = self
            .input
            .as_mut()
            .context("Codex session service closed")?;
        serde_json::to_writer(&mut *input, value)?;
        input.write_all(b"\n")?;
        input.flush()?;
        Ok(())
    }
    fn diagnostic(&self) -> String {
        // On early exit, let the pipe reader finish before presenting the error.
        let _ = self.stderr_done.recv_timeout(Duration::from_millis(100));
        let bytes = self.stderr.lock().unwrap();
        if bytes.is_empty() {
            return String::new();
        }
        let limit = 128 * 1024;
        format!(
            "\n{}{}",
            String::from_utf8_lossy(&bytes[..bytes.len().min(limit)]),
            if bytes.len() > limit {
                "\n[Further diagnostic output omitted]"
            } else {
                ""
            }
        )
    }
    fn call(&mut self, method: &str, params: Value) -> Result<Value> {
        self.next += 1;
        let id = self.next;
        if let Err(error) = self.write(&json!({"id":id,"method":method,"params":params})) {
            bail!(
                "Could not send {method} to Codex: {error}{}",
                self.diagnostic()
            );
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        loop {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let value = match self.messages.recv_timeout(left) {
                Ok(Ok(value)) => value,
                Ok(Err(error)) => bail!(
                    "Codex returned an invalid {method} response: {error}{}",
                    self.diagnostic()
                ),
                Err(error) => bail!(
                    "Codex did not finish {method}: {error}{}",
                    self.diagnostic()
                ),
            };
            if value["id"] == id {
                if let Some(error) = value.get("error") {
                    bail!(
                        "Codex {method}: {}",
                        error["message"].as_str().unwrap_or("request failed")
                    );
                }
                return value.get("result").cloned().context("Missing Codex result");
            }
            if value.get("id").is_some() && value.get("method").is_some() {
                bail!(
                    "Codex setup requires an interactive action: {}",
                    value["method"]
                );
            }
        }
    }
}
impl Drop for Rpc {
    fn drop(&mut self) {
        self.input.take();
        for _ in 0..20 {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub(crate) fn catalog(home: &str) -> Result<crate::model_catalog::Catalog> {
    use crate::model_catalog::{Catalog, Model};
    let mut rpc = Rpc::start(&json!({"path":home}))?;
    let config = rpc.call("config/read", json!({"includeLayers":false}))?;
    let configured = config["config"]["model"].as_str().map(str::to_owned);
    let mut models = vec![];
    let mut cursor = Value::Null;
    for _ in 0..20 {
        let response = rpc.call(
            "model/list",
            json!({"limit":100,"includeHidden":false,"cursor":cursor}),
        )?;
        for entry in response["data"]
            .as_array()
            .context("Codex did not return a model catalog")?
        {
            if entry["hidden"] == true {
                continue;
            }
            let id = entry["model"]
                .as_str()
                .or_else(|| entry["id"].as_str())
                .filter(|s| !s.is_empty())
                .context("Codex returned a model without an ID")?;
            if models.iter().any(|m: &Model| m.id == id) {
                continue;
            }
            models.push(Model {
                id: id.into(),
                name: entry["displayName"].as_str().unwrap_or(id).into(),
                is_default: entry["isDefault"] == true,
                efforts: entry["supportedReasoningEfforts"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|v| v["reasoningEffort"].as_str().map(str::to_owned))
                    .collect(),
            });
        }
        cursor = response["nextCursor"].clone();
        if cursor.is_null() {
            break;
        }
    }
    if models.is_empty() {
        bail!("Codex returned an empty model catalog")
    }
    let default_model =
        configured.or_else(|| models.iter().find(|m| m.is_default).map(|m| m.id.clone()));
    Ok(Catalog {
        provider: "codex".into(),
        models,
        default_model,
    })
}

pub(crate) fn prepare(agent: &Value, guidance: &str) -> Result<Value> {
    let mut rpc = Rpc::start(agent)?;
    let config = rpc.call(
        "config/read",
        json!({"includeLayers":false,"cwd":agent["path"]}),
    )?;
    let base = config["config"]["developer_instructions"]
        .as_str()
        .unwrap_or("");
    let mut params = json!({"cwd":agent["path"], "developerInstructions":format!("{base}\n\n{guidance}"),"ephemeral":false});
    if agent["model"].is_string() {
        params["model"] = agent["model"].clone();
    }
    if let Some(flags) = agent["launch_flags"].as_array() {
        for pair in flags.windows(2) {
            match pair[0].as_str().unwrap_or("") {
                "--sandbox" | "-s" => params["sandbox"] = pair[1].clone(),
                "--ask-for-approval" | "-a" => params["approvalPolicy"] = pair[1].clone(),
                _ => {}
            }
        }
    }
    if agent["bypass"] == true {
        params["approvalPolicy"] = json!("never");
        params["sandbox"] = json!("danger-full-access");
    }
    let existing = agent["session"].as_str();
    let mut result = if let Some(session) = existing {
        params["threadId"] = json!(session);
        params.as_object_mut().unwrap().remove("ephemeral");
        rpc.call("thread/resume", params)
            .context("Could not resume the saved Codex conversation")?
    } else {
        let result = rpc
            .call("thread/start", params)
            .context("Could not create a Codex conversation")?;
        let session = result["thread"]["id"]
            .as_str()
            .context("Codex did not return a session ID")?;
        // Naming commits an otherwise empty thread to disk. No user message/model call is made.
        rpc.call("thread/name/set", json!({"threadId":session,"name":format!("Jam {}",agent["role"].as_str().unwrap_or("agent"))}))?;
        result
    };
    let session = result["thread"]["id"]
        .as_str()
        .context("Codex did not return a session ID")?;
    if existing.is_some_and(|id| id != session) {
        bail!("Codex resumed a different conversation");
    }
    rpc.call(
        "thread/read",
        json!({"threadId":session,"includeTurns":false}),
    )?;
    // A blank thread does not persist developer context until its first turn.
    // The TUI must receive it as a config override as well; never as a user prompt.
    result["jam_instructions"] = json!(format!("{base}\n\n{guidance}"));
    Ok(result)
}
pub(crate) fn sessions(agent: &Value) -> Result<Value> {
    let mut rpc = Rpc::start(agent)?;
    let result = rpc.call(
        "thread/list",
        json!({"limit":100,"cwd":agent["path"],"archived":false}),
    )?;
    Ok(
        json!(result["data"].as_array().into_iter().flatten().map(|t| json!({
        "session":t["id"],"name":t["name"],"preview":t["preview"],"updated":t["updatedAt"],"cwd":t["cwd"]
    })).collect::<Vec<_>>()),
    )
}
