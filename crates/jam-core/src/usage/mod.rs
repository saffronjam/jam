//! Host-local account usage for agents descending from Jam-owned shells.
mod credentials;
mod process;
mod providers;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, LazyLock, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Codex,
    Claude,
}
impl Provider {
    fn name(self) -> &'static str {
        match self {
            Self::Codex => "codex",
            Self::Claude => "claude",
        }
    }
}

// Deliberately neither serializable nor printable: environment tokens remain local.
#[derive(Clone)]
struct Profile {
    key: String,
    provider: Provider,
    directory: PathBuf,
    #[cfg(target_os = "macos")]
    home: PathBuf,
    token: Option<String>,
    api_key: bool,
}
impl Profile {
    fn new(
        provider: Provider,
        directory: PathBuf,
        _home: PathBuf,
        token: Option<String>,
        api_key: bool,
    ) -> Self {
        let key = digest(&format!(
            "{}:{}:{}",
            provider.name(),
            directory.display(),
            token.as_deref().unwrap_or("")
        ));
        Self {
            key,
            provider,
            directory,
            #[cfg(target_os = "macos")]
            home: _home,
            token,
            api_key,
        }
    }
}
fn digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}
fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Window {
    pub key: String,
    pub label: String,
    pub percent: Option<f64>,
    pub resets_at: Option<i64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Extra {
    pub label: String,
    pub unit: String,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub balance: Option<f64>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Report {
    pub provider: Provider,
    pub profile_key: String,
    pub account_key: Option<String>,
    pub account_label: Option<String>,
    pub active_sessions: u32,
    pub windows: Vec<Window>,
    pub extra: Option<Extra>,
    pub checking: bool,
    pub checked_at: Option<i64>,
    pub last_success_at: Option<i64>,
    pub error: Option<String>,
}
impl Report {
    fn new(profile: &Profile, count: u32) -> Self {
        Self {
            provider: profile.provider,
            profile_key: profile.key.clone(),
            account_key: None,
            account_label: None,
            active_sessions: count,
            windows: Vec::new(),
            extra: None,
            checking: true,
            checked_at: None,
            last_success_at: None,
            error: None,
        }
    }
}

struct Entry {
    profile: Profile,
    report: Report,
    attempted: Option<Instant>,
    pending: bool,
    cancelled: Arc<AtomicBool>,
}
#[derive(Default)]
struct State {
    entries: BTreeMap<String, Entry>,
}
struct Job {
    profile: Profile,
    allow_prompt: bool,
    cancelled: Arc<AtomicBool>,
}
impl State {
    fn reconcile(&mut self, profiles: Vec<(Profile, u32)>, jobs: &crossbeam_channel::Sender<Job>) {
        self.entries.retain(|key, entry| {
            let active = profiles.iter().any(|(p, _)| &p.key == key);
            if !active {
                entry.cancelled.store(true, Ordering::Relaxed);
            }
            active
        });
        for (profile, count) in profiles.into_iter().take(64) {
            let entry = self
                .entries
                .entry(profile.key.clone())
                .or_insert_with(|| Entry {
                    profile: profile.clone(),
                    report: Report::new(&profile, count),
                    attempted: None,
                    pending: false,
                    cancelled: Arc::new(AtomicBool::new(false)),
                });
            entry.report.active_sessions = count;
            if !entry.pending
                && entry
                    .attempted
                    .is_none_or(|at| at.elapsed() >= Duration::from_secs(60))
                && jobs
                    .try_send(Job {
                        profile,
                        allow_prompt: false,
                        cancelled: entry.cancelled.clone(),
                    })
                    .is_ok()
            {
                entry.pending = true;
                entry.report.checking = true;
                entry.attempted = Some(Instant::now());
            }
        }
    }
    fn apply(&mut self, job: &Job, mut report: Report) {
        if job.cancelled.load(Ordering::Relaxed) {
            return;
        }
        let Some(entry) = self.entries.get_mut(&job.profile.key) else {
            return;
        };
        report.checking = false;
        report.active_sessions = entry.report.active_sessions;
        if report.error.is_none() {
            report.last_success_at = report.checked_at;
        } else {
            let changed_account = report.account_key.is_some()
                && entry.report.account_key.is_some()
                && report.account_key != entry.report.account_key;
            if !changed_account {
                // Keep the last known values stale, never relabel another account's quota.
                report.windows = entry.report.windows.clone();
                report.extra = entry.report.extra.clone();
                report.last_success_at = entry.report.last_success_at;
                report.account_label = entry.report.account_label.clone().or(report.account_label);
                report.account_key = report
                    .account_key
                    .or_else(|| entry.report.account_key.clone());
            } else {
                report.windows.clear();
                report.extra = None;
                report.last_success_at = None;
            }
        }
        entry.report = report;
        entry.pending = false;
    }
}

struct Monitor {
    roots: Arc<Mutex<Vec<u32>>>,
    state: Arc<Mutex<State>>,
    wake: crossbeam_channel::Sender<()>,
    last_requested: Arc<Mutex<Instant>>,
    jobs: crossbeam_channel::Sender<Job>,
}
impl Monitor {
    fn new() -> Self {
        let roots = Arc::new(Mutex::new(Vec::new()));
        let state = Arc::new(Mutex::new(State::default()));
        let (jobs, queue) = crossbeam_channel::bounded::<Job>(64);
        for _ in 0..2 {
            let (queue, state) = (queue.clone(), state.clone());
            std::thread::spawn(move || {
                for job in queue {
                    if job.cancelled.load(Ordering::Relaxed)
                        || !crate::config::agent_enabled(job.profile.provider.name())
                    {
                        continue;
                    }
                    let report = providers::fetch(&job.profile, job.allow_prompt);
                    state.lock().unwrap().apply(&job, report);
                }
            });
        }
        let last_requested = Arc::new(Mutex::new(Instant::now()));
        let watched = last_requested.clone();
        let (wake, signal) = crossbeam_channel::bounded(1);
        let (observed, results) = (roots.clone(), state.clone());
        let background_jobs = jobs.clone();
        std::thread::spawn(move || {
            loop {
                let _ = signal.recv_timeout(Duration::from_secs(3));
                if watched.lock().unwrap().elapsed() > Duration::from_secs(15) {
                    continue;
                }
                let roots = observed.lock().unwrap().clone();
                let profiles = process::discover(&roots)
                    .into_iter()
                    .filter(|(p, _)| crate::config::agent_enabled(p.provider.name()))
                    .collect();
                results
                    .lock()
                    .unwrap()
                    .reconcile(profiles, &background_jobs);
            }
        });
        Self {
            roots,
            state,
            wake,
            last_requested,
            jobs,
        }
    }
    fn authorize(&self, key: &str) -> anyhow::Result<()> {
        let mut state = self.state.lock().unwrap();
        let entry = state
            .entries
            .get_mut(key)
            .ok_or_else(|| anyhow::anyhow!("This agent is no longer running"))?;
        if !crate::config::agent_enabled(entry.profile.provider.name()) {
            anyhow::bail!("Agent integration is disabled in Settings")
        }
        if entry.pending {
            anyhow::bail!("Usage is already refreshing");
        }
        if entry.profile.provider != Provider::Claude {
            anyhow::bail!("This provider does not use the Keychain");
        }
        self.jobs.try_send(Job {
            profile: entry.profile.clone(),
            allow_prompt: true,
            cancelled: entry.cancelled.clone(),
        })?;
        entry.pending = true;
        entry.report.checking = true;
        entry.attempted = Some(Instant::now());
        Ok(())
    }
    fn snapshot(&self, mut roots: Vec<u32>) -> Vec<Report> {
        *self.last_requested.lock().unwrap() = Instant::now();
        roots.sort_unstable();
        let mut previous = self.roots.lock().unwrap();
        if *previous != roots {
            *previous = roots;
            let _ = self.wake.try_send(());
        }
        self.state
            .lock()
            .unwrap()
            .entries
            .values()
            .filter(|e| crate::config::agent_enabled(e.profile.provider.name()))
            .map(|e| e.report.clone())
            .collect()
    }
}
static MONITOR: LazyLock<Monitor> = LazyLock::new(Monitor::new);
pub fn snapshot(roots: Vec<u32>) -> Vec<Report> {
    MONITOR.snapshot(roots)
}

pub fn authorize(profile: &str) -> anyhow::Result<()> {
    MONITOR.authorize(profile)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn polling_is_per_profile_and_late_results_cannot_resurrect_closed_agents() {
        let profile = Profile::new(
            Provider::Codex,
            "/unused".into(),
            "/unused".into(),
            None,
            false,
        );
        let (tx, rx) = crossbeam_channel::bounded(2);
        let mut state = State::default();
        state.reconcile(vec![(profile.clone(), 4)], &tx);
        let job = rx.try_recv().unwrap();
        assert!(!job.allow_prompt, "Background checks must never prompt");
        state.reconcile(vec![(profile.clone(), 5)], &tx);
        assert!(rx.try_recv().is_err());
        let mut report = Report::new(&profile, 0);
        report.checked_at = Some(100);
        report.account_key = Some("first-account".into());
        report.windows.push(Window {
            key: "five_hour".into(),
            label: "5h".into(),
            percent: Some(25.0),
            resets_at: None,
        });
        state.apply(&job, report);
        assert_eq!(state.entries[&profile.key].report.active_sessions, 5);
        assert_eq!(
            state.entries[&profile.key].report.last_success_at,
            Some(100)
        );
        let mut failed = Report::new(&profile, 0);
        failed.error = Some("Unavailable".into());
        state.apply(&job, failed);
        assert_eq!(
            state.entries[&profile.key].report.windows[0].percent,
            Some(25.0)
        );
        let mut switched = Report::new(&profile, 0);
        switched.account_key = Some("second-account".into());
        switched.account_label = Some("new@example.invalid".into());
        switched.error = Some("Sign in again".into());
        state.apply(&job, switched);
        let current = &state.entries[&profile.key].report;
        assert!(current.windows.is_empty());
        assert!(current.last_success_at.is_none());
        assert_eq!(current.account_key.as_deref(), Some("second-account"));
        state.reconcile(Vec::new(), &tx);
        state.apply(&job, Report::new(&profile, 0));
        assert!(state.entries.is_empty());
    }

    #[test]
    fn only_explicit_authorization_can_queue_a_keychain_prompt() {
        let (jobs, queue) = crossbeam_channel::bounded(4);
        let (wake, _) = crossbeam_channel::bounded(1);
        let monitor = Monitor {
            roots: Arc::default(),
            state: Arc::default(),
            wake,
            last_requested: Arc::new(Mutex::new(Instant::now())),
            jobs: jobs.clone(),
        };
        let profile = Profile::new(
            Provider::Claude,
            "/unused".into(),
            "/unused".into(),
            None,
            false,
        );
        monitor
            .state
            .lock()
            .unwrap()
            .reconcile(vec![(profile.clone(), 1)], &jobs);
        let background = queue.try_recv().unwrap();
        assert!(!background.allow_prompt);
        assert!(
            monitor.authorize(&profile.key).is_err(),
            "Coalesce pending requests"
        );
        monitor
            .state
            .lock()
            .unwrap()
            .apply(&background, Report::new(&profile, 1));
        monitor.authorize(&profile.key).unwrap();
        assert!(queue.try_recv().unwrap().allow_prompt);
        assert!(monitor.authorize("not-an-active-profile").is_err());
    }
}
