//! Each mirror is confined to its worker. Only owned frames cross into the UI.
use crate::{
    vt::{self, TerminalFrame},
    *,
};
use anyhow::{Result, bail};
use crossbeam_channel::{Sender, bounded};
use libghostty_vt::{Terminal, snapshot::Decoder, terminal::ScrollViewport};
use std::{
    net::Shutdown,
    os::unix::net::UnixStream,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
enum Event {
    Network(Message),
    Scroll(isize),
    Bottom,
    Find { id: u64, query: String },
    FindSelect { id: u64, index: usize },
    Request(Request),
    Stop,
}
// Capture the last completed frame at DEC 2026 boundaries, which can occur
// several times inside a single PTY read. The callback stays on the VT thread.
#[derive(Default)]
struct RenderHold {
    started: Option<Instant>,
    boundary: Option<TerminalFrame>,
}
type HeldFrame = std::rc::Rc<std::cell::RefCell<RenderHold>>;

fn observe_render_hold(term: &mut Terminal<'static, 'static>) -> Result<HeldFrame> {
    let hold = HeldFrame::default();
    if term.mode(libghostty_vt::mode::Mode::SYNC_OUTPUT)? {
        // An attachment snapshot may have been taken mid-redraw.
        hold.borrow_mut().started = Some(Instant::now());
    }
    let capture = hold.clone();
    term.on_render_hold(move |term, held| {
        if let Ok(mut state) = capture.try_borrow_mut() {
            state.started = held.then(Instant::now);
            state.boundary = if held { vt::frame(term).ok() } else { None };
        }
    })?;
    Ok(hold)
}

fn complete_frame(
    term: &mut Terminal<'static, 'static>,
    hold: &HeldFrame,
) -> Result<Option<TerminalFrame>> {
    let mut state = hold.borrow_mut();
    if let Some(start) = state.started {
        if start.elapsed() < Duration::from_secs(1) {
            return Ok(state.boundary.take());
        }
        term.set_mode(libghostty_vt::mode::Mode::SYNC_OUTPUT, false)?;
        *state = RenderHold::default();
    }
    Ok(Some(vt::frame(term)?))
}

#[derive(Default)]
pub struct Display {
    pub frame: Arc<TerminalFrame>,
    pub revision: u64,
    pub generation: u64,
    pub error: Option<String>,
    pub selection: Option<(usize, usize)>,
}
pub struct Mirror {
    tx: Sender<Event>,
    pub display: Arc<Mutex<Display>>,
    pub search: crate::search::Search,
    cancel: Arc<AtomicBool>,
    socket: Arc<Mutex<Option<UnixStream>>>,
    resize: Sender<Request>,
    resize_old: crossbeam_channel::Receiver<Request>,
}
impl Mirror {
    pub fn new(pane: String) -> Self {
        Self::at(pane, socket())
    }
    pub fn at(pane: String, path: std::path::PathBuf) -> Self {
        let (tx, rx) = bounded(256);
        let (resize, resize_rx) = bounded::<Request>(1);
        let drop_old = resize_rx.clone();
        let shared_resize = resize.clone();
        let resize_path = path.clone();
        let resize_errors = tx.clone();
        thread::spawn(move || {
            while let Ok(mut r) = resize_rx.recv() {
                while let Ok(new) = resize_rx.try_recv() {
                    r = new;
                }
                let result = (|| -> Result<()> {
                    let mut socket = connect_to(&resize_path)?;
                    write_request(&mut socket, &r)?;
                    if let Message::Error(e) = read_frame(&mut socket)? {
                        bail!(e)
                    }
                    Ok(())
                })();
                if let Err(e) = result {
                    let _ = resize_errors.send(Event::Network(Message::Error(e.to_string())));
                }
            }
        });

        let display = Arc::new(Mutex::new(Display::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let socket = Arc::new(Mutex::new(None));
        let request_path = path.clone();
        let output = tx.clone();
        let cancelled = cancel.clone();
        let connection = socket.clone();
        thread::spawn(move || {
            while !cancelled.load(Ordering::Relaxed) {
                let result = (|| -> Result<()> {
                    let mut s = connect_to(&path)?;
                    s.set_read_timeout(None)?;
                    *connection.lock().unwrap() = Some(s.try_clone()?);
                    write_request(&mut s, &Request::Attach { pane: pane.clone() })?;
                    while !cancelled.load(Ordering::Relaxed) {
                        let m = read_frame(&mut s)?;
                        if output.send(Event::Network(m)).is_err() {
                            return Ok(());
                        }
                    }
                    Ok(())
                })();
                if let Err(e) = result
                    && output
                        .send(Event::Network(Message::Error(
                            if e.downcast_ref::<std::io::Error>().is_some() {
                                "Reconnecting to terminal…".into()
                            } else {
                                e.to_string()
                            },
                        )))
                        .is_err()
                {
                    break;
                }
                for _ in 0..5 {
                    if cancelled.load(Ordering::Relaxed) {
                        return;
                    }
                    thread::sleep(Duration::from_millis(50));
                }
            }
        });
        let search = crate::search::Search::new();
        let finder = search.clone();
        let shared = display.clone();
        let resync = socket.clone();
        thread::spawn(move || {
            let (mut term, mut seq): (Option<Terminal<'static, 'static>>, u64) = (None, 0);
            let mut hold = HeldFrame::default();
            let mut anchor: Option<(
                u64,
                libghostty_vt::grid_ref::TrackedGridRef,
                u16,
                libghostty_vt::screen::Screen,
                String,
            )> = None;
            let (mut dirty, mut last) = (false, Instant::now());
            loop {
                match rx.recv_timeout(if dirty {
                    Duration::from_millis(8)
                } else if let Some(start) = hold.borrow().started {
                    Duration::from_secs(1).saturating_sub(start.elapsed())
                } else {
                    Duration::from_secs(60)
                }) {
                    Ok(Event::Stop) | Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                        break;
                    }
                    Ok(event) => {
                        let result = (|| -> Result<()> {
                            match event {
                                Event::Find { id, query } => {
                                    if finder.active(id) {
                                        let result = (|| -> Result<()> {
                                            let t = term.as_ref().ok_or_else(|| {
                                                anyhow::anyhow!("Terminal is connecting")
                                            })?;
                                            let origin = t.track_grid_ref(
                                                libghostty_vt::point::Point::Screen(
                                                    libghostty_vt::point::Coordinate { x: 0, y: 0 },
                                                ),
                                            )?;
                                            let bytes = t
                                                .encode_snapshot_alloc(None)?
                                                .ok_or_else(|| {
                                                    anyhow::anyhow!("Snapshot unavailable")
                                                })?
                                                .to_vec();
                                            anchor = Some((
                                                id,
                                                origin,
                                                t.cols()?,
                                                t.active_screen()?,
                                                query.clone(),
                                            ));
                                            finder.submit(id, bytes, query, seq);
                                            Ok(())
                                        })();
                                        if let Err(e) = result {
                                            finder.fail(id, e.to_string());
                                        }
                                    }
                                }
                                Event::FindSelect { id, index } => {
                                    if let (
                                        Some(t),
                                        Some((anchor_id, origin, cols, screen, query)),
                                        Some(hit),
                                    ) = (&mut term, &anchor, finder.hit(id, index))
                                    {
                                        if *anchor_id == id
                                            && *cols == t.cols()?
                                            && *screen == t.active_screen()?
                                        {
                                            if let Some(point) =
                                                origin.point(libghostty_vt::point::Space::Screen)?
                                            {
                                                let start =
                                                    hit.start + point.y as usize * *cols as usize;
                                                let end =
                                                    hit.end + point.y as usize * *cols as usize;
                                                let mut text = String::new();
                                                for cell in start..=end {
                                                    if cell / *cols as usize >= t.total_rows()? {
                                                        break;
                                                    }
                                                    text.push_str(
                                                        &crate::search::cell_text(
                                                            t,
                                                            (cell % *cols as usize) as u16,
                                                            (cell / *cols as usize) as u32,
                                                        )?
                                                        .0,
                                                    );
                                                }
                                                if text
                                                    .to_lowercase()
                                                    .contains(&query.to_lowercase())
                                                {
                                                    let row = start / *cols as usize;
                                                    t.scroll_viewport(ScrollViewport::Row(
                                                        row.saturating_sub(t.rows()? as usize / 3),
                                                    ));
                                                    let offset = t.scrollbar()?.offset as usize
                                                        * *cols as usize;
                                                    shared.lock().unwrap().selection = Some((
                                                        start.saturating_sub(offset),
                                                        end.saturating_sub(offset),
                                                    ));
                                                    dirty = true;
                                                } else {
                                                    finder.stale();
                                                }
                                            } else {
                                                finder.stale();
                                            }
                                        } else {
                                            finder.stale();
                                        }
                                    }
                                }
                                Event::Request(r) => {
                                    call_at(&request_path, r)?;
                                }
                                Event::Network(Message::Snapshot { seq: n, bytes, .. }) => {
                                    let mut snapshot = Decoder::new_buf(&bytes)?.decode()?;
                                    hold = observe_render_hold(&mut snapshot)?;
                                    term = Some(snapshot);
                                    seq = n;
                                    finder.revision(seq);
                                    shared.lock().unwrap().selection = None;
                                    {
                                        let mut d = shared.lock().unwrap();
                                        d.error = None;
                                        d.generation += 1;
                                    }
                                    dirty = true;
                                }
                                Event::Network(Message::Output { seq: n, bytes }) => {
                                    if n != seq + 1 {
                                        bail!("terminal output sequence gap")
                                    }
                                    if let Some(t) = &mut term {
                                        t.vt_write(&bytes);
                                    }
                                    seq = n;
                                    finder.revision(seq);
                                    dirty = true;
                                }
                                Event::Network(Message::Geometry { seq: n, cols, rows }) => {
                                    shared.lock().unwrap().selection = None;
                                    if n != seq + 1 {
                                        bail!("terminal geometry sequence gap")
                                    }
                                    if let Some(t) = &mut term {
                                        t.resize(cols, rows, 0, 0)?;
                                    }
                                    seq = n;
                                    finder.revision(seq);
                                    dirty = true;
                                }
                                Event::Network(Message::Error(e)) => {
                                    shared.lock().unwrap().error = Some(e);
                                }
                                Event::Bottom => {
                                    if let Some(t) = &mut term {
                                        t.scroll_viewport(ScrollViewport::Bottom);
                                        shared.lock().unwrap().selection = None;
                                        dirty = true;
                                    }
                                }
                                Event::Scroll(delta) => {
                                    shared.lock().unwrap().selection = None;
                                    if let Some(t) = &mut term {
                                        t.scroll_viewport(ScrollViewport::Delta(delta));
                                        dirty = true;
                                    }
                                }
                                _ => {}
                            }
                            Ok(())
                        })();
                        if let Err(e) = result {
                            shared.lock().unwrap().error = Some(e.to_string());
                            term = None;
                            hold = HeldFrame::default();
                            if let Some(s) = resync.lock().unwrap().as_ref() {
                                let _ = s.shutdown(Shutdown::Both);
                            }
                        }
                    }
                    Err(_) => {}
                }
                dirty |= hold
                    .borrow()
                    .started
                    .is_some_and(|at| at.elapsed() >= Duration::from_secs(1));
                if dirty && last.elapsed() >= Duration::from_millis(8) {
                    if let Some(t) = &mut term {
                        match complete_frame(t, &hold) {
                            Ok(None) => {}
                            Ok(Some(mut f)) => {
                                let mut d = shared.lock().unwrap();
                                f.selection = d.selection;
                                d.frame = Arc::new(f);
                                d.revision += 1;
                            }
                            Err(e) => shared.lock().unwrap().error = Some(e.to_string()),
                        }
                    }
                    dirty = false;
                    last = Instant::now();
                }
            }
        });
        Self {
            tx,
            display,
            search,
            cancel,
            socket,
            resize: shared_resize,
            resize_old: drop_old,
        }
    }
    pub fn request(&self, r: Request) -> bool {
        if matches!(r, Request::Resize { .. }) {
            match self.resize.try_send(r) {
                Ok(()) => true,
                Err(crossbeam_channel::TrySendError::Full(r)) => {
                    let _ = self.resize_old.try_recv();
                    self.resize.try_send(r).is_ok()
                }
                Err(_) => false,
            }
        } else {
            self.tx.try_send(Event::Request(r)).is_ok()
        }
    }
    pub fn find(&self, id: u64, query: String) {
        self.search.begin(id);
        if self.tx.try_send(Event::Find { id, query }).is_err() {
            self.search.fail(id, "Terminal is busy. Retrying…".into());
        }
    }
    pub fn find_select(&self, id: u64, index: usize) {
        if self.tx.try_send(Event::FindSelect { id, index }).is_err() {
            self.search.stale();
        }
    }
    pub fn bottom(&self) {
        let _ = self.tx.try_send(Event::Bottom);
    }
    pub fn scroll(&self, delta: isize) {
        let _ = self.tx.try_send(Event::Scroll(delta));
    }
    pub fn select(&self, index: usize, extend: bool) {
        let mut d = self.display.lock().unwrap();
        let index = index.min(d.frame.cells.len().saturating_sub(1));
        d.selection = Some(if extend {
            (d.selection.map(|s| s.0).unwrap_or(index), index)
        } else {
            (index, index)
        });
        let selection = d.selection;
        Arc::make_mut(&mut d.frame).selection = selection;
        d.revision += 1;
    }
    pub fn clear_selection(&self) {
        let mut d = self.display.lock().unwrap();
        d.selection = None;
        Arc::make_mut(&mut d.frame).selection = None;
        d.revision += 1;
    }
    pub fn selected_text(&self) -> String {
        let d = self.display.lock().unwrap();
        let f = &d.frame;
        let Some((a, b)) = f.selection else {
            return String::new();
        };
        let (mut text, mut line) = (String::new(), String::new());
        for i in a.min(b)..=a.max(b).min(f.cells.len().saturating_sub(1)) {
            if i > a.min(b) && i % f.cols.max(1) as usize == 0 {
                text.push_str(line.trim_end());
                text.push('\n');
                line.clear();
            }
            if let Some(c) = f.cells.get(i)
                && c.wide < 2
            {
                line.push_str(if c.text.is_empty() { " " } else { &c.text });
            }
        }
        text.push_str(line.trim_end());
        text
    }
}
impl Drop for Mirror {
    fn drop(&mut self) {
        self.search.cancel();
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(s) = self.socket.lock().unwrap().take() {
            let _ = s.shutdown(Shutdown::Both);
        }
        let _ = self.tx.try_send(Event::Stop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fragmented_synchronized_redraws_only_publish_complete_frames() {
        let mut term = vt::terminal(12, 2).unwrap();
        let hold = observe_render_hold(&mut term).unwrap();
        term.vt_write(b"first\x1b[?20");
        term.vt_write(b"26h\x1b[?25l\rsecond");
        let first = complete_frame(&mut term, &hold).unwrap().unwrap();
        assert!(first.cursor_visible && first.cells[0].text == "f");
        assert!(complete_frame(&mut term, &hold).unwrap().is_none());
        term.vt_write(b"\x1b[?25h\x1b[?2026l\x1b[?2026h\x1b[?25l\rthird");
        let second = complete_frame(&mut term, &hold).unwrap().unwrap();
        assert!(second.cursor_visible && second.cells[0].text == "s");
        term.vt_write(b"\x1b[?25h");
        hold.borrow_mut().started = Some(Instant::now() - Duration::from_secs(2));
        let third = complete_frame(&mut term, &hold).unwrap().unwrap();
        assert!(third.cursor_visible && third.cells[0].text == "t");
        assert!(!term.mode(libghostty_vt::mode::Mode::SYNC_OUTPUT).unwrap());
    }
    #[test]
    fn selection_is_immediately_available_and_clearable() {
        let (tx, _) = bounded(1);
        let (resize, resize_old) = bounded(1);
        let mut term = vt::terminal(20, 2).unwrap();
        term.vt_write("hello 界".as_bytes());
        let mirror = Mirror {
            tx,
            resize,
            resize_old,
            search: crate::search::Search::new(),
            display: Arc::new(Mutex::new(Display {
                frame: Arc::new(vt::frame(&term).unwrap()),
                ..Default::default()
            })),
            cancel: Arc::new(AtomicBool::new(false)),
            socket: Arc::new(Mutex::new(None)),
        };
        mirror.select(6, false);
        mirror.select(0, true);
        assert_eq!(mirror.selected_text(), "hello 界");
        mirror.clear_selection();
        assert_eq!(mirror.selected_text(), "");
        assert!(mirror.display.lock().unwrap().frame.selection.is_none());
    }
    #[test]
    fn find_jumps_through_history_without_sending_terminal_input() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("find.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            write_frame(&mut s, &Message::Hello(VERSION.into())).unwrap();
            assert!(matches!(
                *match read_frame::<Request>(&mut s).unwrap() {
                    Request::Client { request, .. } => request,
                    r => Box::new(r),
                },
                Request::Attach { .. }
            ));
            let mut t = vt::terminal(28, 4).unwrap();
            t.vt_write(b"needle first\r\n");
            for _ in 0..40 {
                t.vt_write(b"filler line\r\n");
            }
            t.vt_write(b"needle second\r\nneedle third");
            write_frame(
                &mut s,
                &Message::Snapshot {
                    seq: 1,
                    cols: 28,
                    rows: 4,
                    bytes: t.encode_snapshot_alloc(None).unwrap().unwrap().to_vec(),
                },
            )
            .unwrap();
            let mut byte = [0];
            assert_eq!(
                std::io::Read::read(&mut s, &mut byte).unwrap(),
                0,
                "find must never send PTY input"
            );
        });
        let mirror = Mirror::at("find".into(), path);
        let deadline = Instant::now() + Duration::from_secs(5);
        while mirror.display.lock().unwrap().generation == 0 {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        mirror.find(1, "needle".into());
        while mirror.search.status().busy {
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(mirror.search.status().count, 3);
        for index in 0..3 {
            mirror.clear_selection();
            mirror.find_select(1, index);
            loop {
                assert!(!mirror.search.status().stale, "hit {index} became stale");
                if mirror.selected_text() == "needle" {
                    break;
                }
                assert!(
                    Instant::now() < deadline,
                    "hit {index} not highlighted: {:?}",
                    mirror.selected_text()
                );
                thread::sleep(Duration::from_millis(5));
            }
        }
        mirror.search.cancel();
        assert!(!mirror.search.status().busy);
        drop(mirror);
        server.join().unwrap();
    }
    #[test]
    fn sequence_gap_forces_fresh_snapshot_and_continues() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mirror.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = thread::spawn(move || {
            let mut term = vt::terminal(30, 4).unwrap();
            term.vt_write(b"start");
            let attach = || {
                let (mut s, _) = listener.accept().unwrap();
                s.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
                write_frame(&mut s, &Message::Hello(VERSION.into())).unwrap();
                assert!(matches!(
                    *match read_frame::<Request>(&mut s).unwrap() {
                        Request::Client { request, .. } => request,
                        r => Box::new(r),
                    },
                    Request::Attach { .. }
                ));
                s
            };
            let mut s = attach();
            write_frame(
                &mut s,
                &Message::Snapshot {
                    seq: 3,
                    cols: 30,
                    rows: 4,
                    bytes: term.encode_snapshot_alloc(None).unwrap().unwrap().to_vec(),
                },
            )
            .unwrap();
            write_frame(
                &mut s,
                &Message::Output {
                    seq: 8,
                    bytes: b"bad gap".to_vec(),
                },
            )
            .unwrap();
            let mut b = [0];
            assert_eq!(
                std::io::Read::read(&mut s, &mut b).unwrap(),
                0,
                "gap must close the stream"
            );
            term.vt_write(b" recovered");
            let mut s = attach();
            write_frame(
                &mut s,
                &Message::Snapshot {
                    seq: 8,
                    cols: 30,
                    rows: 4,
                    bytes: term.encode_snapshot_alloc(None).unwrap().unwrap().to_vec(),
                },
            )
            .unwrap();
            write_frame(
                &mut s,
                &Message::Output {
                    seq: 9,
                    bytes: " 界".as_bytes().to_vec(),
                },
            )
            .unwrap();
            let _ = std::io::Read::read(&mut s, &mut b);
        });
        let mirror = Mirror::at("test".into(), path);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let d = mirror.display.lock().unwrap();
            let text = d
                .frame
                .cells
                .iter()
                .map(|c| c.text.as_str())
                .collect::<String>();
            if text.contains("start recovered 界") {
                assert!(d.error.is_none());
                assert_eq!(d.generation, 2);
                break;
            }
            assert!(
                Instant::now() < deadline,
                "mirror did not recover: {text:?} {:?}",
                d.error
            );
            drop(d);
            thread::sleep(Duration::from_millis(20));
        }
        drop(mirror);
        server.join().unwrap();
    }
}
