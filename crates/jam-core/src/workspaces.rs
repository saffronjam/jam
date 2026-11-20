use crate::{RepositoryContext, State, Workspace};
use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};

pub fn active_path(state: &State, workspace: &Workspace) -> String {
    state
        .panes
        .iter()
        .find(|p| {
            p.workspace == workspace.id
                && p.agent_role.as_deref() != Some("coordinator")
                && Some(&p.id) == workspace.active_pane.as_ref()
        })
        .or_else(|| {
            state.panes.iter().find(|p| {
                p.workspace == workspace.id && p.agent_role.as_deref() != Some("coordinator")
            })
        })
        .map(|p| p.cwd.clone())
        .unwrap_or_else(|| {
            workspace
                .last_cwd
                .as_ref()
                .unwrap_or(&workspace.path)
                .clone()
        })
}

pub fn owner(state: &State, id: &str) -> Result<String> {
    let workspace = state
        .workspaces
        .iter()
        .find(|w| w.id == id)
        .context("Workspace missing")?;
    Ok(workspace.parent.as_ref().unwrap_or(&workspace.id).clone())
}

/// Convert the old implicit repository groups once, without merging independent
/// containers on subsequent launches. No terminal or checkout is recreated.
pub fn migrate(state: &mut State) {
    if state.workspace_model == 0 {
        let old = state.workspaces.clone();
        let mut roots: HashMap<String, String> = HashMap::new();
        for workspace in &old {
            if let Some(repo) = &workspace.repository
                && workspace.main_path.as_ref() == Some(&workspace.path)
            {
                roots
                    .entry(repo.clone())
                    .or_insert_with(|| workspace.id.clone());
            }
        }
        for workspace in &old {
            let (Some(repo), Some(main)) = (&workspace.repository, &workspace.main_path) else {
                continue;
            };
            if &workspace.path == main {
                continue;
            }
            let root = roots
                .entry(repo.clone())
                .or_insert_with(|| {
                    let id = uuid::Uuid::new_v4().to_string();
                    state.workspaces.push(Workspace {
                        coordinator: None,
                        id: id.clone(),
                        name: Path::new(main)
                            .file_name()
                            .unwrap_or_default()
                            .to_string_lossy()
                            .into_owned(),
                        path: main.clone(),
                        tabs: vec![],
                        auto_name: Some(true),
                        active_pane: None,
                        last_cwd: None,
                        repository: Some(repo.clone()),
                        main_path: Some(main.clone()),
                        branch: None,
                        parent: None,
                        context: None,
                    });
                    id
                })
                .clone();
            state
                .workspaces
                .iter_mut()
                .find(|w| w.id == workspace.id)
                .unwrap()
                .parent = Some(root);
        }
        state.workspace_model = 1;
    }
    // Preserve sessions from a damaged or manually edited layout as roots.
    let roots: Vec<_> = state
        .workspaces
        .iter()
        .filter(|w| w.parent.is_none())
        .map(|w| w.id.clone())
        .collect();
    for workspace in &mut state.workspaces {
        if workspace
            .parent
            .as_ref()
            .is_some_and(|p| !roots.contains(p))
        {
            workspace.parent = None;
        }
        workspace.context = None;
    }
}

struct Entry {
    checked: Instant,
    pending: bool,
    value: RepositoryContext,
}

/// Git subprocesses never execute while holding the daemon's state lock.
pub struct RepositoryContexts {
    entries: Arc<Mutex<HashMap<String, Entry>>>,
    requests: mpsc::SyncSender<String>,
}
impl Default for RepositoryContexts {
    fn default() -> Self {
        let entries = Arc::new(Mutex::new(HashMap::<String, Entry>::new()));
        let results = entries.clone();
        let (requests, receive) = mpsc::sync_channel::<String>(128);
        std::thread::spawn(move || {
            while let Ok(path) = receive.recv() {
                let mut value = empty_context(&path);
                value.ready = true;
                if let Ok(repo) = crate::worktree::inspect(Path::new(&path)) {
                    let root = std::process::Command::new("git")
                        .arg("-C")
                        .arg(&path)
                        .args(["rev-parse", "--show-toplevel"])
                        .output()
                        .ok()
                        .filter(|output| output.status.success())
                        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned());
                    value.checkout_path = root.clone();
                    value.branch = root.and_then(|root| {
                        repo.checkouts
                            .iter()
                            .find(|c| c.path == root)
                            .map(|c| c.branch.clone())
                    });
                    value.repository = Some(repo.common_dir);
                    value.main_path = Some(repo.main_path);
                    value.create_disabled_reason = repo.create_disabled_reason;
                }
                if let Some(entry) = results.lock().unwrap().get_mut(&path) {
                    entry.value = value;
                    entry.checked = Instant::now();
                    entry.pending = false;
                }
            }
        });
        Self { entries, requests }
    }
}
fn empty_context(path: &str) -> RepositoryContext {
    RepositoryContext {
        path: path.into(),
        checkout_path: None,
        ready: false,
        repository: None,
        main_path: None,
        branch: None,
        create_disabled_reason: None,
    }
}
impl RepositoryContexts {
    pub fn resolve(&self, path: &str) -> RepositoryContext {
        let mut entries = self.entries.lock().unwrap();
        if !entries.contains_key(path) && entries.len() >= 128 {
            if let Some(oldest) = entries
                .iter()
                .filter(|(_, e)| !e.pending)
                .min_by_key(|(_, e)| e.checked)
                .map(|(p, _)| p.clone())
            {
                entries.remove(&oldest);
            } else {
                return empty_context(path);
            }
        }
        let entry = entries.entry(path.into()).or_insert_with(|| Entry {
            checked: Instant::now() - Duration::from_secs(3),
            pending: false,
            value: empty_context(path),
        });
        if !entry.pending && entry.checked.elapsed() >= Duration::from_secs(2) {
            entry.pending = self.requests.try_send(path.into()).is_ok();
        }
        entry.value.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_groups_migrate_once_without_merging_new_containers() {
        let mut state: State = serde_json::from_value(serde_json::json!({
            "revision":0,"panes":[],"workspaces":[
                {"id":"root","name":"repo","path":"/repo","tabs":[],"repository":"/repo/.git","main_path":"/repo"},
                {"id":"child","name":"feature","path":"/wt/feature","tabs":[],"repository":"/repo/.git","main_path":"/repo"},
                {"id":"orphan","name":"other","path":"/wt/other","tabs":[],"repository":"/other/.git","main_path":"/other"}
            ]
        })).unwrap();
        migrate(&mut state);
        assert_eq!(state.workspace_model, 1);
        assert_eq!(state.workspaces[1].parent.as_deref(), Some("root"));
        let orphan_owner = state.workspaces[2].parent.clone().unwrap();
        assert_eq!(
            state
                .workspaces
                .iter()
                .find(|w| w.id == orphan_owner)
                .unwrap()
                .path,
            "/other"
        );
        state.workspaces[1].parent = None;
        migrate(&mut state);
        assert!(
            state.workspaces[1].parent.is_none(),
            "explicit containers must not regroup on restart"
        );
        assert_eq!(state.workspaces.len(), 4);
    }
}
