//! Owned OSC 7501 records. A prompt boundary ends the previous program's lifetime.
use crate::StatusRecord;
use libghostty_vt::effect::{ProgramStatus, ProgramStatusState};
use std::collections::{BTreeMap, VecDeque};
#[derive(Default)]
pub struct Status {
    records: BTreeMap<String, StatusRecord>,
    order: VecDeque<String>,
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
    pub fn end(&mut self) {
        self.records
            .retain(|_, r| ["done", "error"].contains(&r.state.as_str()));
    }
    pub fn acknowledge(&mut self) {
        self.records
            .retain(|_, r| !["done", "error"].contains(&r.state.as_str()));
    }
    pub fn set(&mut self, state: String) {
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
