use super::Report;
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
};

pub(super) const REFRESH_INTERVAL: i64 = 5 * 60;
const MAX_AGE: i64 = 7 * 24 * 60 * 60;
const MAX_PROFILES: usize = 64;
const MAX_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone, Deserialize, Serialize)]
pub(super) struct Reading {
    pub credential: String,
    pub report: Report,
    pub retry_at: i64,
    pub rate_limits: u32,
}

impl Reading {
    pub fn reusable_at(&self, now: i64) -> bool {
        self.report.checked_at.is_some_and(|at| {
            at <= now && (now.saturating_sub(at) <= MAX_AGE || self.retry_at > now)
        })
    }

    fn has_recent_success(&self, now: i64) -> bool {
        self.report
            .last_success_at
            .is_some_and(|at| at <= now && now.saturating_sub(at) <= MAX_AGE)
    }

    pub fn expire_success(&self, report: &mut Report, now: i64) {
        if !self.has_recent_success(now) {
            report.windows.clear();
            report.extra = None;
            report.last_success_at = None;
        }
    }

    pub fn keep_last_success(&self, report: &mut Report, now: i64) {
        if self.has_recent_success(now) {
            report.windows.clone_from(&self.report.windows);
            report.extra.clone_from(&self.report.extra);
            report.last_success_at = self.report.last_success_at;
        }
    }
}

pub(super) struct Cache {
    path: PathBuf,
    readings: BTreeMap<String, Reading>,
}

impl Cache {
    pub fn open(path: PathBuf) -> Self {
        let readings = (|| -> Option<_> {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
                .open(&path)
                .ok()?;
            let metadata = file.metadata().ok()?;
            if !metadata.is_file()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
                || metadata.len() > MAX_BYTES
            {
                return None;
            }
            let mut bytes = Vec::new();
            file.take(MAX_BYTES + 1).read_to_end(&mut bytes).ok()?;
            if bytes.len() as u64 > MAX_BYTES {
                return None;
            }
            serde_json::from_slice(&bytes).ok()
        })()
        .unwrap_or_default();
        Self { path, readings }
    }

    pub fn get(&self, profile: &str, credential: &str, now: i64) -> Option<Reading> {
        self.readings
            .get(profile)
            .filter(|entry| entry.credential == credential && entry.reusable_at(now))
            .cloned()
    }

    pub fn store(&mut self, entry: Reading, now: i64) -> anyhow::Result<()> {
        self.readings
            .insert(entry.report.profile_key.clone(), entry);
        self.readings.retain(|_, entry| entry.reusable_at(now));
        while self.readings.len() > MAX_PROFILES {
            let oldest = self
                .readings
                .iter()
                .min_by_key(|(_, entry)| entry.report.checked_at)
                .map(|(profile, _)| profile.clone())
                .unwrap();
            self.readings.remove(&oldest);
        }
        let bytes = serde_json::to_vec(&self.readings)?;
        anyhow::ensure!(
            bytes.len() as u64 <= MAX_BYTES,
            "Usage cache exceeds its size limit"
        );
        let directory = self.path.parent().unwrap();
        crate::server::private_dir(directory)?;
        let temporary = directory.join(format!(".usage-cache-{}.tmp", crate::id()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)?;
        let result = (|| -> std::io::Result<()> {
            file.write_all(&bytes)?;
            file.sync_all()?;
            fs::rename(&temporary, &self.path)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(temporary);
        }
        result.map_err(Into::into)
    }
}
