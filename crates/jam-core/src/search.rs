//! Cancellable history search. Snapshot decoding and scanning never run on the UI thread.
use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, bounded};
use libghostty_vt::{
    Terminal,
    point::{Coordinate, Point},
    snapshot::Decoder,
};
use serde::Serialize;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    pub start: usize,
    pub end: usize,
}
#[derive(Clone, Default, Serialize)]
pub struct Status {
    pub id: u64,
    pub count: usize,
    pub busy: bool,
    pub revision: u64,
    pub snapshot_revision: u64,
    pub stale: bool,
    pub error: Option<String>,
}
#[derive(Default)]
struct Results {
    status: Status,
    hits: Arc<Vec<Hit>>,
}
struct Job {
    id: u64,
    bytes: Vec<u8>,
    query: String,
    revision: u64,
}
#[derive(Clone)]
pub struct Search {
    epoch: Arc<AtomicU64>,
    results: Arc<Mutex<Results>>,
    tx: Sender<Job>,
    old: Receiver<Job>,
}
impl Default for Search {
    fn default() -> Self {
        Self::new()
    }
}
impl Search {
    pub fn new() -> Self {
        let (tx, rx) = bounded::<Job>(1);
        let result = Self {
            epoch: Arc::new(AtomicU64::new(0)),
            results: Arc::new(Mutex::new(Results::default())),
            tx,
            old: rx.clone(),
        };
        let epoch = result.epoch.clone();
        let shared = result.results.clone();
        std::thread::spawn(move || {
            while let Ok(job) = rx.recv() {
                if epoch.load(Ordering::Relaxed) != job.id {
                    continue;
                }
                let found = (|| -> Result<Option<Vec<Hit>>> {
                    let term = Decoder::new_buf(&job.bytes)?.decode()?;
                    scan(&term, &job.query, || {
                        epoch.load(Ordering::Relaxed) != job.id
                    })
                })();
                let mut r = shared.lock().unwrap();
                if epoch.load(Ordering::Relaxed) != job.id || r.status.id != job.id {
                    continue;
                }
                r.status.busy = false;
                r.status.snapshot_revision = job.revision;
                match found {
                    Ok(Some(hits)) => {
                        r.status.count = hits.len();
                        r.hits = Arc::new(hits);
                    }
                    Ok(None) => {}
                    Err(e) => r.status.error = Some(e.to_string()),
                }
            }
        });
        result
    }
    pub fn begin(&self, id: u64) {
        self.epoch.store(id, Ordering::Relaxed);
        *self.results.lock().unwrap() = Results {
            status: Status {
                id,
                busy: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let _ = self.old.try_recv();
    }
    pub fn active(&self, id: u64) -> bool {
        self.epoch.load(Ordering::Relaxed) == id
    }
    pub fn submit(&self, id: u64, bytes: Vec<u8>, query: String, revision: u64) {
        if !self.active(id) {
            return;
        }
        let _ = self.tx.try_send(Job {
            id,
            bytes,
            query,
            revision,
        });
    }
    pub fn fail(&self, id: u64, error: String) {
        let mut r = self.results.lock().unwrap();
        if r.status.id == id {
            r.status.busy = false;
            r.status.error = Some(error);
        }
    }
    pub fn cancel(&self) {
        self.epoch.fetch_add(1, Ordering::Relaxed);
        let _ = self.old.try_recv();
        *self.results.lock().unwrap() = Results::default();
    }
    pub fn status(&self) -> Status {
        self.results.lock().unwrap().status.clone()
    }
    pub fn revision(&self, revision: u64) {
        self.results.lock().unwrap().status.revision = revision;
    }
    pub fn stale(&self) {
        self.results.lock().unwrap().status.stale = true;
    }
    pub fn hit(&self, id: u64, index: usize) -> Option<Hit> {
        let r = self.results.lock().unwrap();
        if r.status.id == id && !r.status.busy {
            r.hits.get(index).copied()
        } else {
            None
        }
    }
}

pub fn cell_text(t: &Terminal<'_, '_>, x: u16, y: u32) -> Result<(String, usize)> {
    let cell = t.grid_ref(Point::Screen(Coordinate { x, y }))?;
    let wide = cell.cell()?.wide()? as usize;
    if wide >= 2 {
        return Ok((String::new(), 0));
    }
    let mut chars = ['\0'; 8];
    let text = match cell.graphemes(&mut chars) {
        Err(libghostty_vt::Error::OutOfSpace { required }) => {
            let mut extended = vec!['\0'; required];
            let len = cell.graphemes(&mut extended)?;
            extended[..len].iter().collect()
        }
        Ok(0) => " ".into(),
        Ok(len) => chars[..len].iter().collect(),
        Err(e) => return Err(e.into()),
    };
    Ok((text, if wide == 1 { 2 } else { 1 }))
}

pub fn scan(
    t: &Terminal<'_, '_>,
    query: &str,
    cancelled: impl Fn() -> bool,
) -> Result<Option<Vec<Hit>>> {
    let query = query.to_lowercase();
    if query.is_empty() {
        return Ok(Some(Vec::new()));
    }
    let cols = t.cols()?;
    let rows = t.total_rows()?;
    let mut hits = Vec::new();
    let mut line = String::new();
    // UTF-8 byte boundaries map back to terminal cells, including combining and wide glyphs.
    let mut positions = Vec::<(usize, usize, usize)>::new();
    for y in 0..rows {
        if cancelled() {
            return Ok(None);
        }
        for x in 0..cols {
            if x % 256 == 0 && cancelled() {
                return Ok(None);
            }
            let (text, width) = cell_text(t, x, y as u32)?;
            if width == 0 {
                continue;
            }
            let start = y * cols as usize + x as usize;
            positions.push((line.len(), start, start + width - 1));
            line.push_str(&text.to_lowercase());
        }
        let wrapped = t
            .grid_ref(Point::Screen(Coordinate { x: 0, y: y as u32 }))?
            .row()?
            .is_wrapped()?;
        if !wrapped || y + 1 == rows {
            let haystack = line.trim_end();
            for (offset, _) in haystack.match_indices(&query) {
                if cancelled() {
                    return Ok(None);
                }
                let a = positions
                    .partition_point(|p| p.0 <= offset)
                    .saturating_sub(1);
                let b = positions
                    .partition_point(|p| p.0 < offset + query.len())
                    .saturating_sub(1);
                hits.push(Hit {
                    start: positions[a].1,
                    end: positions[b].2,
                });
            }
            line.clear();
            positions.clear();
        }
    }
    Ok(Some(hits))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn searches_scrollback_wrapped_lines_unicode_and_case() {
        let mut t = crate::vt::terminal(12, 2).unwrap();
        t.vt_write("old Needle\r\n1234567890界abc\r\nNEEDLE e\u{301}\r\nlast".as_bytes());
        assert_eq!(scan(&t, "needle", || false).unwrap().unwrap().len(), 2);
        let wide = scan(&t, "界abc", || false).unwrap().unwrap();
        assert_eq!(wide.len(), 1);
        assert_eq!(wide[0].end - wide[0].start, 4);
        assert_eq!(scan(&t, "e\u{301}", || false).unwrap().unwrap().len(), 1);
        assert!(scan(&t, "needle", || true).unwrap().is_none());
    }
    #[test]
    fn large_history_can_cancel_during_scan() {
        let mut t = crate::vt::terminal(100, 25).unwrap();
        for _ in 0..10_000 {
            t.vt_write(b"large history with one needle and some text\r\n");
        }
        let calls = std::cell::Cell::new(0);
        let started = std::time::Instant::now();
        assert!(
            scan(&t, "needle", || {
                let n = calls.get() + 1;
                calls.set(n);
                n > 128
            })
            .unwrap()
            .is_none()
        );
        eprintln!("cancelled large-history scan after {:?}", started.elapsed());
        let started = std::time::Instant::now();
        let hits = scan(&t, "needle", || false).unwrap().unwrap();
        assert!(hits.len() > 1000);
        eprintln!(
            "searched {} rows / {} matches in {:?}",
            t.total_rows().unwrap(),
            hits.len(),
            started.elapsed()
        );
    }
    #[test]
    fn superseded_search_cannot_publish() {
        let mut t = crate::vt::terminal(30, 2).unwrap();
        t.vt_write(b"needle");
        let search = Search::new();
        search.begin(10);
        search.submit(
            10,
            t.encode_snapshot_alloc(None).unwrap().unwrap().to_vec(),
            "needle".into(),
            1,
        );
        search.begin(11);
        search.submit(
            11,
            t.encode_snapshot_alloc(None).unwrap().unwrap().to_vec(),
            "absent".into(),
            1,
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
        while search.status().busy {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        assert_eq!(search.status().id, 11);
        assert_eq!(search.status().count, 0);
        search.cancel();
        assert_eq!(search.status().id, 0);
    }
}
