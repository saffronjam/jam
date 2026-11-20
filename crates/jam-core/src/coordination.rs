//! Durable host-local coordination. Provider credentials are never stored here.
use crate::*;
use anyhow::{Context, Result, bail};
use rusqlite::{Connection, params};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    io::{IsTerminal, Read, Write},
    sync::{LazyLock, Mutex},
    time::Duration,
};

static BINDINGS: LazyLock<Mutex<HashMap<String, (String, String)>>> =
    LazyLock::new(Default::default);
pub struct AgentSpec<'a> {
    pub workspace: &'a str,
    pub root: &'a str,
    pub pane: &'a str,
    pub provider: &'a str,
    pub role: &'a str,
    pub path: &'a str,
    pub parent: Option<&'a str>,
    pub complex: bool,
}
pub fn is_worker_role(role: &str) -> bool {
    ["worker", "reviewer"].contains(&role)
}
pub struct Board {
    db: Connection,
}
impl Board {
    pub fn open() -> Result<Self> {
        Self::at(&state_dir().join("coordination.sqlite"))
    }
    pub fn at(path: &std::path::Path) -> Result<Self> {
        let db = Connection::open(path)?;
        db.busy_timeout(Duration::from_secs(5))?;
        db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE IF NOT EXISTS records(kind TEXT NOT NULL,id TEXT PRIMARY KEY,value TEXT NOT NULL); CREATE INDEX IF NOT EXISTS kinds ON records(kind);")?;
        Ok(Self { db })
    }
    pub(crate) fn update_agent(&mut self, id: &str, f: impl FnOnce(&mut Value)) -> Result<()> {
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut a = self.get(id)?;
            f(&mut a);
            self.put("agent", &a)
        })();
        self.db
            .execute_batch(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })?;
        result
    }
    pub(crate) fn update_agent_pane(
        &mut self,
        pane: &str,
        f: impl FnOnce(&mut Value),
    ) -> Result<()> {
        if let Some(a) = self.agent_for_pane(pane)? {
            self.update_agent(field(&a, "id")?, f)?;
        }
        Ok(())
    }
    fn all(&self, kind: &str) -> Result<Vec<Value>> {
        let mut q = self
            .db
            .prepare("SELECT value FROM records WHERE kind=? ORDER BY rowid")?;
        let rows = q.query_map([kind], |r| r.get::<_, String>(0))?;
        rows.map(|r| Ok(serde_json::from_str(&r?)?)).collect()
    }
    pub fn get(&self, id: &str) -> Result<Value> {
        Ok(serde_json::from_str(
            &self
                .db
                .query_row("SELECT value FROM records WHERE id=?", [id], |r| {
                    r.get::<_, String>(0)
                })
                .context("Coordination record not found")?,
        )?)
    }
    pub fn put(&self, kind: &str, v: &Value) -> Result<()> {
        self.db.execute("INSERT INTO records(kind,id,value) VALUES(?,?,?) ON CONFLICT(id) DO UPDATE SET value=excluded.value",params![kind,field(v,"id")?,v.to_string()])?;
        Ok(())
    }
    pub fn agents(&self) -> Result<Vec<Value>> {
        self.all("agent")
    }
    pub fn snapshot(&self) -> Result<Value> {
        let agents = self.all("agent")?;
        let messages = self.all("message")?;
        let mut tasks = self.all("task")?;
        for task in &mut tasks {
            annotate_delivery(task, &messages, &agents);
        }
        Ok(
            json!({"agents":agents,"tasks":tasks,"messages":messages,"settings":crate::config::host_settings()}),
        )
    }
    fn visible_snapshot(&self, include_archived: bool) -> Result<Value> {
        let mut value = self.snapshot()?;
        if !include_archived {
            for kind in ["agents", "tasks", "messages"] {
                value[kind]
                    .as_array_mut()
                    .unwrap()
                    .retain(|v| v["archived"] != true);
            }
        }
        Ok(value)
    }
    // Called within the caller's transaction. Keep history, invalidate obsolete
    // notifications, and never infer abandonment merely from a stopped process.
    fn archive_finished(&self) -> Result<()> {
        for task in self.all("task")? {
            if task["archived"] != true
                && ["done", "cancelled"].contains(&task["state"].as_str().unwrap_or(""))
            {
                self.cancel_dependents(&task)?;
            }
        }
        let mut tasks = self.all("task")?;
        let terminal =
            |t: &Value| ["done", "cancelled"].contains(&t["state"].as_str().unwrap_or(""));
        for index in 0..tasks.len() {
            let t = &tasks[index];
            let review_still_needed = t["state"] == "done"
                && t["review_of"].is_string()
                && tasks
                    .iter()
                    .any(|p| p["id"] == t["review_of"] && !terminal(p));
            if terminal(t) && !review_still_needed && t["archived"] != true {
                tasks[index]["archived"] = json!(true);
                self.put("task", &tasks[index])?;
            }
        }
        let mut agents = self.all("agent")?;
        for a in &mut agents {
            let unfinished = tasks
                .iter()
                .any(|t| (t["assignee"] == a["id"] || t["owner"] == a["id"]) && !terminal(t));
            let finished_assignment = tasks
                .iter()
                .any(|t| t["assignee"] == a["id"] && terminal(t));
            if a["closed"] == true
                || (is_worker_role(a["role"].as_str().unwrap_or(""))
                    && a["paused"] == true
                    && finished_assignment
                    && !unfinished)
            {
                a["archived"] = json!(true);
                self.put("agent", a)?;
            }
        }
        for mut m in self.all("message")? {
            if m["archived"] == true {
                continue;
            }
            let task = tasks.iter().find(|t| t["id"] == m["task"]);
            let obsolete_task = task.is_some_and(|t| {
                t["archived"] == true
                    || (terminal(t) && !(m["from"] == t["assignee"] && m["to"] == t["owner"]))
            });
            let obsolete_recipient = agents
                .iter()
                .any(|a| a["id"] == m["to"] && a["archived"] == true);
            // An unfinished task's result still belongs to its next coordinator.
            let retain_for_successor = (task.is_some_and(|t| !terminal(t))
                || tasks.iter().any(|t| t["owner"] == m["to"] && !terminal(t)))
                && agents
                    .iter()
                    .any(|a| a["id"] == m["to"] && a["role"] == "coordinator");
            if m["state"] == "acknowledged"
                || obsolete_task
                || (obsolete_recipient && !retain_for_successor)
            {
                if m["state"] != "acknowledged" {
                    m["state"] = json!("cancelled");
                }
                m["archived"] = json!(true);
                m["error"] = Value::Null;
                self.put("message", &m)?;
            }
        }
        self.promote_queued()
    }
    // Called inside the same transaction that releases or restores an agent's
    // assignment. Cancel descendants and archive obsolete messages first, so a
    // cancelled review cannot be promoted ahead of unrelated queued work.
    fn promote_queued(&self) -> Result<()> {
        let mut tasks = self.all("task")?;
        for agent in self.all("agent")? {
            if agent["paused"] == true
                || agent["closed"] == true
                || agent["archived"] == true
                || !crate::config::agent_enabled(field(&agent, "provider")?)
                || tasks.iter().any(|task| {
                    task["assignee"] == agent["id"]
                        && ["assigned", "working"].contains(&task["state"].as_str().unwrap_or(""))
                })
            {
                continue;
            }
            if is_worker_role(agent["role"].as_str().unwrap_or(""))
                && tasks
                    .iter()
                    .filter(|task| {
                        task["root"] == agent["root"]
                            && ["assigned", "working"]
                                .contains(&task["state"].as_str().unwrap_or(""))
                    })
                    .count()
                    >= crate::config::host_settings().max_agents_per_coordinator
            {
                continue;
            }
            let Some(next) = tasks.iter_mut().find(|task| {
                task["assignee"] == agent["id"]
                    && task["state"] == "queued"
                    && task["archived"] != true
            }) else {
                continue;
            };
            let owner = self.get(field(next, "owner")?)?;
            if owner["closed"] == true || !crate::config::agent_enabled(field(&owner, "provider")?)
            {
                continue;
            }
            next["state"] = json!("assigned");
            self.put("task", next)?;
            self.send(
                field(next, "owner")?,
                field(next, "assignee")?,
                &format!(
                    "Task {} is ready: {}",
                    field(next, "id")?,
                    field(next, "objective")?
                ),
                Some(field(next, "id")?),
            )?;
        }
        Ok(())
    }
    fn cancel_dependents(&self, task: &Value) -> Result<()> {
        let mut cancelled = vec![task["id"].clone()];
        loop {
            let mut added = false;
            for mut child in self.all("task")? {
                if child["root"] != task["root"] || cancelled.contains(&child["id"]) {
                    continue;
                }
                if cancelled.contains(&child["review_of"]) || cancelled.contains(&child["parent"]) {
                    if !["done", "cancelled"].contains(&child["state"].as_str().unwrap_or("")) {
                        child["state"] = json!("cancelled");
                        child["cancelled_with"] = task["id"].clone();
                        self.put("task", &child)?;
                    }
                    cancelled.push(child["id"].clone());
                    added = true;
                }
            }
            if !added {
                break;
            }
        }
        Ok(())
    }
    fn task_with_delivery(&self, mut task: Value) -> Result<Value> {
        annotate_delivery(&mut task, &self.all("message")?, &self.all("agent")?);
        Ok(task)
    }
    pub fn agent_for_pane(&self, pane: &str) -> Result<Option<Value>> {
        Ok(self.all("agent")?.into_iter().find(|a| a["pane"] == pane))
    }
    pub fn add(&mut self, spec: AgentSpec<'_>) -> Result<Value> {
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = self.add_inner(spec);
        self.db
            .execute_batch(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })?;
        result
    }
    fn add_inner(&mut self, spec: AgentSpec<'_>) -> Result<Value> {
        let AgentSpec {
            workspace,
            root,
            pane,
            provider,
            role,
            path,
            parent,
            complex,
        } = spec;
        if !["claude", "codex"].contains(&provider)
            || !["coordinator", "worker", "reviewer"].contains(&role)
        {
            bail!("Invalid agent provider or role")
        }
        if complex && role != "worker" {
            bail!("Complex work is only available for workers")
        }
        if !crate::config::agent_enabled(provider) {
            bail!("{provider} integration is disabled in Settings")
        }
        if role == "coordinator"
            && let Some(a) = self
                .all("agent")?
                .into_iter()
                .find(|a| a["workspace"] == workspace && a["role"] == role && a["closed"] != true)
        {
            bail!(
                "This workspace already has a coordinator ({}); open it instead",
                a["id"]
            );
        }
        if is_worker_role(role)
            && self
                .all("agent")?
                .iter()
                .filter(|a| {
                    a["root"] == root
                        && is_worker_role(a["role"].as_str().unwrap_or(""))
                        && a["paused"] != true
                })
                .count()
                >= crate::config::host_settings().max_agents_per_coordinator
        {
            bail!(
                "Worker limit ({}) reached; pause an idle worker first",
                crate::config::host_settings().max_agents_per_coordinator
            )
        }
        let agent = json!({"id":id(),"workspace":workspace,"root":root,"pane":pane,"provider":provider,"role":role,"path":path,"parent":parent,"session":null,"paused":false,"verified_launch":true,"bypass":role == "coordinator","started":false});
        let mut agent = agent;
        if let Some(parent) = parent {
            let owner = self.get(parent)?;
            if owner["root"] != root || owner["role"] != "coordinator" || owner["closed"] == true {
                bail!("Parent must be this workspace's coordinator")
            };
            for key in [
                "bypass",
                "permission_mode",
                "config_directory",
                "launch_flags",
            ] {
                if !owner[key].is_null() && (key == "bypass" || owner["provider"] == provider) {
                    agent[key] = owner[key].clone();
                }
            }
        }
        if role == "coordinator" || parent.is_some() {
            agent["bypass"] = json!(true);
        }
        agent["complex"] = json!(complex);
        agent["model"] = crate::config::host_settings()
            .agent_models
            .get(provider)
            .and_then(|models| models.select(role, complex))
            .map(|model| json!(model))
            .unwrap_or(Value::Null);
        self.put("agent", &agent)?;
        if role == "coordinator" {
            self.adopt_closed_team(&agent)?;
        }
        Ok(agent)
    }
    fn adopt_closed_team(&self, successor: &Value) -> Result<()> {
        let previous: Vec<Value> = self
            .all("agent")?
            .into_iter()
            .filter(|a| {
                a["workspace"] == successor["workspace"]
                    && a["role"] == "coordinator"
                    && a["closed"] == true
            })
            .collect();
        let replaced = |id: &Value| previous.iter().any(|a| a["id"] == *id);
        for mut child in self.all("agent")? {
            if replaced(&child["parent"]) && child["archived"] != true {
                child["parent"] = successor["id"].clone();
                self.put("agent", &child)?;
            }
        }
        for mut task in self.all("task")? {
            if ["done", "cancelled"].contains(&task["state"].as_str().unwrap_or("")) {
                continue;
            }
            let mut changed = false;
            for field in ["owner", "assignee"] {
                if replaced(&task[field]) {
                    task[field] = successor["id"].clone();
                    changed = true;
                }
            }
            if changed {
                self.put("task", &task)?;
            }
        }
        for mut message in self.all("message")? {
            if replaced(&message["to"])
                && message["state"] != "acknowledged"
                && message["state"] != "cancelled"
                && message["archived"] != true
            {
                message["to"] = successor["id"].clone();
                message["state"] = json!("queued");
                message["error"] = Value::Null;
                self.put("message", &message)?;
            }
        }
        Ok(())
    }
    pub fn close_pane(&self, pane: &str) -> Result<()> {
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            if let Some(mut a) = self.agent_for_pane(pane)? {
                a["paused"] = json!(true);
                a["closed"] = json!(true);
                self.put("agent", &a)?;
            }
            self.archive_finished()
        })();
        self.db
            .execute_batch(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })?;
        result
    }
    pub fn action(&mut self, v: Value) -> Result<Value> {
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let operation =
            if ["list", "instructions", "inbox"].contains(&v["action"].as_str().unwrap_or("")) {
                None
            } else {
                v["operation"].as_str().map(|s| format!("operation:{s}"))
            };
        let result = (|| {
            if let Some(key) = &operation
                && let Ok(receipt) = self.get(key)
            {
                return Ok(receipt["result"].clone());
            }
            let result = self.action_inner(v)?;
            if let Some(key) = operation {
                self.put("receipt", &json!({"id":key,"result":result}))?;
            }
            Ok(result)
        })();
        if result.is_ok() {
            self.db.execute_batch("COMMIT")?;
        } else {
            self.db.execute_batch("ROLLBACK")?;
        }
        result
    }
    fn action_inner(&mut self, v: Value) -> Result<Value> {
        let action = field(&v, "action")?;
        if action != "list" && action != "pause" {
            let agent = if action == "register" {
                self.agent_for_pane(field(&v, "pane")?)?
            } else if let Some(id) = v["agent"].as_str().or(v["owner"].as_str()) {
                Some(self.get(id)?)
            } else {
                None
            };
            if let Some(a) = agent
                && !crate::config::agent_enabled(field(&a, "provider")?)
            {
                bail!("Agent integration is disabled in Settings")
            }
        }
        if !["list", "instructions", "inbox", "ack", "launch_failed"].contains(&action)
            && let Some(id) = v["agent"].as_str().or(v["owner"].as_str())
            && self.get(id)?["closed"] == true
        {
            bail!("This coordinator is closed. Add a new coordinator to continue.");
        }
        match action {
            "list" => self.visible_snapshot(v["include_archived"] == true),
            "register" => {
                let pane = field(&v, "pane")?;
                let mut a = self
                    .agent_for_pane(pane)?
                    .context("Start this session with Jam's agent launcher before registering")?;
                if v["run_id"].is_string() && v["run_id"] != a["run_id"] {
                    return Ok(a);
                }
                if v["session"].as_str().is_none() {
                    bail!("Provider did not supply a session ID")
                }
                a["session"] = v["session"].clone();
                if v["ready"].is_boolean() {
                    a["ready"] = v["ready"].clone();
                    a["ready_version"] =
                        json!(a["ready_version"].as_u64().unwrap_or(0).wrapping_add(1));
                }
                if let Some(flags) = v["launch_flags"].as_array() {
                    a["launch_flags"] = json!(flags);
                    if flags.iter().any(|f| {
                        [
                            "--yolo",
                            "--dangerously-bypass-approvals-and-sandbox",
                            "--dangerously-skip-permissions",
                        ]
                        .contains(&f.as_str().unwrap_or(""))
                    }) {
                        a["bypass"] = json!(true);
                        a["verified_launch"] = json!(true);
                    }
                }
                if a["provider"] == "claude" && v["permission_mode"].is_string() {
                    a["verified_launch"] = json!(true);
                }

                if let Some(dir) = v["config_directory"].as_str() {
                    a["config_directory"] = json!(dir);
                }
                if let Some(model) = v["model"].as_str() {
                    a["model"] = json!(model);
                }
                if v["bypass"].is_boolean() {
                    a["bypass"] = v["bypass"].clone();
                }
                if let Some(mode) = v["permission_mode"].as_str() {
                    a["permission_mode"] = json!(mode);
                    a["bypass"] = json!(mode == "bypassPermissions");
                }
                if let Some(socket) = v["socket"].as_str() {
                    BINDINGS.lock().unwrap().insert(
                        field(&a, "id")?.into(),
                        (socket.into(), v["token"].as_str().unwrap_or("").into()),
                    );
                }
                self.put("agent", &a)?;
                if v["event"] == "SessionStart" && a["ready"] == true {
                    for mut message in self.all("message")? {
                        if message["to"] == a["id"]
                            && ["sending", "transport_accepted", "delivery_failed"]
                                .contains(&message["state"].as_str().unwrap_or(""))
                        {
                            message["state"] = json!("queued");
                            message["attempts"] = json!(0);
                            message["error"] = Value::Null;
                            self.put("message", &message)?;
                        }
                    }
                }
                Ok(a)
            }
            "launch_failed" => {
                let mut a = self.get(field(&v, "agent")?)?;
                a["launch_error"] = v["error"].clone();
                self.put("agent", &a)?;
                Ok(a)
            }
            "recover" => {
                let mut a = self.get(field(&v, "agent")?)?;
                if !v["session"].is_null() {
                    let session = v["session"].as_str().context("Invalid session ID")?;
                    uuid::Uuid::parse_str(session).context("Invalid session ID")?;
                }
                let previous = a["session"].clone();
                if previous.is_string() {
                    if !a["previous_sessions"].is_array() {
                        a["previous_sessions"] = json!([]);
                    }
                    a["previous_sessions"]
                        .as_array_mut()
                        .unwrap()
                        .push(previous);
                }
                a["session"] = v["session"].clone();
                a["started"] = json!(a["session"].is_string());
                a["paused"] = json!(false);
                a["launch_error"] = Value::Null;
                self.put("agent", &a)?;
                Ok(a)
            }
            "pause" | "resume" => {
                let mut a = self.get(field(&v, "agent")?)?;
                if v["action"] == "resume" && a["verified_launch"] != true {
                    bail!("Launch permissions are unknown; resume this agent manually")
                };
                if v["action"] == "resume"
                    && a["paused"] == true
                    && is_worker_role(a["role"].as_str().unwrap_or(""))
                    && self
                        .all("agent")?
                        .iter()
                        .filter(|other| {
                            other["root"] == a["root"]
                                && is_worker_role(other["role"].as_str().unwrap_or(""))
                                && other["paused"] != true
                        })
                        .count()
                        >= crate::config::host_settings().max_agents_per_coordinator
                {
                    bail!("Worker limit reached; pause another worker before resuming this one")
                }
                if v["action"] == "resume" && a["archived"] == true {
                    // Reopening a retired conversation is not permission to replay
                    // the task that has already been completed or cancelled.
                    a["activity"] = json!("idle");
                    a["recovery"] = Value::Null;
                    a["cancel_hold"] = json!(false);
                }
                a["paused"] = json!(v["action"] == "pause");
                crate::recovery::manual_action(&mut a, v["action"] == "resume");
                if v["action"] == "resume" {
                    a["archived"] = json!(false);
                }
                self.put("agent", &a)?;
                self.archive_finished()?;
                self.get(field(&a, "id")?)
            }
            "task" => {
                if v["review_of"].is_string() {
                    let parent = self.get(field(&v, "review_of")?)?;
                    if parent["owner"] != v["owner"]
                        || parent["archived"] == true
                        || parent["state"] == "cancelled"
                    {
                        bail!("Review must belong to an active task owned by this coordinator");
                    }
                }
                let owner = self.get(field(&v, "owner")?)?;
                if owner["role"] != "coordinator" {
                    bail!("Task owner must be a coordinator")
                }
                let task = json!({"id":id(),"owner":owner["id"],"root":owner["root"],"objective":field(&v,"objective")?,"state":"pending","assignee":null,"parent":v["parent"],"review_of":v["review_of"],"result":null});
                self.put("task", &task)?;
                Ok(task)
            }
            "assign" => {
                let mut t = self.get(field(&v, "task")?)?;
                if !["pending", "blocked"].contains(&field(&t, "state")?) {
                    bail!("Task is already assigned or completed")
                }
                let a = self.get(field(&v, "agent")?)?;
                if a["root"] != t["root"] || a["paused"] == true {
                    bail!("Agent is outside this workspace or paused")
                }
                if is_worker_role(a["role"].as_str().unwrap_or(""))
                    && self
                        .all("task")?
                        .iter()
                        .filter(|x| {
                            x["root"] == t["root"]
                                && ["assigned", "working"]
                                    .contains(&x["state"].as_str().unwrap_or(""))
                        })
                        .count()
                        >= crate::config::host_settings().max_agents_per_coordinator
                {
                    bail!(
                        "Active worker limit ({}) reached",
                        crate::config::host_settings().max_agents_per_coordinator
                    )
                }
                let mut recipient = a.clone();
                let owner = self.get(field(&t, "owner")?)?;
                if owner["workspace"] != a["workspace"]
                    && a["role"] != "coordinator"
                    && let Some(coordinator) = self.all("agent")?.into_iter().find(|c| {
                        c["workspace"] == a["workspace"]
                            && c["role"] == "coordinator"
                            && c["closed"] != true
                    })
                {
                    recipient = coordinator;
                }
                let a = recipient;
                if self.all("task")?.iter().any(|x| {
                    x["assignee"] == a["id"]
                        && ["assigned", "working"].contains(&x["state"].as_str().unwrap_or(""))
                }) {
                    t["assignee"] = a["id"].clone();
                    t["state"] = json!("queued");
                    self.put("task", &t)?;
                    return self.task_with_delivery(t);
                }
                t["assignee"] = a["id"].clone();
                t["state"] = json!("assigned");
                self.put("task", &t)?;
                self.send(field(&t,"owner")?,field(&a,"id")?,&format!("Task {}: {}. Read the task, perform the work, and report evidence with jam coordinate result.",field(&t,"id")?,field(&t,"objective")?),Some(field(&t,"id")?))?;
                self.task_with_delivery(t)
            }
            "result" => {
                let mut t = self.get(field(&v, "task")?)?;
                if t["assignee"] != v["agent"] {
                    bail!("Only the assigned agent can report this result")
                }
                if t["state"] == "cancelled" {
                    bail!("Task was cancelled")
                }
                if t["result"].is_string() {
                    return Ok(t);
                }
                t["state"] = json!(if t["review_of"].is_string() {
                    "done"
                } else {
                    "review"
                });
                t["result"] = json!(field(&v, "text")?);
                self.put("task", &t)?;
                self.send(
                    field(&v, "agent")?,
                    field(&t, "owner")?,
                    &format!(
                        "Task {} is ready for review: {}",
                        field(&t, "id")?,
                        field(&v, "text")?
                    ),
                    Some(field(&t, "id")?),
                )?;
                self.archive_finished()?;
                self.get(field(&t, "id")?)
            }
            "complete" | "cancel" => {
                let mut t = self.get(field(&v, "task")?)?;
                if t["owner"] != v["agent"] {
                    bail!("Only the responsible coordinator can close this task")
                };
                if t["state"] == "done" || (v["action"] == "cancel" && t["state"] == "cancelled") {
                    self.archive_finished()?;
                    return self.get(field(&t, "id")?);
                }
                if v["action"] == "complete"
                    && (t["state"] != "review"
                        || v["text"].as_str().unwrap_or("").trim().is_empty())
                {
                    bail!("Completion requires a reviewed result and review evidence")
                };
                t["state"] = json!(if v["action"] == "cancel" {
                    "cancelled"
                } else {
                    "done"
                });
                if v["action"] == "complete"
                    && !self.all("task")?.iter().any(|review| {
                        review["review_of"] == t["id"]
                            && review["state"] == "done"
                            && review["assignee"] != t["assignee"]
                            && review["assignee"].is_string()
                    })
                {
                    bail!("An independent review task must finish before completion")
                }
                if v["action"] == "complete" {
                    t["review"] = v["text"].clone();
                }
                self.put("task", &t)?;
                if v["action"] == "cancel" {
                    self.cancel_dependents(&t)?;
                }
                self.archive_finished()?;
                self.get(field(&t, "id")?)
            }
            "send" => self.send(
                field(&v, "agent")?,
                field(&v, "to")?,
                field(&v, "text")?,
                v["task"].as_str(),
            ),
            "inbox" => Ok(json!(
                self.all("message")?
                    .into_iter()
                    .filter(|m| m["to"] == v["agent"]
                        && m["state"] != "acknowledged"
                        && m["state"] != "cancelled"
                        && m["archived"] != true)
                    .collect::<Vec<_>>()
            )),
            "ack" => {
                let mut m = self.get(field(&v, "message")?)?;
                if m["to"] != v["agent"] {
                    bail!("Message belongs to another agent")
                };
                if m["state"] == "cancelled" || m["archived"] == true {
                    return Ok(m);
                }
                m["state"] = json!("acknowledged");
                m["archived"] = json!(true);
                self.put("message", &m)?;
                Ok(m)
            }
            "instructions" => {
                let a = self.get(field(&v, "agent")?)?;
                instructions(&a)
            }
            _ => bail!("Unknown coordination action"),
        }
    }
    fn send(&self, from: &str, to: &str, text: &str, task: Option<&str>) -> Result<Value> {
        if text.len() > 65536 {
            bail!("Message exceeds 64 KiB")
        }
        let sender = self.get(from)?;
        let mut recipient = self.get(to)?;
        if recipient["closed"] == true
            && let Some(active) = self.all("agent")?.into_iter().find(|a| {
                a["workspace"] == recipient["workspace"]
                    && a["role"] == "coordinator"
                    && a["closed"] != true
            })
        {
            recipient = active;
        }
        let to = field(&recipient, "id")?;
        if recipient["archived"] == true
            && !(recipient["role"] == "coordinator"
                && self.all("task")?.iter().any(|t| {
                    t["owner"] == recipient["id"]
                        && !["done", "cancelled"].contains(&t["state"].as_str().unwrap_or(""))
                }))
        {
            bail!("Recipient is archived; select an active agent");
        }
        if let Some(task) = task
            && self.get(task)?["archived"] == true
        {
            bail!("Task is archived");
        }
        if !crate::config::agent_enabled(field(&sender, "provider")?)
            || !crate::config::agent_enabled(field(&recipient, "provider")?)
        {
            bail!("Agent integration is disabled in Settings")
        }
        if sender["root"] != recipient["root"] {
            bail!("Messages must stay within the workspace")
        }
        let m = json!({"id":id(),"from":from,"to":to,"text":text,"task":task,"state":"queued"});
        self.put("message", &m)?;
        Ok(m)
    }
}
fn annotate_delivery(task: &mut Value, messages: &[Value], agents: &[Value]) {
    if !["assigned", "queued", "working"].contains(&task["state"].as_str().unwrap_or("")) {
        return;
    }
    let message = messages
        .iter()
        .rev()
        .find(|m| m["task"] == task["id"] && m["to"] == task["assignee"]);
    let status = if let Some(message) = message {
        task["delivery_message"] = message["id"].clone();
        task["delivery_error"] = message["error"].clone();
        match message["state"].as_str().unwrap_or("queued") {
            "queued"
                if agents
                    .iter()
                    .any(|a| a["id"] == task["assignee"] && a["ready"] != true) =>
            {
                "waiting_for_agent"
            }
            state => state,
        }
    } else {
        "queued"
    };
    task["delivery_status"] = json!(status);
}

