//! Owned OSC 7501 records. A prompt boundary ends the previous program's lifetime.
use crate::StatusRecord;
use libghostty_vt::effect::{ProgramStatus, ProgramStatusState};
use std::collections::{BTreeMap, VecDeque};
#[derive(Default)]
pub struct Status {
    records: BTreeMap<String, StatusRecord>,
    order: VecDeque<String>,
    provider: Option<(String, crate::agents::Agent)>,
    foreground: Option<i32>,
    shell: Option<i32>,
    provider_group: Option<i32>,
    returned_to_shell: bool,
    session: Option<String>,
    progress: Vec<crate::ProgressStep>,
    progress_group: Option<i32>,
}
impl Status {
    pub fn report(&mut self, r: ProgramStatus<'_>) {
        let Ok(state) = r.state() else { return };
        let id = String::from_utf8_lossy(r.id()).into_owned();
        if state == ProgramStatusState::Clear {
            self.records
                .retain(|k, _| !id.is_empty() && k != &id && !k.starts_with(&format!("{id}/")));
            return;
        }
        // Final output can arrive after tcgetpgrp has already returned to the
        // shell. It must not resurrect the program whose lifetime just ended.
        let terminal_state = format!("{state:?}").to_lowercase();
        let stale_completion =
            self.returned_to_shell && ["done", "error"].contains(&terminal_state.as_str());
        if !stale_completion
            && let Some(provider) =
                crate::agents::Agent::from_program(&String::from_utf8_lossy(r.app()))
        {
            let may_replace = self.provider.as_ref().is_none_or(|(owner, _)| {
                owner == &id || owner.starts_with(&format!("{id}/")) || id.is_empty()
            });
            if may_replace {
                if self
                    .provider
                    .as_ref()
                    .is_none_or(|(_, old)| *old != provider)
                {
                    if self.provider.is_some() {
                        self.clear_progress();
                    }
                    self.records.clear();
                    self.order.clear();
                }
                self.returned_to_shell = false;
                self.provider = Some((id.clone(), provider));
                self.provider_group = self.foreground;
            }
        }
        if matches!(terminal_state.as_str(), "done" | "error")
            && self
                .provider
                .as_ref()
                .is_some_and(|(owner, _)| owner == &id)
        {
            self.clear_progress();
        }
        self.order
            .retain(|k| self.records.contains_key(k) && k != &id);
        if self.records.len() >= 128
            && !self.records.contains_key(&id)
            && let Some(old) = self.order.pop_front()
        {
            self.records.remove(&old);
        }
        self.order.push_back(id.clone());
        self.records.insert(
            id.clone(),
            StatusRecord {
                id,
                state: format!("{state:?}").to_lowercase(),
                app: String::from_utf8_lossy(r.app()).into_owned(),
                title: String::from_utf8_lossy(r.title()).into_owned(),
                message: String::from_utf8_lossy(r.message()).into_owned(),
                kind: r
                    .kind()
                    .map(|k| format!("{k:?}").to_lowercase())
                    .unwrap_or_default(),
                progress: r.progress(),
            },
        );
    }
    pub fn provider(&self) -> Option<crate::agents::Agent> {
        self.provider.as_ref().map(|(_, p)| *p)
    }
    /// Foreground return is a fallback when a shell has no semantic prompt integration.
    pub fn foreground(&mut self, group: Option<i32>, shell: Option<i32>) -> bool {
        self.foreground = group;
        self.shell = shell;
        if group.is_some() && group != shell {
            self.returned_to_shell = false;
        }
        if group.is_some()
            && group == shell
            && [self.provider_group, self.progress_group]
                .into_iter()
                .flatten()
                .any(|group| Some(group) != shell)
        {
            self.end();
            self.returned_to_shell = true;
            return true;
        }
        false
    }
    pub fn prompt(&mut self) {
        // An application's own input prompt is not a return to the shell.
        if self.foreground.is_none() || self.foreground == self.shell {
            self.end();
        }
    }
    pub fn enter_session(&mut self, session: &str) {
        if self.session.as_deref() == Some(session) {
            return;
        }
        if self.session.replace(session.to_owned()).is_some() {
            self.clear_progress();
        }
    }
    pub fn set_progress(&mut self, steps: Vec<crate::ProgressStep>) -> anyhow::Result<()> {
        crate::ProgressStep::validate(&steps)?;
        self.progress_group = (!steps.is_empty()).then_some(self.foreground).flatten();
        self.progress = steps;
        Ok(())
    }
    fn clear_progress(&mut self) {
        self.progress.clear();
        self.progress_group = None;
    }
    pub fn update_pane(&self, pane: &mut crate::Pane) {
        pane.status = self.state();
        pane.reports = self.records();
        if pane.progress != self.progress {
            pane.progress.clone_from(&self.progress);
        }
    }
    pub fn end(&mut self) {
        self.clear_progress();
        self.session = None;
        self.provider = None;
        self.provider_group = None;
        self.records
            .retain(|_, r| ["done", "error"].contains(&r.state.as_str()));
    }
    pub fn acknowledge(&mut self) {
        self.records
            .retain(|_, r| !["done", "error"].contains(&r.state.as_str()));
    }
    pub fn set(&mut self, state: String) {
        if matches!(state.as_str(), "idle" | "done" | "error") {
            self.clear_progress();
        }
        self.order.retain(|k| !k.is_empty());
        self.order.push_back(String::new());
        self.records.insert(
            String::new(),
            StatusRecord {
                state,
                ..Default::default()
            },
        );
    }
    pub fn records(&self) -> Vec<StatusRecord> {
        self.records
            .values()
            .cloned()
            .map(|mut r| {
                if r.app.is_empty() {
                    let mut ancestor = r.id.as_str();
                    loop {
                        ancestor = ancestor.rsplit_once('/').map(|(p, _)| p).unwrap_or("");
                        if let Some(p) = self.records.get(ancestor)
                            && !p.app.is_empty()
                        {
                            r.app = p.app.clone();
                            break;
                        }
                        if ancestor.is_empty() {
                            break;
                        }
                    }
                }
                r
            })
            .collect()
    }
    pub fn state(&self) -> String {
        ["error", "blocked", "working", "done", "idle"]
            .into_iter()
            .find(|s| self.records.values().any(|r| r.state == *s))
            .unwrap_or("unknown")
            .into()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};
    #[test]
    fn progress_belongs_to_one_conversation() {
        let mut status = Status::default();
        status.enter_session("one");
        status
            .set_progress(vec![crate::ProgressStep {
                label: "Build".into(),
                percent: 40,
            }])
            .unwrap();
        status.enter_session("one");
        assert_eq!(status.progress[0].percent, 40);
        status.enter_session("two");
        assert!(status.progress.is_empty());
    }
    #[test]
    fn identity_is_separate_from_activity_and_nested_tool_reports() {
        let status = Arc::new(Mutex::new(Status::default()));
        let mut terminal = crate::vt::terminal(20, 4).unwrap();
        let observed = status.clone();
        terminal
            .on_program_status(move |_, r| observed.lock().unwrap().report(r))
            .unwrap();
        status.lock().unwrap().foreground(Some(20), Some(10));
        terminal.vt_write(b"\x1b]7501;state=done:app=codex\x07");
        assert_eq!(
            status.lock().unwrap().provider(),
            Some(crate::agents::Agent::Codex)
        );
        status.lock().unwrap().acknowledge();
        assert_eq!(status.lock().unwrap().state(), "unknown");
        assert!(status.lock().unwrap().provider().is_some());
        terminal.vt_write(b"\x1b]7501;state=clear\x07");
        assert!(status.lock().unwrap().provider().is_some());
        terminal.vt_write(b"\x1b]7501;state=working:id=tool:app=claude-code\x07");
        assert_eq!(
            status.lock().unwrap().provider(),
            Some(crate::agents::Agent::Codex)
        );
        status.lock().unwrap().prompt();
        assert!(status.lock().unwrap().provider().is_some());
        assert!(status.lock().unwrap().foreground(Some(10), Some(10)));
        assert!(status.lock().unwrap().provider().is_none());
        terminal.vt_write(b"\x1b]7501;state=done:app=codex\x07");
        assert!(
            status.lock().unwrap().provider().is_none(),
            "Late completion must not revive an exited agent"
        );
        terminal.vt_write(b"\x1b]7501;state=idle:app=claude-code\x07");
        assert_eq!(
            status.lock().unwrap().provider(),
            Some(crate::agents::Agent::Claude)
        );
        assert_eq!(status.lock().unwrap().state(), "idle");
        let resolved =
            crate::agents::Agent::resolve(Some("codex"), status.lock().unwrap().provider())
                .unwrap();
        assert_eq!(
            resolved,
            (crate::agents::Agent::Codex, crate::AgentSource::Registered)
        );
        assert!(crate::agents::Agent::from_program("build-codex-report").is_none());
    }
    #[test]
    fn hierarchical_clear_and_prompt_lifetime() {
        let status = Arc::new(Mutex::new(Status::default()));
        let mut t = crate::vt::terminal(20, 4).unwrap();
        let s = status.clone();
        t.on_program_status(move |_, r| s.lock().unwrap().report(r))
            .unwrap();
        t.vt_write(b"\x1b]7501;state=working:id=build:app=cargo\x07\x1b]7501;state=blocked:id=build/test:kind=permission\x07");
        assert_eq!(status.lock().unwrap().state(), "blocked");
        assert_eq!(status.lock().unwrap().records().len(), 2);
        assert_eq!(status.lock().unwrap().records()[1].app, "cargo");
        t.vt_write(b"\x1b]7501;state=clear:id=build\x07");
        assert!(status.lock().unwrap().records().is_empty());
        t.vt_write(b"\x1b]7501;state=done:id=completed\x07\x1b]7501;state=working:id=busy\x07");
        status.lock().unwrap().end();
        assert_eq!(status.lock().unwrap().state(), "done");
        status.lock().unwrap().acknowledge();
        assert_eq!(status.lock().unwrap().state(), "unknown");
        for n in 0..200 {
            t.vt_write(format!("\x1b]7501;state=working:id={n}\x07").as_bytes());
        }
        let records = status.lock().unwrap().records();
        assert_eq!(records.len(), 128);
        assert!(!records.iter().any(|r| r.id == "0"));
    }
}