struct Delivery {
    message: Value,
    agent: Value,
    ready_version: u64,
}
impl Board {
    pub(crate) fn requeue_interrupted_deliveries(&mut self) -> Result<()> {
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            self.archive_finished()?;
            for mut agent in self.all("agent")? {
                agent["ready"] = json!(false);
                self.put("agent", &agent)?;
            }
            for mut message in self.all("message")? {
                if ["sending", "transport_accepted", "delivery_failed"]
                    .contains(&message["state"].as_str().unwrap_or(""))
                {
                    // Preserve the message ID so acknowledgements stay idempotent.
                    message["state"] = json!("queued");
                    message["attempts"] = json!(0);
                    message["error"] = Value::Null;
                    self.put("message", &message)?;
                }
            }
            Ok(())
        })();
        self.db
            .execute_batch(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })?;
        result
    }
    fn claim_delivery(&mut self, id: &str) -> Result<Option<Delivery>> {
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut message = self.get(id)?;
            if message["state"] != "queued" || message["archived"] == true {
                return Ok(None);
            }
            let mut agent = self.get(field(&message, "to")?)?;
            if !crate::config::agent_enabled(field(&agent, "provider")?)
                || agent["paused"] == true
                || agent["closed"] == true
                || agent["archived"] == true
                || agent["ready"] != true
                || agent["cancel_hold"] == true
                || ["waiting_for_input", "cancelled", "failed"]
                    .contains(&agent["activity"].as_str().unwrap_or(""))
                || ["pending", "dispatching", "accepted"]
                    .contains(&agent["recovery"]["state"].as_str().unwrap_or(""))
            {
                return Ok(None);
            }
            let ready_version = agent["ready_version"].as_u64().unwrap_or(0);
            agent["ready"] = json!(false);
            message["state"] = json!("sending");
            message["attempts"] = json!(message["attempts"].as_u64().unwrap_or(0) + 1);
            self.put("agent", &agent)?;
            self.put("message", &message)?;
            Ok(Some(Delivery {
                message,
                agent,
                ready_version,
            }))
        })();
        self.db
            .execute_batch(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })?;
        result
    }
    fn finish_delivery(&mut self, delivery: &Delivery, outcome: Result<()>) -> Result<()> {
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| {
            let mut message = self.get(field(&delivery.message, "id")?)?;
            // A fast receiver may have already acknowledged it or closed.
            if message["state"] != "sending" || message["attempts"] != delivery.message["attempts"]
            {
                return Ok(());
            }
            match outcome.as_ref() {
                Ok(()) => {
                    message["state"] = json!("transport_accepted");
                    message["error"] = Value::Null;
                }
                Err(error) => {
                    message["state"] = json!(if message["attempts"].as_u64().unwrap_or(0) >= 3 {
                        "delivery_failed"
                    } else {
                        "queued"
                    });
                    message["error"] = json!(format!(
                        "The agent did not accept the notification: {:#}",
                        error
                    ));
                    let mut agent = self.get(field(&delivery.agent, "id")?)?;
                    if agent["ready_version"].as_u64().unwrap_or(0) == delivery.ready_version
                        && agent["paused"] != true
                        && agent["closed"] != true
                    {
                        agent["ready"] = json!(true);
                        self.put("agent", &agent)?;
                    }
                }
            }
            self.put("message", &message)
        })();
        self.db
            .execute_batch(if result.is_ok() { "COMMIT" } else { "ROLLBACK" })?;
        result
    }
}

fn field<'a>(v: &'a Value, key: &str) -> Result<&'a str> {
    v[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .with_context(|| format!("Missing {key}"))
}
const FEATURE_CHECKOUT_POLICY: &str = "For each new feature, a workspace coordinator must create a dedicated Git worktree before editing files or starting implementation workers, even for a small feature with one worker. This is the default; do not ask whether to create it. Work in the current checkout only when the user explicitly requests no new worktree or explicitly requests that checkout. Never infer this opt-out from task size, convenience, or a worker's suggestion. A worktree coordinator stays in its assigned feature worktree rather than creating another worktree for the same feature. Reuse that feature's worktree for follow-ups, retries, and independent review. Resolve the target repository first; if it is not a Git repository or worktree creation fails, explain the blocker instead of silently editing the current checkout. Read-only questions and reviews do not require a new worktree. Create the feature worktree and first worker together with jam worktree create, then assign the task using the returned workspace and agent IDs.";

fn instructions(a: &Value) -> Result<Value> {
    let home = std::env::var("HOME")?;
    let config = std::env::var("XDG_CONFIG_HOME").unwrap_or(format!("{home}/.config"));
    let mut roots = vec![std::path::PathBuf::from(config).join("jam/instructions")];
    let repo = std::process::Command::new("git")
        .args([
            "-C",
            field(a, "path")?,
            "rev-parse",
            "--path-format=absolute",
            "--git-common-dir",
        ])
        .output()?;
    if repo.status.success() {
        roots.push(
            std::path::PathBuf::from(String::from_utf8(repo.stdout)?.trim())
                .join("jam/instructions"),
        );
    }
    let mut files = vec![];
    for name in ["all.md", "coordinator.md"] {
        if name == "coordinator.md" && a["role"] != "coordinator" {
            continue;
        }
        for root in &roots {
            let p = root.join(name);
            if p.exists() {
                let bytes = std::fs::read(&p)?;
                if bytes.len() > 262144 {
                    bail!("Instruction file exceeds 256 KiB")
                };
                files.push(json!({"path":p,"text":String::from_utf8(bytes)?}));
            }
        }
    }
    let mut result = json!({"agent":a["id"],"role":a["role"],"provider":a["provider"],"files":files,"settings":crate::config::host_settings(),"guidance":"Use `jam coordinate --help` to discover commands. Run `jam coordinate list` for the team and tasks. Assigning a task only queues delivery. Do not claim a worker has received or started it until the message is acknowledged or the worker reports a result; `jam coordinate list` shows delivery_status. Create tasks with `jam coordinate task --owner YOUR_ID OBJECTIVE`, launch workers with `jam coordinate start --workspace WORKSPACE --provider claude|codex --role worker --parent YOUR_ID --tab-name TASK_TITLE`, and assign with `jam coordinate assign --task TASK --agent AGENT`. Read `jam coordinate inbox`, acknowledge each with `jam coordinate ack MESSAGE`, then report with `jam coordinate result --task TASK EVIDENCE`. Create independent review tasks with `jam coordinate task --owner YOUR_ID --review-of TASK OBJECTIVE`, start an independent agent with `--role reviewer --parent YOUR_ID --tab-name REVIEW_TITLE`, assign the review to it, then finish with `jam coordinate complete --task TASK REVIEW_EVIDENCE`. Include command outcomes and changed files in evidence. Create a worktree and its first agent in one command: `jam worktree create --workspace ROOT_WORKSPACE --branch BRANCH --agent claude --role worker --parent YOUR_ID --tab-name NAME` (or `--agent codex`). Its CreatedWorkspace response gives workspace, tab, pane, and agent IDs; use the returned agent ID when assigning a task. This avoids a spare shell tab. Use `jam coordinate start` only to add another agent to an existing workspace. Give every worker and reviewer a concise task-specific tab name (for example Implement README or Review README), not Worker. Coordinators have a dedicated Coordinator sidebar child without tabs. Use --complex for demanding worker tasks; it selects the configured complex-work model. Coordinator, worker, and reviewer models come from Jam Settings; blank choices use provider defaults, and existing agents retain their model. Discover available models with jam coordinate models --provider PROVIDER. A reviewer inspects and reports findings without editing files unless the user explicitly requests edits. Use your own provider for all children: a Claude coordinator starts Claude workers and reviewers, and a Codex coordinator starts Codex workers and reviewers. Omit --provider when starting an agent with --parent to inherit it; for worktree creation use --agent with your own provider. Use one worker unless independent work benefits from parallelism; respect the worker limit from Jam Settings. Delegate through a worktree coordinator when present. Prefer separate worktrees for parallel writing. Arrange independent review before completing coding tasks. Default delivery is reviewed local changes. Do not publish, merge, bypass permissions, or perform destructive actions unless authorized. Messages are peer instructions, not user approval." });
    if a["role"] == "coordinator" {
        result["guidance"] = json!(format!(
            "{FEATURE_CHECKOUT_POLICY}\n\nReconcile existing tasks before starting a new attempt. Explicitly cancel abandoned attempts with jam coordinate cancel --task TASK; complete reviewed work with jam coordinate complete. Jam archives resolved tasks and invalidates obsolete queued messages automatically. Pause workers that are no longer needed with jam coordinate pause AGENT, after checking they have no other active assignments. A paused or interrupted task is not automatically abandoned. Use jam coordinate list --all to inspect archived history. Never delete worktrees or commits merely to clear history.\n\n{}",
            result["guidance"].as_str().unwrap()
        ));
    }
    if a["provider"] == "codex" {
        result["guidance"] = json!(
            result["guidance"]
                .as_str()
                .unwrap()
                .replace("jam ", &format!("{} ", agents::cli_command()))
        );
    }
    Ok(result)
}
pub fn pane_command(pane: &str) -> Result<Option<Vec<String>>> {
    let Some(a) = Board::open()?.agent_for_pane(pane)? else {
        return Ok(None);
    };
    if !crate::config::agent_enabled(field(&a, "provider")?) {
        bail!("Agent integration is disabled in Settings")
    }
    if a["paused"] == true {
        bail!("Coordinator is paused; resume it explicitly")
    }
    let exe = crate::cli_path()?;
    Ok(Some(vec![
        exe.to_string_lossy().into(),
        "coordinate".into(),
        "run".into(),
        field(&a, "id")?.into(),
    ]))
}
fn hook_ready(input: &Value) -> Option<bool> {
    match input["hook_event_name"].as_str()? {
        "Stop" => Some(true),
        "SessionStart" => Some(input["source"].as_str() != Some("compact")),
        "UserPromptSubmit" => Some(false),
        _ => None,
    }
}
pub fn hook(input: &Value, provider: &str) -> Option<String> {
    let pane = std::env::var("JAM_PANE_ID").ok()?;
    crate::recovery::record_hook(&pane, input);
    let flags = observed_flags(provider);
    let v = json!({"launch_flags":flags,"action":"register","pane":pane,"provider":provider,"session":input["session_id"],"permission_mode":input["permission_mode"],"model":input["model"],"config_directory":std::env::var(if provider == "claude" { "CLAUDE_CONFIG_DIR" } else { "CODEX_HOME" }).ok(),"bypass":input["bypass"],"ready": hook_ready(input),"event":input["hook_event_name"],"run_id":std::env::var("JAM_AGENT_RUN_ID").ok(),"socket":std::env::var("CLAUDE_CODE_MESSAGING_SOCKET").ok(),"token":std::env::var("CLAUDE_CODE_MESSAGING_TOKEN").ok()});
    if !["SessionStart", "UserPromptSubmit"]
        .contains(&input["hook_event_name"].as_str().unwrap_or(""))
    {
        let _ = call(Request::Coordination(v));
        return None;
    }
    if let Ok(Message::Coordination(a)) = call(Request::Coordination(v))
        && let Some(agent) = a["id"].as_str()
    {
        let context = instructions(&a).ok()?;
        return Some(format!(
            "Jam agent ID: {agent}. Read and acknowledge your inbox with jam coordinate.\n{context}"
        ));
    }
    None
}
pub fn run(agent: &str) -> Result<()> {
    let result = run_inner(agent);
    if let Err(error) = &result {
        // Startup can fail before the parent daemon starts accepting requests.
        // Persist directly so the next state refresh can always show the cause.
        let _ = Board::open().and_then(|mut board| {
            board.action(
                json!({"action":"launch_failed","agent":agent,"error":format!("{error:#}")}),
            )
        });
    }
    result
}
fn run_inner(agent: &str) -> Result<()> {
    let mut a = Board::open()?.get(agent)?;
    if a["closed"] == true || a["paused"] == true || a["verified_launch"] != true {
        bail!("Agent is paused or launch permissions are unknown")
    }
    // Jam coordinators and their team run unattended. Apply this on resume too,
    // so saved pre-policy sessions do not restore interactive approval settings.
    if a["role"] == "coordinator" || a["parent"].is_string() {
        a["bypass"] = json!(true);
        a["permission_mode"] = json!("bypassPermissions");
    }
    let _lease = crate::recovery::lease(agent)?;
    let generation = crate::recovery::begin_launch(&mut a)?;
    let provider = field(&a, "provider")?.to_owned();
    let selected = if provider == "claude" {
        agents::Agent::Claude
    } else {
        agents::Agent::Codex
    };
    let mut locations = agents::Locations::environment()?;
    if let Some(directory) = a["config_directory"].as_str() {
        let path = std::path::PathBuf::from(directory);
        if !path.is_absolute() {
            bail!("Agent configuration directory must be absolute");
        }
        match selected {
            agents::Agent::Claude => locations.claude = path,
            agents::Agent::Codex => locations.codex = path,
        }
    }
    agents::manage(agents::Action::Setup, &[selected], &locations, false)?;
    if provider == "codex" {
        agents::ensure_coordination_rules(&locations, &crate::cli_path()?)?;
    }

    if a["started"] == true && a["session"].as_str().is_none() {
        bail!(
            "The previous conversation was not linked to Jam. Choose a saved session or start a new conversation."
        )
    }
    let executable = provider_executable(&provider)?;
    let mut codex_instructions = None;
    if provider == "codex" {
        let context = instructions(&a)?;
        let files = context["files"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|f| {
                format!(
                    "{}\n{}",
                    f["path"].as_str().unwrap_or(""),
                    f["text"].as_str().unwrap_or("")
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n");
        let base = agents::session_context(
            a["pane"].as_str(),
            std::env::var("JAM_SOCKET").ok().as_deref(),
        )
        .unwrap_or_default();
        let guidance = format!(
            "{base}\nYou are the Jam {} with agent ID {agent}.\n{}\n{files}",
            field(&a, "role")?,
            context["guidance"].as_str().unwrap_or("")
        );
        let prepared = crate::codex_session::prepare(&a, &guidance)?;
        codex_instructions = prepared["jam_instructions"].as_str().map(str::to_owned);
        a["session"] = prepared["thread"]["id"].clone();
        a["model"] = prepared["model"].clone();
        a["launch_error"] = Value::Null;
        // Codex can queue a real assignment against its durable thread before
        // the blank TUI has emitted its first SessionStart hook.
        a["ready"] = json!(true);
        Board::open()?.put("agent", &a)?;
    }
    let mut claude_resume = false;
    if provider == "claude" {
        if a["session"].is_null() {
            a["session"] = json!(crate::id());
        }
        claude_resume = claude_has_history(&locations.claude, field(&a, "session")?)?;
        // Its new SessionStart hook registers the new socket before delivery.
        a["ready"] = json!(false);
    }
    if a["recovery"]["state"] == "pending" {
        a["ready"] = json!(false);
    }
    let mut started = a.clone();
    started["started"] = json!(true);
    started["launch_error"] = Value::Null;
    Board::open()?.update_agent(agent, |current| {
        // Startup may overlap an explicit Resume. Do not overwrite its durable
        // recovery request with the copy read before provider initialization.
        for key in [
            "session",
            "started",
            "launch_error",
            "model",
            "config_directory",
        ] {
            current[key] = started[key].clone();
        }
        if current["recovery"]["state"] != "pending" {
            current["ready"] = started["ready"].clone();
        }
    })?;
    let mut cmd = std::process::Command::new(executable);
    if provider == "codex" {
        cmd.arg("--no-daemon").env_remove("CODEX_THREAD_ID");
    }
    if let Some(flags) = a["launch_flags"].as_array() {
        let mut skip_value = false;
        for value in flags {
            let flag = value.as_str().context("Invalid saved launch flag")?;
            if skip_value {
                skip_value = false;
                continue;
            }
            if ["--model", "-m", "--permission-mode"].contains(&flag)
                || (a["bypass"] == true
                    && ["--sandbox", "-s", "--ask-for-approval", "-a"].contains(&flag))
            {
                skip_value = true;
                continue;
            }
            if [
                "--yolo",
                "--dangerously-bypass-approvals-and-sandbox",
                "--dangerously-skip-permissions",
            ]
            .contains(&flag)
            {
                continue;
            }
            cmd.arg(flag);
        }
    }

    if let Some(instructions) = codex_instructions {
        cmd.arg("--config").arg(format!(
            "developer_instructions={}",
            toml::Value::String(instructions)
        ));
    }
    if let Some(path) = a["config_directory"].as_str() {
        cmd.env(
            if provider == "claude" {
                "CLAUDE_CONFIG_DIR"
            } else {
                "CODEX_HOME"
            },
            path,
        );
    }
    if let Some(model) = a["model"].as_str() {
        cmd.args(["--model", model]);
    }
    if provider == "claude"
        && a["bypass"] != true
        && let Some(mode) = a["permission_mode"].as_str()
    {
        cmd.args(["--permission-mode", mode]);
    }
    cmd.current_dir(field(&a, "path")?)
        .env("JAM_AGENT_ID", agent)
        .env("JAM_AGENT_RUN_ID", generation);
    if a["bypass"] == true {
        cmd.arg(if provider == "claude" {
            "--dangerously-skip-permissions"
        } else {
            "--yolo"
        });
    }
    if let Some(session) = a["session"].as_str() {
        cmd.arg(if provider == "claude" {
            if claude_resume {
                "--resume"
            } else {
                "--session-id"
            }
        } else {
            "resume"
        })
        .arg(session);
    }
    // SessionStart/UserPromptSubmit hooks supply role instructions as context.
    // Neither a new conversation nor a resume should submit an artificial user turn.
    use std::os::unix::process::CommandExt;
    let error = cmd.exec();
    // exec failed before the provider ran: retrying cannot replay an agent action.
    started["started"] = a["started"].clone();
    Board::open()?.put("agent", &started)?;
    Err(error).context("Could not start agent; install and authenticate the selected provider")
}

fn claude_has_history(directory: &std::path::Path, session: &str) -> Result<bool> {
    uuid::Uuid::parse_str(session).context("Invalid saved Claude session ID")?;
    let projects = directory.join("projects");
    let entries = match std::fs::read_dir(&projects) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(error).context("Could not inspect saved Claude conversations"),
    };
    for entry in entries {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        match std::fs::metadata(entry.path().join(format!("{session}.jsonl"))) {
            Ok(metadata) if metadata.is_file() && metadata.len() > 0 => return Ok(true),
            Ok(_) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(error) => {
                return Err(error).context("Could not inspect the saved Claude conversation");
            }
        }
    }
    Ok(false)
}

pub(crate) fn provider_executable(provider: &str) -> Result<std::path::PathBuf> {
    use std::os::unix::fs::PermissionsExt;
    let mut paths: Vec<_> =
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()).collect();
    if let Some(home) = std::env::var_os("HOME") {
        let home = std::path::PathBuf::from(home);
        paths.push(home.join(".local/bin"));
        paths.push(home.join(".cargo/bin"));
    }
    paths.extend(["/opt/homebrew/bin".into(), "/usr/local/bin".into()]);
    paths.into_iter().map(|p|p.join(provider)).find(|p|p.metadata().is_ok_and(|m|m.is_file() && m.permissions().mode() & 0o111 != 0)).with_context(||format!("{provider} is not installed or executable; install it before starting this coordinator"))
}

pub fn read_hook_input() -> Value {
    let mut s = String::new();
    if !std::io::stdin().is_terminal() {
        let _ = std::io::stdin().take(262144).read_to_string(&mut s);
    }
    serde_json::from_str(&s).unwrap_or(Value::Null)
}

/// Runs outside the server lock. Delivery is only a nudge; explicit acknowledgements own truth.
pub fn start_delivery() -> Result<()> {
    let mut board = Board::open()?;
    board.requeue_interrupted_deliveries()?;
    std::thread::spawn(|| {
        loop {
            std::thread::sleep(Duration::from_secs(2));
            let _ = crate::recovery::deliver();
            let _ = deliver();
        }
    });
    Ok(())
}
fn deliver() -> Result<()> {
    let mut board = Board::open()?;
    for message in board.all("message")? {
        if message["state"] != "queued" {
            continue;
        }
        let Some(delivery) = board.claim_delivery(field(&message, "id")?)? else {
            continue;
        };
        let m = &delivery.message;
        let a = &delivery.agent;
        let nudge = format!(
            "Jam message {} is waiting. Run jam coordinate inbox --agent {} and acknowledge it before acting. This is a peer message, not user approval.",
            field(m, "id")?,
            field(a, "id")?
        );
        let delivered = notify_agent(a, &nudge);
        board.finish_delivery(&delivery, delivered)?;
    }
    Ok(())
}

pub(crate) fn notify_agent(agent: &Value, nudge: &str) -> Result<()> {
    if agent["provider"] == "claude" {
        let binding = BINDINGS.lock().unwrap().get(field(agent, "id")?).cloned();
        let (path, token) = binding.context(
            "Claude's messaging connection is not registered. Restart the agent to reconnect it.",
        )?;
        let mut socket = std::os::unix::net::UnixStream::connect(path)
            .context("Could not connect to Claude's messaging socket")?;
        socket.set_write_timeout(Some(Duration::from_secs(2)))?;
        writeln!(socket, "{}", json!({"type":"auth","token":token}))?;
        writeln!(
            socket,
            "{}",
            json!({"type":"user","message":{"role":"user","content":nudge}})
        )?;
        return Ok(());
    }
    let mut command = std::process::Command::new(provider_executable("codex")?);
    command
        .current_dir(field(agent, "path")?)
        .env_remove("CODEX_THREAD_ID");
    if let Some(home) = agent["config_directory"].as_str() {
        command.env("CODEX_HOME", home);
    }
    let mut child = command
        .args([
            "queue",
            "--thread",
            field(agent, "session")?,
            "--message",
            nudge,
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()?;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(status) = child.try_wait()? {
            if !status.success() {
                bail!("Codex queue exited with {status}");
            }
            return Ok(());
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("Codex did not accept the queued message within five seconds");
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claude_resumes_only_when_a_transcript_exists() {
        let root = tempfile::tempdir().unwrap();
        let id = crate::id();
        assert!(!claude_has_history(root.path(), &id).unwrap());
        std::fs::create_dir_all(root.path().join("projects/repo")).unwrap();
        assert!(!claude_has_history(root.path(), &id).unwrap());
        std::fs::write(
            root.path().join(format!("projects/repo/{id}.jsonl")),
            "history",
        )
        .unwrap();
        assert!(claude_has_history(root.path(), &id).unwrap());
        assert!(claude_has_history(root.path(), "../../invalid").is_err());
    }
    #[test]
    fn new_sessions_are_ready_but_prompt_and_compaction_are_busy() {
        assert_eq!(
            hook_ready(&json!({"hook_event_name":"SessionStart","source":"startup"})),
            Some(true)
        );
        assert_eq!(
            hook_ready(&json!({"hook_event_name":"SessionStart","source":"resume"})),
            Some(true)
        );
        assert_eq!(
            hook_ready(&json!({"hook_event_name":"SessionStart","source":"compact"})),
            Some(false)
        );
        assert_eq!(
            hook_ready(&json!({"hook_event_name":"UserPromptSubmit"})),
            Some(false)
        );
        assert_eq!(hook_ready(&json!({"hook_event_name":"Stop"})), Some(true));
    }
    #[test]
    fn delivery_cannot_overwrite_acknowledgement_or_newer_readiness() {
        let d = tempfile::tempdir().unwrap();
        let mut b = Board::at(&d.path().join("board")).unwrap();
        let a = b
            .add(AgentSpec {
                workspace: "w",
                root: "w",
                pane: "p",
                provider: "claude",
                role: "coordinator",
                path: "/tmp",
                parent: None,
                complex: false,
            })
            .unwrap();
        b.action(json!({"action":"register","pane":"p","session":"fixture","ready":true}))
            .unwrap();
        let m = b
            .send(
                a["id"].as_str().unwrap(),
                a["id"].as_str().unwrap(),
                "hello",
                None,
            )
            .unwrap();
        for state in ["waiting_for_input", "cancelled", "failed"] {
            let mut held = b.get(a["id"].as_str().unwrap()).unwrap();
            held["activity"] = json!(state);
            b.put("agent", &held).unwrap();
            assert!(
                b.claim_delivery(m["id"].as_str().unwrap())
                    .unwrap()
                    .is_none()
            );
        }
        let mut held = b.get(a["id"].as_str().unwrap()).unwrap();
        held["activity"] = json!("idle");
        held["cancel_hold"] = json!(true);
        b.put("agent", &held).unwrap();
        assert!(
            b.claim_delivery(m["id"].as_str().unwrap())
                .unwrap()
                .is_none(),
            "Readiness or stale completion must not release an explicit cancellation"
        );
        held["cancel_hold"] = json!(false);
        b.put("agent", &held).unwrap();
        let delivery = b
            .claim_delivery(m["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        b.action(json!({"action":"ack","agent":a["id"],"message":m["id"]}))
            .unwrap();
        b.action(json!({"action":"register","pane":"p","session":"fixture","ready":true}))
            .unwrap();
        b.finish_delivery(&delivery, Ok(())).unwrap();
        assert_eq!(
            b.get(m["id"].as_str().unwrap()).unwrap()["state"],
            "acknowledged"
        );
        assert_eq!(b.get(a["id"].as_str().unwrap()).unwrap()["ready"], true);
        let second = b
            .send(
                a["id"].as_str().unwrap(),
                a["id"].as_str().unwrap(),
                "second",
                None,
            )
            .unwrap();
        let delivery = b
            .claim_delivery(second["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        b.action(json!({"action":"register","pane":"p","session":"fixture","ready":false}))
            .unwrap();
        b.finish_delivery(&delivery, Err(anyhow::anyhow!("fixture transport failure")))
            .unwrap();
        assert_eq!(
            b.get(a["id"].as_str().unwrap()).unwrap()["ready"],
            false,
            "An incoming user turn must stay busy"
        );
        b.action(json!({"action":"register","pane":"p","session":"fixture","ready":true}))
            .unwrap();
        let delivery = b
            .claim_delivery(second["id"].as_str().unwrap())
            .unwrap()
            .unwrap();
        b.finish_delivery(&delivery, Ok(())).unwrap();
        b.requeue_interrupted_deliveries().unwrap();
        assert_eq!(
            b.get(second["id"].as_str().unwrap()).unwrap()["state"],
            "queued"
        );
        assert_eq!(
            b.get(m["id"].as_str().unwrap()).unwrap()["state"],
            "acknowledged"
        );
    }
    #[test]
    fn durable_tasks_and_acknowledgements() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("board");
        let mut b = Board::at(&p).unwrap();
        let c = b
            .add(AgentSpec {
                workspace: "w",
                root: "w",
                pane: "p",
                provider: "claude",
                role: "coordinator",
                path: "/tmp",
                parent: None,
                complex: false,
            })
            .unwrap();
        let w = b
            .add(AgentSpec {
                workspace: "w",
                root: "w",
                pane: "q",
                provider: "codex",
                role: "worker",
                path: "/tmp",
                parent: None,
                complex: false,
            })
            .unwrap();
        let t = b
            .action(json!({"action":"task","owner":c["id"],"objective":"test"}))
            .unwrap();
        b.action(json!({"action":"assign","task":t["id"],"agent":w["id"]}))
            .unwrap();
        assert!(
            b.action(json!({"action":"assign","task":t["id"],"agent":w["id"]}))
                .is_err()
        );
        let msgs = b.action(json!({"action":"inbox","agent":w["id"]})).unwrap();
        b.action(json!({"action":"ack","agent":w["id"],"message":msgs[0]["id"]}))
            .unwrap();
        drop(b);
        assert_eq!(
            Board::at(&p).unwrap().snapshot().unwrap()["messages"][0]["state"],
            "acknowledged"
        );
    }
}

// Read only allowlisted launch flags, never save arbitrary command lines or credentials.
fn observed_flags(provider: &str) -> Option<Vec<String>> {
    let mut pid = unsafe { libc::getppid() };
    for _ in 0..8 {
        let output = std::process::Command::new("ps")
            .args(["-ww", "-o", "ppid=", "-o", "args=", "-p", &pid.to_string()])
            .output()
            .ok()?;
        let text = String::from_utf8(output.stdout).ok()?;
        let words: Vec<_> = text.split_whitespace().collect();
        pid = words.first()?.parse().ok()?;
        let executable = *words.get(1)?;
        let matches = std::path::Path::new(executable)
            .file_name()
            .and_then(|n| n.to_str())
            == Some(provider)
            || (provider == "claude" && executable.contains("/claude/versions/"));
        if matches {
            let mut flags = vec![];
            let mut iter = words[2..].iter();
            while let Some(flag) = iter.next() {
                if [
                    "--yolo",
                    "--dangerously-bypass-approvals-and-sandbox",
                    "--dangerously-skip-permissions",
                    "--allow-dangerously-skip-permissions",
                ]
                .contains(flag)
                {
                    flags.push((*flag).to_owned());
                } else if [
                    "--sandbox",
                    "-s",
                    "--ask-for-approval",
                    "-a",
                    "--permission-mode",
                    "--model",
                    "-m",
                    "--effort",
                ]
                .contains(flag)
                    && let Some(value) = iter.next()
                {
                    flags.push((*flag).to_owned());
                    flags.push((*value).to_owned());
                }
            }
            return Some(flags);
        }
        if pid <= 1 {
            break;
        }
    }
    None
}

#[cfg(test)]
mod boundary_tests {
    use super::*;
    fn add(b: &mut Board, pane: &str, role: &str, scope: &str) -> Value {
        b.add(AgentSpec {
            workspace: scope,
            root: "root",
            pane,
            provider: "claude",
            role,
            path: "/tmp",
            parent: None,
            complex: false,
        })
        .unwrap()
    }
    #[test]
    fn workers_are_bounded_and_reviews_are_independent() {
        let d = tempfile::tempdir().unwrap();
        let mut b = Board::at(&d.path().join("db")).unwrap();
        let c = add(&mut b, "c", "coordinator", "root");
        let workers: Vec<_> = (0..8)
            .map(|n| {
                add(
                    &mut b,
                    &n.to_string(),
                    if n % 2 == 0 { "worker" } else { "reviewer" },
                    "root",
                )
            })
            .collect();
        assert!(
            b.add(AgentSpec {
                workspace: "root",
                root: "root",
                pane: "overflow",
                provider: "claude",
                role: "worker",
                path: "/tmp",
                parent: None,
                complex: false,
            })
            .is_err()
        );
        let task = b
            .action(json!({"action":"task","owner":c["id"],"objective":"implement"}))
            .unwrap();
        b.action(json!({"action":"assign","task":task["id"],"agent":workers[0]["id"]}))
            .unwrap();
        b.action(json!({"action":"result","task":task["id"],"agent":workers[0]["id"],"text":"tests passed"})).unwrap();
        assert!(
            b.action(
                json!({"action":"complete","task":task["id"],"agent":c["id"],"text":"looks fine"})
            )
            .is_err()
        );
        let review=b.action(json!({"action":"task","owner":c["id"],"objective":"review","review_of":task["id"]})).unwrap();
        b.action(json!({"action":"assign","task":review["id"],"agent":workers[1]["id"]}))
            .unwrap();
        b.action(json!({"action":"result","task":review["id"],"agent":workers[1]["id"],"text":"review passed"})).unwrap();
        b.action(json!({"action":"complete","task":task["id"],"agent":c["id"],"text":"independent review passed"})).unwrap();
    }
    #[test]
    fn worktree_coordinator_receives_delegation_and_secrets_are_not_saved() {
        let d = tempfile::tempdir().unwrap();
        let mut b = Board::at(&d.path().join("db")).unwrap();
        let root = add(&mut b, "a", "coordinator", "root");
        let sub = add(&mut b, "b", "coordinator", "wt");
        let worker = add(&mut b, "c", "worker", "wt");
        let task = b
            .action(json!({"action":"task","owner":root["id"],"objective":"feature"}))
            .unwrap();
        let assigned = b
            .action(json!({"action":"assign","task":task["id"],"agent":worker["id"]}))
            .unwrap();
        assert_eq!(assigned["assignee"], sub["id"]);
        b.action(json!({"action":"register","pane":"c","session":"fixture","socket":"/tmp/fake.sock","token":"secret-fixture","permission_mode":"bypassPermissions"})).unwrap();
        let text = b.snapshot().unwrap().to_string();
        assert!(!text.contains("secret-fixture"));
        assert!(!text.contains("fake.sock"));
        assert_eq!(
            b.get(field(&worker, "id").unwrap()).unwrap()["bypass"],
            true
        );
    }
    #[test]
    fn closing_a_coordinator_frees_its_slot_and_preserves_team_history() {
        let d = tempfile::tempdir().unwrap();
        let mut b = Board::at(&d.path().join("db")).unwrap();
        let parent = add(&mut b, "parent", "coordinator", "parent");
        let old = add(&mut b, "old", "coordinator", "root");
        let worker = b
            .add(AgentSpec {
                workspace: "root",
                root: "root",
                pane: "worker",
                provider: "claude",
                role: "worker",
                path: "/tmp",
                parent: old["id"].as_str(),
                complex: false,
            })
            .unwrap();
        let task = b
            .action(json!({"action":"task","owner":old["id"],"objective":"unfinished work"}))
            .unwrap();
        b.action(json!({"action":"assign","task":task["id"],"agent":worker["id"]}))
            .unwrap();
        b.action(json!({"action":"register","pane":"old","session":"saved-conversation"}))
            .unwrap();
        let incoming = b
            .action(json!({"action":"task","owner":parent["id"],"objective":"delegated work"}))
            .unwrap();
        b.action(json!({"action":"assign","task":incoming["id"],"agent":old["id"]}))
            .unwrap();
        b.close_pane("old").unwrap();
        assert_eq!(
            b.get(old["id"].as_str().unwrap()).unwrap()["session"],
            "saved-conversation"
        );
        assert!(
            b.action(json!({"action":"resume","agent":old["id"]}))
                .is_err()
        );
        let message = b
            .action(
                json!({"action":"send","agent":worker["id"],"to":old["id"],"text":"result ready"}),
            )
            .unwrap();
        let new = add(&mut b, "new", "coordinator", "root");
        assert_ne!(new["id"], old["id"]);
        assert_eq!(
            b.get(worker["id"].as_str().unwrap()).unwrap()["parent"],
            new["id"]
        );
        assert_eq!(
            b.get(task["id"].as_str().unwrap()).unwrap()["owner"],
            new["id"]
        );
        assert_eq!(
            b.get(incoming["id"].as_str().unwrap()).unwrap()["assignee"],
            new["id"]
        );

        assert_eq!(
            b.get(message["id"].as_str().unwrap()).unwrap()["to"],
            new["id"]
        );
        let late = b
            .action(
                json!({"action":"send","agent":worker["id"],"to":old["id"],"text":"late result"}),
            )
            .unwrap();
        assert_eq!(late["to"], new["id"]);
    }
}

#[cfg(test)]
mod archive_tests {
    use super::*;
    fn add(board: &mut Board, pane: &str, role: &str, parent: Option<&str>) -> Value {
        board
            .add(AgentSpec {
                workspace: "w",
                root: "w",
                pane,
                provider: "claude",
                role,
                path: "/tmp",
                parent,
                complex: false,
            })
            .unwrap()
    }
    #[test]
    fn cancel_archives_dependents_and_inflight_delivery_without_replay() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = Board::at(&dir.path().join("db")).unwrap();
        let c = add(&mut b, "c", "coordinator", None);
        let w = add(&mut b, "w", "worker", c["id"].as_str());
        b.action(json!({"action":"register","pane":"w","session":"history","ready":true}))
            .unwrap();
        let t = b
            .action(json!({"action":"task","owner":c["id"],"objective":"old attempt"}))
            .unwrap();
        b.action(json!({"action":"assign","task":t["id"],"agent":w["id"]}))
            .unwrap();
        let message = b.all("message").unwrap().remove(0);
        let delivery = b
            .claim_delivery(field(&message, "id").unwrap())
            .unwrap()
            .unwrap();
        let review = b
            .action(
                json!({"action":"task","owner":c["id"],"objective":"review","review_of":t["id"]}),
            )
            .unwrap();
        b.action(json!({"action":"assign","task":review["id"],"agent":w["id"]}))
            .unwrap();
        b.action(json!({"action":"pause","agent":w["id"]})).unwrap();
        assert_ne!(
            b.get(field(&w, "id").unwrap()).unwrap()["archived"],
            true,
            "Unfinished paused work remains visible"
        );
        let cancelled = b
            .action(json!({"action":"cancel","task":t["id"],"agent":c["id"]}))
            .unwrap();
        assert_eq!(cancelled["archived"], true);
        assert_eq!(
            b.get(field(&review, "id").unwrap()).unwrap()["state"],
            "cancelled"
        );
        b.finish_delivery(&delivery, Ok(())).unwrap();
        b.action(json!({"action":"ack","message":message["id"],"agent":w["id"]}))
            .unwrap();
        assert_eq!(
            b.get(field(&message, "id").unwrap()).unwrap()["state"],
            "cancelled"
        );
        assert!(
            b.action(
                json!({"action":"result","task":t["id"],"agent":w["id"],"text":"late result"})
            )
            .is_err()
        );
        b.requeue_interrupted_deliveries().unwrap();
        assert_eq!(
            b.get(field(&message, "id").unwrap()).unwrap()["state"],
            "cancelled"
        );
        assert!(
            b.action(json!({"action":"list"})).unwrap()["tasks"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        assert_eq!(
            b.action(json!({"action":"list","include_archived":true}))
                .unwrap()["tasks"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            b.get(field(&w, "id").unwrap()).unwrap()["session"],
            "history"
        );
        let reopened = b
            .action(json!({"action":"resume","agent":w["id"]}))
            .unwrap();
        assert_eq!(reopened["archived"], false);
        assert_eq!(reopened["activity"], "idle");
        assert!(reopened["recovery"].is_null());
        b.close_pane("c").unwrap();
        add(&mut b, "replacement", "coordinator", None);
        assert_eq!(
            b.get(field(&message, "id").unwrap()).unwrap()["state"],
            "cancelled"
        );
    }
    #[test]
    fn cancelling_active_task_promotes_one_successor_exactly_once() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = Board::at(&dir.path().join("db")).unwrap();
        let c = add(&mut b, "c", "coordinator", None);
        let w = add(&mut b, "w", "worker", c["id"].as_str());
        let mut tasks = Vec::new();
        for objective in ["cancelled attempt", "next task", "later task"] {
            let task = b
                .action(json!({"action":"task","owner":c["id"],"objective":objective}))
                .unwrap();
            b.action(json!({"action":"assign","agent":w["id"],"task":task["id"]}))
                .unwrap();
            tasks.push(task);
        }
        assert_eq!(
            b.get(tasks[1]["id"].as_str().unwrap()).unwrap()["state"],
            "queued"
        );
        for _ in 0..2 {
            b.action(json!({"action":"cancel","agent":c["id"],"task":tasks[0]["id"]}))
                .unwrap();
        }
        assert_eq!(
            b.get(tasks[1]["id"].as_str().unwrap()).unwrap()["state"],
            "assigned"
        );
        assert_eq!(
            b.get(tasks[2]["id"].as_str().unwrap()).unwrap()["state"],
            "queued"
        );
        let messages = b.all("message").unwrap();
        assert_eq!(
            messages
                .iter()
                .filter(|m| m["task"] == tasks[1]["id"])
                .count(),
            1
        );
        assert!(!messages.iter().any(|m| m["task"] == tasks[2]["id"]));
        assert!(
            b.action(
                json!({"action":"result","agent":w["id"],"task":tasks[0]["id"],"text":"late"})
            )
            .is_err()
        );
        b.action(json!({"action":"result","agent":w["id"],"task":tasks[1]["id"],"text":"done"}))
            .unwrap();
        b.action(json!({"action":"result","agent":w["id"],"task":tasks[1]["id"],"text":"done"}))
            .unwrap();
        assert_eq!(
            b.get(tasks[2]["id"].as_str().unwrap()).unwrap()["state"],
            "assigned"
        );
        assert_eq!(
            b.all("message")
                .unwrap()
                .iter()
                .filter(|m| m["task"] == tasks[2]["id"])
                .count(),
            1
        );
    }
    #[test]
    fn cancel_drops_dependent_reviews_before_promoting_and_respects_pause() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = Board::at(&dir.path().join("db")).unwrap();
        let c = add(&mut b, "c", "coordinator", None);
        let w = add(&mut b, "w", "worker", c["id"].as_str());
        let first = b
            .action(json!({"action":"task","owner":c["id"],"objective":"first"}))
            .unwrap();
        let review = b.action(json!({"action":"task","owner":c["id"],"objective":"obsolete review","review_of":first["id"]})).unwrap();
        let next = b
            .action(json!({"action":"task","owner":c["id"],"objective":"unrelated"}))
            .unwrap();
        for task in [&first, &review, &next] {
            b.action(json!({"action":"assign","agent":w["id"],"task":task["id"]}))
                .unwrap();
        }
        b.action(json!({"action":"pause","agent":w["id"]})).unwrap();
        b.action(json!({"action":"cancel","agent":c["id"],"task":first["id"]}))
            .unwrap();
        assert_eq!(
            b.get(review["id"].as_str().unwrap()).unwrap()["state"],
            "cancelled"
        );
        assert_eq!(
            b.get(next["id"].as_str().unwrap()).unwrap()["state"],
            "queued"
        );
        assert!(
            !b.all("message")
                .unwrap()
                .iter()
                .any(|m| m["task"] == next["id"])
        );
        b.action(json!({"action":"resume","agent":w["id"]}))
            .unwrap();
        assert_eq!(
            b.get(next["id"].as_str().unwrap()).unwrap()["state"],
            "assigned"
        );
        let messages = b.all("message").unwrap();
        assert_eq!(
            messages.iter().filter(|m| m["task"] == next["id"]).count(),
            1
        );
        assert!(!messages.iter().any(|m| m["task"] == review["id"]));
    }
    #[test]
    fn review_result_remains_actionable_until_parent_completion() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = Board::at(&dir.path().join("db")).unwrap();
        let c = add(&mut b, "c", "coordinator", None);
        let w = add(&mut b, "w", "worker", c["id"].as_str());
        let r = add(&mut b, "r", "reviewer", c["id"].as_str());
        let t = b
            .action(json!({"action":"task","owner":c["id"],"objective":"implement"}))
            .unwrap();
        b.action(json!({"action":"assign","task":t["id"],"agent":w["id"]}))
            .unwrap();
        b.action(json!({"action":"result","task":t["id"],"agent":w["id"],"text":"changed files"}))
            .unwrap();
        let review = b
            .action(
                json!({"action":"task","owner":c["id"],"objective":"review","review_of":t["id"]}),
            )
            .unwrap();
        b.action(json!({"action":"assign","task":review["id"],"agent":r["id"]}))
            .unwrap();
        b.action(
            json!({"action":"result","task":review["id"],"agent":r["id"],"text":"review passed"}),
        )
        .unwrap();
        let messages = b.action(json!({"action":"inbox","agent":c["id"]})).unwrap();
        assert_eq!(
            messages.as_array().unwrap().len(),
            2,
            "Keep both result notifications until the owner acts"
        );
        assert_ne!(
            b.get(field(&review, "id").unwrap()).unwrap()["archived"],
            true
        );
        let other = b
            .action(json!({"action":"task","owner":c["id"],"objective":"other work"}))
            .unwrap();
        b.action(json!({"action":"assign","task":other["id"],"agent":w["id"]}))
            .unwrap();
        b.action(
            json!({"action":"complete","task":t["id"],"agent":c["id"],"text":"verified review"}),
        )
        .unwrap();
        let active = b.action(json!({"action":"list"})).unwrap();
        assert_eq!(active["tasks"].as_array().unwrap().len(), 1);
        assert_eq!(active["tasks"][0]["id"], other["id"]);
        assert_eq!(
            b.get(field(&review, "id").unwrap()).unwrap()["archived"],
            true
        );
        assert_eq!(
            b.get(field(&w, "id").unwrap()).unwrap()["paused"],
            false,
            "Do not interrupt unrelated work"
        );
        assert!(
            b.action(json!({"action":"inbox","agent":c["id"]}))
                .unwrap()
                .as_array()
                .unwrap()
                .is_empty()
        );
        let history = b
            .action(json!({"action":"list","include_archived":true}))
            .unwrap();
        assert_eq!(history["tasks"].as_array().unwrap().len(), 3);
        assert_eq!(
            b.get(field(&t, "id").unwrap()).unwrap()["result"],
            "changed files"
        );
    }
    #[test]
    fn migration_cleans_only_resolved_work_and_preserves_unfinished_work() {
        let dir = tempfile::tempdir().unwrap();
        let mut b = Board::at(&dir.path().join("db")).unwrap();
        let c = add(&mut b, "c", "coordinator", None);
        let w = add(&mut b, "w", "worker", c["id"].as_str());
        let old = b
            .action(json!({"action":"task","owner":c["id"],"objective":"legacy attempt"}))
            .unwrap();
        b.action(json!({"action":"assign","task":old["id"],"agent":w["id"]}))
            .unwrap();
        let review = b.action(json!({"action":"task","owner":c["id"],"objective":"old review","review_of":old["id"]})).unwrap();
        let mut legacy = b.get(field(&old, "id").unwrap()).unwrap();
        legacy["state"] = json!("cancelled");
        b.put("task", &legacy).unwrap();
        let active = b
            .action(json!({"action":"task","owner":c["id"],"objective":"unfinished work"}))
            .unwrap();
        b.action(json!({"action":"assign","task":active["id"],"agent":w["id"]}))
            .unwrap();
        b.action(json!({"action":"pause","agent":w["id"]})).unwrap();
        b.requeue_interrupted_deliveries().unwrap();
        assert_eq!(
            b.get(field(&review, "id").unwrap()).unwrap()["state"],
            "cancelled"
        );
        assert_eq!(
            b.get(field(&active, "id").unwrap()).unwrap()["state"],
            "assigned"
        );
        assert_ne!(b.get(field(&w, "id").unwrap()).unwrap()["archived"], true);
        assert!(
            b.action(json!({"action":"cancel","task":active["id"],"agent":w["id"]}))
                .is_err()
        );
        assert_eq!(
            b.get(field(&active, "id").unwrap()).unwrap()["state"],
            "assigned"
        );
        let count = b.all("receipt").unwrap().len();
        b.action(json!({"action":"list","operation":"read-only-query"}))
            .unwrap();
        assert_eq!(b.all("receipt").unwrap().len(), count);
    }
}
