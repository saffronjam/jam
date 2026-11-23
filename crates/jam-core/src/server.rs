use crate::*;
use anyhow::{Context, Result, bail};
use crossbeam_channel::{Sender, bounded};
use fs2::FileExt;
use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use std::{
    collections::HashMap,
    fs::{self, OpenOptions},
    io::{Read, Write},
    os::unix::{
        fs::{MetadataExt, OpenOptionsExt, PermissionsExt},
        net::UnixListener,
    },
    sync::{Arc, Mutex},
    thread,
};

// Retain the session leader's PID until its process group has been stopped.
// Checking exit with try_wait would reap it first, permitting a recycled PGID
// to be signalled. The group is consumed before the child can ever be reaped.
struct Process {
    child: Box<dyn portable_pty::Child + Send + Sync>,
    owned_group: Option<libc::pid_t>,
}
impl Process {
    fn new(child: Box<dyn portable_pty::Child + Send + Sync>) -> Self {
        let group = child
            .process_id()
            .map(|pid| pid as libc::pid_t)
            .filter(|&pid| {
                pid > 1 && unsafe { libc::getpgid(pid) } == pid && pid != unsafe { libc::getpgrp() }
            });
        Self {
            child,
            owned_group: group,
        }
    }
    fn exited_without_reaping(&mut self) -> Result<bool> {
        let Some(pid) = self.child.process_id() else {
            return Ok(self.child.try_wait()?.is_some());
        };
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        if unsafe {
            libc::waitid(
                libc::P_PID,
                pid,
                &mut info,
                libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error()).with_context(|| {
                format!("Could not observe terminal child {pid} exit without reaping")
            });
        }
        #[cfg(target_os = "macos")]
        let exited = info.si_pid != 0;
        #[cfg(target_os = "linux")]
        let exited = unsafe { info.si_pid() } != 0;
        Ok(exited)
    }
    fn poll_exit(&mut self) -> Result<bool> {
        let exited = self.exited_without_reaping()?;
        if exited {
            self.terminate()?;
        }
        Ok(exited)
    }
    fn terminate(&mut self) -> Result<()> {
        if let Some(group) = self.owned_group.take()
            && unsafe { libc::killpg(group, libc::SIGKILL) } < 0
        {
            let error = std::io::Error::last_os_error();
            let harmless = error.raw_os_error() == Some(libc::ESRCH);
            #[cfg(target_os = "macos")]
            let harmless = if error.raw_os_error() == Some(libc::EPERM) {
                let inspected = (|| -> Result<bool> {
                    if !self.exited_without_reaping()? {
                        return Ok(false);
                    }
                    // XNU reports EPERM for zombie-only groups. A live or
                    // uninspectable member must retain the permission error.
                    group_has_no_live_members(group, self.child.process_id().unwrap() as i32)
                })();
                match inspected {
                    Ok(empty) => empty,
                    Err(error) => {
                        self.owned_group = Some(group);
                        return Err(error)
                            .context("Could not verify terminal process-group cleanup");
                    }
                }
            } else {
                harmless
            };
            if !harmless {
                self.owned_group = Some(group);
                return Err(error)
                    .with_context(|| format!("Could not stop terminal process group {group}"));
            }
        }
        if self.child.try_wait()?.is_none() {
            if let Some(pid) = self.child.process_id() {
                if unsafe { libc::kill(pid as i32, libc::SIGKILL) } < 0
                    && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
                {
                    return Err(std::io::Error::last_os_error())
                        .with_context(|| format!("Could not stop terminal child {pid}"));
                }
            } else {
                self.child
                    .kill()
                    .context("Could not stop terminal child without a PID")?;
            }
            self.child
                .wait()
                .context("Could not reap terminal child after stopping it")?;
        }
        Ok(())
    }
}
// Called only after WNOWAIT proved the unreaped root exited. The retained
// root PID prevents the group ID from being recycled during this inspection.
#[cfg(target_os = "macos")]
fn group_has_no_live_members(group: libc::pid_t, root: libc::pid_t) -> Result<bool> {
    const PROC_PGRP_ONLY: u32 = 2; // sys/proc_info.h
    let mut pids = vec![0i32; 64];
    loop {
        unsafe {
            *libc::__error() = 0;
        }
        let bytes = unsafe {
            libc::proc_listpids(
                PROC_PGRP_ONLY,
                group as u32,
                pids.as_mut_ptr().cast(),
                (pids.len() * std::mem::size_of::<i32>()) as i32,
            )
        };
        if bytes <= 0 {
            let error = std::io::Error::last_os_error();
            if bytes == 0 && matches!(error.raw_os_error(), Some(0) | Some(libc::ESRCH)) {
                return Ok(true);
            }
            return Err(error)
                .with_context(|| format!("Could not inspect terminal process group {group}"));
        }
        let count = bytes as usize / std::mem::size_of::<i32>();
        if count >= pids.len() {
            if pids.len() >= 1024 * 1024 {
                bail!("Terminal process group membership exceeds inspection limit");
            }
            pids.resize(pids.len() * 2, 0);
            continue;
        }
        for pid in pids[..count]
            .iter()
            .copied()
            .filter(|pid| *pid > 0 && *pid != root)
        {
            let mut info: libc::proc_bsdinfo = unsafe { std::mem::zeroed() };
            let size = std::mem::size_of_val(&info) as i32;
            let read = unsafe {
                libc::proc_pidinfo(
                    pid,
                    libc::PROC_PIDTBSDINFO,
                    0,
                    (&mut info as *mut libc::proc_bsdinfo).cast(),
                    size,
                )
            };
            if read != size {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() == Some(libc::ESRCH) {
                    continue;
                }
                return Err(error).with_context(|| {
                    format!("Could not inspect terminal process {pid} in group {group}")
                });
            }
            if info.pbi_pgid == group as u32 && info.pbi_status != libc::SZOMB {
                return Ok(false);
            }
        }
        return Ok(true);
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.terminate();
    }
}

#[derive(Clone)]
struct Handle {
    tx: Sender<Op>,
    info: Arc<Mutex<Pane>>,
    root_pid: Arc<std::sync::atomic::AtomicU32>,
}
enum Op {
    Output(Vec<u8>),
    Exit,
    Request(Box<Request>, Sender<Message>),
    Attach(Sender<Message>),
}
struct Server {
    agents: Vec<serde_json::Value>,
    contexts: crate::workspaces::RepositoryContexts,
    owners: HashMap<String, String>,
    draining: bool,
    upgrade_lease: Option<UpgradeLease>,
    upgrade_boot: Option<UpgradeBoot>,
    coordination_inflight: usize,
    state: State,
    panes: Arc<HashMap<String, Handle>>,
    routes: Arc<Mutex<PaneRoutes>>,
    operations: HashMap<String, Message>,
}
#[derive(Default)]
struct PaneRoutes {
    panes: Arc<HashMap<String, Handle>>,
    draining: bool,
}
impl Server {
    fn set_draining(&mut self, value: bool) {
        self.routes.lock().unwrap().draining = value;
        self.draining = value;
    }
    fn change_handle(&mut self, pane: String, replacement: Option<Handle>) -> Option<Handle> {
        let mut routes = self.routes.lock().unwrap();
        // Give the layout and the input path the same immutable handle map.
        routes.panes = Arc::default();
        let panes = Arc::make_mut(&mut self.panes);
        let previous = match replacement {
            Some(handle) => panes.insert(pane, handle),
            None => panes.remove(&pane),
        };
        routes.panes = self.panes.clone();
        previous
    }
}
struct UpgradeLease {
    token: String,
    expires: std::time::Instant,
}
pub const UPGRADE_BOOT_FILE: &str = ".upgrade-boot.json";
#[derive(serde::Deserialize)]
struct UpgradeBoot {
    token: String,
    #[serde(default)]
    resume_panes: Vec<String>,
}
fn upgrade_boot_hold(path: &std::path::Path) -> Result<Option<UpgradeBoot>> {
    use std::os::unix::fs::MetadataExt;
    let file = match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
        || metadata.len() > 1024 * 1024
    {
        bail!("Invalid upgrade boot hold ownership or permissions");
    }
    let hold: UpgradeBoot = serde_json::from_reader(file)?;
    uuid::Uuid::parse_str(&hold.token).context("Invalid upgrade boot ownership token")?;
    for pane in &hold.resume_panes {
        uuid::Uuid::parse_str(pane).context("Invalid takeover pane ID")?;
    }
    Ok(Some(hold))
}
fn restart_saved_agents(server: &mut Server, resume_panes: Option<&[String]>) -> Result<()> {
    let board = crate::coordination::Board::open()?;
    let ids: Vec<String> = board.snapshot()?["agents"]
        .as_array()
        .context("Invalid saved agent list")?
        .iter()
        .filter(|a| a["paused"] != true && a["closed"] != true && a["verified_launch"] == true)
        .filter(|a| resume_panes.is_none() || a["session"].is_string())
        .filter_map(|a| a["pane"].as_str())
        .filter(|pane| resume_panes.is_none_or(|panes| panes.iter().any(|id| id == pane)))
        .map(str::to_owned)
        .collect();
    for pane in ids {
        let _ = request(server, Request::Restart { pane });
    }
    Ok(())
}
fn incarnation() -> &'static str {
    static VALUE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VALUE.get_or_init(id)
}
fn refresh_upgrade_lease(server: &mut Server) {
    if server
        .upgrade_lease
        .as_ref()
        .is_some_and(|lease| std::time::Instant::now() >= lease.expires)
    {
        server.upgrade_lease = None;
        server.set_draining(server.upgrade_boot.is_some());
    }
}
fn upgrade_control(request: &Request) -> bool {
    matches!(
        request,
        Request::State
            | Request::Capabilities
            | Request::HostInfo
            | Request::Stop
            | Request::PrepareUpgrade
            | Request::PrepareUpgradeApproved { .. }
            | Request::AbortUpgrade { .. }
            | Request::CommitUpgrade { .. }
            | Request::StopUpgrade { .. }
    )
}
fn claim_upgrade_stop(server: &mut Server, token: &str) -> Result<()> {
    refresh_upgrade_lease(server);
    if server.upgrade_boot.as_ref().map(|hold| hold.token.as_str()) == Some(token) {
        let path = state_dir().join(UPGRADE_BOOT_FILE);
        if upgrade_boot_hold(&path)?.is_none_or(|hold| hold.token != token) {
            bail!("Upgrade boot ownership changed. Server was left running");
        }
        server.upgrade_lease = None;
        server.set_draining(true);
        return Ok(());
    }
    if !server
        .upgrade_lease
        .as_ref()
        .is_some_and(|lease| lease.token == token)
    {
        bail!("Upgrade confirmation expired or changed. Confirm the upgrade again");
    }
    // Stop consumes ownership under the same lock used to stop all panes. No
    // expiring lease can reopen admission between this check and shutdown.
    server.upgrade_lease = None;
    server.set_draining(true);
    Ok(())
}
// Board mutations span requests to the pane actors. Account for the whole
// operation without holding the state lock across SQLite/provider work.
struct CoordinationAdmission(Arc<Mutex<Server>>);
impl CoordinationAdmission {
    fn acquire(server: &Arc<Mutex<Server>>) -> Result<Self> {
        let mut state = server.lock().unwrap();
        refresh_upgrade_lease(&mut state);
        if state.draining {
            bail!("Daemon is preparing an upgrade");
        }
        state.coordination_inflight += 1;
        Ok(Self(server.clone()))
    }
}
impl Drop for CoordinationAdmission {
    fn drop(&mut self) {
        self.0.lock().unwrap().coordination_inflight -= 1;
    }
}
fn cwd_path(value: &str) -> Option<String> {
    if let Some(url) = value.strip_prefix("file://") {
        let (_, path) = url.split_once('/')?;
        Some(
            percent_encoding::percent_decode_str(&format!("/{path}"))
                .decode_utf8()
                .ok()?
                .into_owned(),
        )
    } else if value.starts_with('/') {
        Some(value.into())
    } else {
        None
    }
}
fn spawn(mut pane: Pane) -> Result<Handle> {
    let directory = fs::metadata(&pane.cwd)
        .with_context(|| format!("Cannot open terminal directory: {}", pane.cwd))?;
    if !directory.is_dir() {
        bail!("Terminal directory is not a folder: {}", pane.cwd)
    }
    pane.session_generation = Some(id());
    pane.agent_provider = None;
    pane.agent_source = None;
    let info = Arc::new(Mutex::new(pane.clone()));
    let meta = info.clone();
    let root_pid = Arc::new(std::sync::atomic::AtomicU32::new(0));
    let spawned_pid = root_pid.clone();
    let (tx, rx) = bounded(256);
    let (ready, wait) = bounded(1);
    let actor = tx.clone();
    thread::spawn(move || {
        let result = (|| -> Result<()> {
            let pair = native_pty_system().openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })?;
            let managed = crate::coordination::pane_command(&pane.id)?;
            let mut cmd = if let Some(args) = &managed {
                let mut c = CommandBuilder::new(&args[0]);
                c.args(&args[1..]);
                c
            } else {
                CommandBuilder::new(std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into()))
            };
            if managed.is_none()
                && let Some(shell) = std::path::Path::new(cmd.get_argv()[0].as_os_str())
                    .file_name()
                    .and_then(|s| s.to_str())
                && ["zsh", "bash", "sh", "fish"].contains(&shell)
            {
                cmd.arg("-l");
            }
            cmd.cwd(&pane.cwd);
            cmd.env("TERM", "xterm-256color");
            cmd.env("COLORTERM", "truecolor");
            cmd.env("JAM_PANE_ID", &pane.id);
            cmd.env("JAM_WORKSPACE_ID", &pane.workspace);
            cmd.env("JAM_SOCKET", socket());
            if let Ok(cli) = crate::cli_path()
                && let Some(bin) = cli.parent()
                && bin.is_dir()
            {
                cmd.env("JAM_SKILL_PATH", bin.join("SKILL.md"));
                cmd.env(
                    "PATH",
                    format!(
                        "{}:{}",
                        bin.display(),
                        std::env::var("PATH").unwrap_or_default()
                    ),
                );
            }
            let mut child = Process::new(pair.slave.spawn_command(cmd)?);
            spawned_pid.store(
                child.child.process_id().unwrap_or(0),
                std::sync::atomic::Ordering::Relaxed,
            );
            drop(pair.slave);
            let mut reader = pair.master.try_clone_reader()?;
            let mut writer = pair.master.take_writer()?;
            let (write_tx, write_rx) = bounded::<Vec<u8>>(16);
            thread::spawn(move || {
                while let Ok(bytes) = write_rx.recv() {
                    if writer.write_all(&bytes).is_err() {
                        break;
                    }
                }
            });
            let write = |bytes: &[u8]| -> Result<()> {
                if bytes.len() > 1024 * 1024 {
                    bail!("input exceeds 1 MiB limit")
                }
                write_tx
                    .try_send(bytes.to_vec())
                    .context("terminal input queue is full or closed")?;
                Ok(())
            };
            let mut vt = vt::terminal(80, 24)?;
            let replies = Arc::new(Mutex::new(Vec::<u8>::new()));
            let q = replies.clone();
            vt.on_pty_write(move |_, b| q.lock().unwrap().extend_from_slice(b))?;
            let status = Arc::new(Mutex::new(crate::status::Status::default()));
            let report = status.clone();
            vt.on_program_status(move |_, r| {
                report.lock().unwrap().report(r);
            })?;
            let prompt = status.clone();
            vt.on_semantic_prompt(move |_, p| {
                use libghostty_vt::effect::{PromptKind, SemanticPromptKind};
                if matches!(p.kind(), Ok(SemanticPromptKind::PromptStart))
                    && matches!(p.prompt_kind(), Ok(PromptKind::Primary))
                {
                    prompt.lock().unwrap().prompt();
                }
            })?;
            let mut clients: Vec<Sender<Message>> = Vec::new();
            let (mut seq, mut cols, mut rows) = (0u64, 80, 24);
            let mut exited = false;
            let mut root_reaped = false;
            let input = actor.clone();
            thread::spawn(move || {
                let mut b = [0u8; 65536];
                while let Ok(n) = reader.read(&mut b) {
                    if n == 0 {
                        break;
                    }
                    if input.send(Op::Output(b[..n].to_vec())).is_err() {
                        return;
                    }
                }
                let _ = input.send(Op::Exit);
            });
            drop(actor);
            ready.send(Ok(())).ok();
            loop {
                {
                    let mut activity = status.lock().unwrap();
                    if activity.foreground(
                        pair.master.process_group_leader(),
                        child.child.process_id().map(|p| p as i32),
                    ) {
                        let mut m = meta.lock().unwrap();
                        m.agent_provider = None;
                        m.agent_source = None;
                        m.status = activity.state();
                        m.reports = activity.records();
                    }
                }
                // PTY EOF and root exit are independent: reap the root even
                // when EOF arrived first, and clean up jobs keeping the PTY open.
                if !root_reaped && child.poll_exit()? {
                    root_reaped = true;
                    if !exited {
                        exited = true;
                        let mut m = meta.lock().unwrap();
                        m.exited = true;
                        m.agent_provider = None;
                        m.agent_source = None;
                        clients.retain(|c| c.try_send(Message::Exited).is_ok());
                    }
                }
                let op = match rx.recv_timeout(std::time::Duration::from_millis(100)) {
                    Ok(op) => op,
                    Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                    Err(_) => break,
                };
                match op {
                    Op::Output(b) => {
                        vt.vt_write(&b);
                        {
                            let mut m = meta.lock().unwrap();
                            m.title = String::from_utf8_lossy(vt.title()?).into_owned();
                            if m.title.is_empty() {
                                m.title = "Terminal".into()
                            }
                            let cwd = String::from_utf8_lossy(vt.pwd()?).into_owned();
                            if let Some(path) = cwd_path(&cwd) {
                                m.cwd = path;
                            }
                            let status = status.lock().unwrap();
                            m.status = status.state();
                            m.reports = status.records();
                            let identity = crate::agents::Agent::resolve(None, status.provider());
                            m.agent_provider = identity.map(|(p, _)| p.name().to_owned());
                            m.agent_source = identity.map(|(_, source)| source);
                        }
                        seq += 1;
                        let msg = Message::Output { seq, bytes: b };
                        clients.retain(|c| c.try_send(msg.clone()).is_ok());
                        let reply = std::mem::take(&mut *replies.lock().unwrap());
                        if !reply.is_empty() {
                            write(&reply)?;
                        }
                    }
                    Op::Exit => {
                        if exited {
                            continue;
                        }
                        exited = true;
                        {
                            let mut status = status.lock().unwrap();
                            status.end();
                            let mut m = meta.lock().unwrap();
                            m.exited = true;
                            m.agent_provider = None;
                            m.agent_source = None;
                            m.status = status.state();
                            m.reports = status.records();
                        }
                        clients.retain(|c| c.try_send(Message::Exited).is_ok());
                    }
                    Op::Attach(c) => match vt.encode_snapshot_alloc(None) {
                        Ok(Some(bytes)) => {
                            if c.try_send(Message::Snapshot {
                                seq,
                                cols,
                                rows,
                                bytes: bytes.to_vec(),
                            })
                            .is_ok()
                            {
                                clients.push(c)
                            }
                        }
                        e => {
                            let _ =
                                c.try_send(Message::Error(format!("snapshot unavailable: {e:?}")));
                        }
                    },
                    Op::Request(req, res) => {
                        let req = *req;
                        let closing = matches!(req, Request::Close { .. });
                        let result = (|| -> Result<Message> {
                            match req {
                                Request::Key {
                                    key,
                                    text,
                                    mods,
                                    release,
                                    ..
                                } => {
                                    let mut event = libghostty_vt::key::Event::new()?;
                                    event.set_action(if release {
                                        libghostty_vt::key::Action::Release
                                    } else {
                                        libghostty_vt::key::Action::Press
                                    });
                                    event
                                        .set_key(
                                            libghostty_vt::key::Key::try_from(key as i32)
                                                .map_err(|_| anyhow::anyhow!("invalid key"))?,
                                        )
                                        .set_mods(libghostty_vt::key::Mods::from_bits_truncate(
                                            mods,
                                        ));
                                    if let Some(c) = text.chars().next() {
                                        event.set_unshifted_codepoint(c);
                                    }
                                    event.set_utf8(Some(text.clone()));
                                    let mut encoder = libghostty_vt::key::Encoder::new()?;
                                    encoder.set_options_from_terminal(&vt);
                                    let mut b = Vec::new();
                                    encoder.encode_to_vec(&event, &mut b)?;
                                    if !release
                                        && (b == [3]
                                            || b == [27]
                                            || key == libghostty_vt::key::Key::Escape as u32
                                            || (text.eq_ignore_ascii_case("c")
                                                && libghostty_vt::key::Mods::from_bits_truncate(
                                                    mods,
                                                )
                                                .contains(libghostty_vt::key::Mods::CTRL)))
                                    {
                                        crate::recovery::cancel_input(&pane.id)?;
                                    }
                                    write(&b)?;
                                    Ok(Message::Ok)
                                }
                                Request::Mouse {
                                    action,
                                    button,
                                    x,
                                    y,
                                    cell_width,
                                    cell_height,
                                    mods,
                                    ..
                                } => {
                                    use libghostty_vt::mouse::{
                                        Action, Button, Encoder, EncoderSize, Event, Position,
                                    };
                                    if !x.is_finite()
                                        || !y.is_finite()
                                        || cell_width == 0
                                        || cell_height == 0
                                        || cell_width > 1000
                                        || cell_height > 1000
                                    {
                                        bail!("invalid mouse geometry")
                                    }
                                    let mut encoder = Encoder::new()?;
                                    encoder.set_options_from_terminal(&vt);
                                    encoder.set_size(EncoderSize {
                                        screen_width: u32::from(cols) * cell_width,
                                        screen_height: u32::from(rows) * cell_height,
                                        cell_width,
                                        cell_height,
                                        padding_top: 0,
                                        padding_bottom: 0,
                                        padding_left: 0,
                                        padding_right: 0,
                                    })?;
                                    encoder.set_any_button_pressed(action == 2 && button != 255);
                                    let mut event = Event::new()?;
                                    event.set_action(match action {
                                        0 => Action::Press,
                                        1 => Action::Release,
                                        2 => Action::Motion,
                                        _ => bail!("invalid mouse action"),
                                    });
                                    event.set_button(match button {
                                        0 => Some(Button::Left),
                                        1 => Some(Button::Right),
                                        2 => Some(Button::Middle),
                                        3 => Some(Button::Four),
                                        4 => Some(Button::Five),
                                        255 => None,
                                        _ => bail!("invalid mouse button"),
                                    });
                                    event.set_position(Position { x, y }).set_mods(
                                        libghostty_vt::key::Mods::from_bits_truncate(mods),
                                    );
                                    let mut b = Vec::new();
                                    encoder.encode_to_vec(&event, &mut b)?;
                                    write(&b)?;
                                    Ok(Message::Ok)
                                }
                                Request::Paste { text, .. } => {
                                    let bracketed = vt.mode(libghostty_vt::mode::Mode::new(
                                        2004,
                                        libghostty_vt::mode::Kind::Dec,
                                    ))?;
                                    let mut data = Vec::new();
                                    if bracketed {
                                        data.extend_from_slice(b"\x1b[200~");
                                    }
                                    data.extend_from_slice(text.as_bytes());
                                    if bracketed {
                                        data.extend_from_slice(b"\x1b[201~");
                                    }
                                    write(&data)?;
                                    Ok(Message::Ok)
                                }
                                Request::Acknowledge { .. } => {
                                    let mut status = status.lock().unwrap();
                                    status.acknowledge();
                                    let mut m = meta.lock().unwrap();
                                    m.status = status.state();
                                    m.reports = status.records();
                                    Ok(Message::Ok)
                                }
                                Request::Capture { .. } => Ok(Message::Capture(vt::text(&vt)?)),
                                Request::Send { data, .. } => {
                                    if data == [3] || data == [27] {
                                        crate::recovery::cancel_input(&pane.id)?;
                                    }
                                    if exited {
                                        bail!("pane exited")
                                    }
                                    write(&data)?;
                                    Ok(Message::Ok)
                                }
                                Request::Resize {
                                    cols: c, rows: r, ..
                                } => {
                                    if c == 0 || r == 0 || c > 1000 || r > 1000 {
                                        bail!("invalid geometry")
                                    }
                                    if (c, r) != (cols, rows) {
                                        vt.resize(c, r, 0, 0)?;
                                        cols = c;
                                        rows = r;
                                        pair.master.resize(PtySize {
                                            cols,
                                            rows,
                                            pixel_width: 0,
                                            pixel_height: 0,
                                        })?;
                                        seq += 1;
                                        clients.retain(|x| {
                                            x.try_send(Message::Geometry { seq, cols, rows })
                                                .is_ok()
                                        });
                                    }
                                    Ok(Message::Ok)
                                }
                                Request::Close { .. } => {
                                    if !exited
                                        && let Some(pg) = pair.master.process_group_leader()
                                        && pg > 1
                                        && pg != unsafe { libc::getpgrp() }
                                        && Some(pg) != child.owned_group
                                    {
                                        unsafe {
                                            libc::killpg(pg, libc::SIGKILL);
                                        }
                                    }
                                    child.terminate()?;
                                    meta.lock().unwrap().exited = true;
                                    Ok(Message::Ok)
                                }
                                Request::Status { state, .. } => {
                                    if !["unknown", "idle", "working", "blocked", "done", "error"]
                                        .contains(&state.as_str())
                                    {
                                        bail!("invalid status")
                                    }
                                    status.lock().unwrap().set(state.clone());
                                    meta.lock().unwrap().status = state;
                                    Ok(Message::Ok)
                                }
                                _ => bail!("unsupported pane request"),
                            }
                        })();
                        let _ =
                            res.send(result.unwrap_or_else(|e| Message::Error(format!("{e:#}"))));
                        if closing {
                            break;
                        }
                    }
                }
            }
            Ok(())
        })();
        if let Err(e) = result {
            meta.lock().unwrap().exited = true;
            let _ = ready.send(Err(e.to_string()));
        }
    });
    wait.recv()?.map_err(anyhow::Error::msg)?;
    Ok(Handle { tx, info, root_pid })
}
fn pane_send_error<T>(error: crossbeam_channel::SendTimeoutError<T>) -> anyhow::Error {
    match error {
        crossbeam_channel::SendTimeoutError::Disconnected(_) => {
            anyhow::anyhow!("Terminal has stopped")
        }
        crossbeam_channel::SendTimeoutError::Timeout(_) => anyhow::anyhow!("Terminal is busy"),
    }
}
fn ask(h: &Handle, r: Request) -> Result<Message> {
    let (tx, rx) = bounded(1);
    h.tx.send_timeout(
        Op::Request(Box::new(r), tx),
        std::time::Duration::from_secs(2),
    )
    .map_err(pane_send_error)?;
    rx.recv_timeout(std::time::Duration::from_secs(10))
        .map_err(|error| match error {
            crossbeam_channel::RecvTimeoutError::Disconnected => {
                anyhow::anyhow!("Terminal has stopped")
            }
            crossbeam_channel::RecvTimeoutError::Timeout => {
                anyhow::anyhow!("Terminal did not respond")
            }
        })
}
fn stop_pane(s: &Server, pane: &str) -> Result<()> {
    let Some(handle) = s.panes.get(pane) else {
        return Ok(());
    };
    // An exited root can still have a live actor; close it to release its PTY.
    // Already-disconnected stopped actors remain an idempotent success below.
    match ask(handle, Request::Close { pane: pane.into() }) {
        Ok(Message::Error(error)) => bail!(error),
        Ok(_) => Ok(()),
        Err(_) if handle.info.lock().unwrap().exited => Ok(()),
        Err(error) => Err(error),
    }
}
fn save(s: &Server) -> Result<()> {
    let dir = state_dir();
    private_dir(&dir)?;
    let tmp = dir.join("layout.tmp");
    let mut f = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&tmp)?;
    f.write_all(&serde_json::to_vec_pretty(&s.state)?)?;
    f.sync_all()?;
    fs::rename(tmp, dir.join("layout.json"))?;
    fs::File::open(&dir)?.sync_all()?;
    Ok(())
}
fn new_pane(s: &mut Server, path: String, workspace: String) -> Result<String> {
    let id = id();
    let p = Pane {
        session_generation: None,
        launch_error: None,
        agent_provider: None,
        agent_source: None,
        agent_resume: None,
        agent_role: None,
        id: id.clone(),
        title: "Terminal".into(),
        cwd: path,
        status: "unknown".into(),
        exited: false,
        reports: Vec::new(),
        workspace,
    };
    let h = spawn(p.clone())?;
    s.change_handle(id.clone(), Some(h));
    s.state.panes.push(p);
    Ok(id)
}
pub(crate) fn prune(n: Node, id: &str) -> Option<Node> {
    match n {
        Node::Empty => Some(Node::Empty),
        Node::Leaf(p) => {
            if p == id {
                None
            } else {
                Some(Node::Leaf(p))
            }
        }
        Node::Split {
            vertical,
            ratio,
            a,
            b,
        } => match (prune(*a, id), prune(*b, id)) {
            (Some(a), Some(b)) => Some(Node::Split {
                vertical,
                ratio,
                a: Box::new(a),
                b: Box::new(b),
            }),
            (a, b) => a.or(b),
        },
    }
}
pub(crate) fn remove_pane_from_layout(
    state: &mut State,
    pane: &str,
    settings: &crate::config::HostSettings,
) {
    for w in &mut state.workspaces {
        w.tabs = std::mem::take(&mut w.tabs)
            .into_iter()
            .filter_map(|mut t| {
                t.root = match prune(t.root, pane) {
                    Some(root) => root,
                    None if !settings.close_empty_tabs => Node::Empty,
                    None => return None,
                };
                Some(t)
            })
            .collect();
        if w.active_pane.as_deref() == Some(pane) {
            w.active_pane = None;
        }
    }
    remove_empty_workspaces(state, settings);
}
fn remove_empty_workspaces(state: &mut State, settings: &crate::config::HostSettings) {
    if settings.close_empty_workspaces {
        // Keep containers with open worktrees: closing a pane must not orphan them.
        let parents: std::collections::HashSet<_> = state
            .workspaces
            .iter()
            .filter_map(|w| w.parent.clone())
            .collect();
        state
            .workspaces
            .retain(|w| !w.tabs.is_empty() || w.coordinator.is_some() || parents.contains(&w.id));
    }
}
fn split(n: &mut Node, id: &str, new: &str, vertical: bool) -> bool {
    match n {
        Node::Leaf(p) if p == id => {
            *n = Node::Split {
                vertical,
                ratio: 0.5,
                a: Box::new(Node::Leaf(id.into())),
                b: Box::new(Node::Leaf(new.into())),
            };
            true
        }
        Node::Split { a, b, .. } => split(a, id, new, vertical) || split(b, id, new, vertical),
        _ => false,
    }
}
fn create_workspace(
    s: &mut Server,
    path: String,
    name: String,
    reuse: bool,
    proposed: Option<&str>,
) -> Result<String> {
    let path = fs::canonicalize(path)?;
    if !path.is_dir() {
        bail!("workspace must be a directory")
    }
    let path = path.to_string_lossy().into_owned();
    if reuse && let Some(existing) = s.state.workspaces.iter().find(|w| w.path == path) {
        return Ok(existing.id.clone());
    }
    let identity = crate::worktree::identify(std::path::Path::new(&path));
    let workspace = proposed.map(str::to_owned).unwrap_or_else(id);
    let p = new_pane(s, path.clone(), workspace.clone())?;
    s.state.workspaces.push(Workspace {
        coordinator: None,
        parent: None,
        context: None,
        id: workspace.clone(),
        auto_name: Some(name.is_empty()),
        active_pane: Some(p.clone()),
        last_cwd: None,
        name: if name.is_empty() {
            path.rsplit('/').next().unwrap_or("Workspace").into()
        } else {
            name
        },
        repository: identity.as_ref().map(|x| x.0.clone()),
        main_path: identity.as_ref().map(|x| x.1.clone()),
        branch: identity.map(|x| x.2),
        path,
        tabs: vec![Tab {
            agent_role: None,
            id: id(),
            name: "Tab 1".into(),
            root: Node::Leaf(p),
        }],
    });
    Ok(workspace)
}
fn folder_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("/")
        .to_owned()
}
fn update_workspace_names(state: &mut State) {
    for workspace in &mut state.workspaces {
        let automatic = *workspace
            .auto_name
            .get_or_insert_with(|| workspace.name == folder_name(&workspace.path));
        if !automatic {
            continue;
        }
        let active = state
            .panes
            .iter()
            .find(|p| {
                Some(&p.id) == workspace.active_pane.as_ref()
                    && p.workspace == workspace.id
                    && p.agent_role.as_deref() != Some("coordinator")
            })
            .or_else(|| {
                state.panes.iter().find(|p| {
                    p.workspace == workspace.id && p.agent_role.as_deref() != Some("coordinator")
                })
            });
        workspace.name = folder_name(
            active
                .map(|p| p.cwd.as_str())
                .unwrap_or(workspace.last_cwd.as_ref().unwrap_or(&workspace.path)),
        );
    }
}
fn refresh_pane_metadata(s: &mut Server) {
    let agents = &s.agents;
    for pane in &mut s.state.panes {
        if let Some(handle) = s.panes.get(&pane.id) {
            *pane = handle.info.lock().unwrap().clone();
        }
        pane.agent_resume = None;
        if let Some(agent) = agents.iter().find(|a| a["pane"] == pane.id) {
            pane.agent_resume = agent["id"].as_str().map(|id| AgentResume {
                id: id.to_owned(),
                verified_launch: agent["verified_launch"] == true,
                session: agent["session"].as_str().map(str::to_owned),
            });
            let reported = if pane.agent_source == Some(crate::AgentSource::Reported) {
                pane.agent_provider
                    .as_deref()
                    .and_then(crate::agents::Agent::from_program)
            } else {
                None
            };
            let identity = crate::agents::Agent::resolve(agent["provider"].as_str(), reported);
            pane.agent_provider = identity.map(|(p, _)| p.name().to_owned());
            pane.agent_source = identity.map(|(_, source)| source);
            pane.agent_role = agent["role"].as_str().map(str::to_owned);
            pane.launch_error = agent["launch_error"]
                .as_str()
                .or(agent["recovery"]["error"].as_str())
                .map(str::to_owned);
            match agent["recovery"]["state"].as_str() {
                Some("pending" | "dispatching" | "accepted") => pane.status = "Recovering…".into(),
                Some("failed") => pane.status = "recovery_failed".into(),
                _ => {}
            }
        }
    }
    for w in &mut s.state.workspaces {
        if let Some(p) = s.state.panes.iter().find(|p| {
            p.workspace == w.id
                && p.agent_role.as_deref() != Some("coordinator")
                && Some(&p.id) == w.active_pane.as_ref()
        }) {
            w.last_cwd = Some(p.cwd.clone());
        } else if w.last_cwd.is_none() {
            w.last_cwd = s
                .state
                .panes
                .iter()
                .find(|p| p.workspace == w.id && p.agent_role.as_deref() != Some("coordinator"))
                .map(|p| p.cwd.clone());
        }
    }
    for w in &mut s.state.workspaces {
        w.coordinator = agents
            .iter()
            .find(|a| a["workspace"] == w.id && a["role"] == "coordinator" && a["closed"] != true)
            .map(|a| serde_json::json!({"id":a["id"],"pane":a["pane"],"provider":a["provider"]}));
        // Coordinators own a dedicated surface, never a terminal tab. Lift
        // legacy coordinator panes out without recreating their conversations
        // or discarding ordinary panes from an old split layout.
        if let Some(pane) = w.coordinator.as_ref().and_then(|a| a["pane"].as_str())
            && w.tabs.iter().any(|tab| tab.root.contains(pane))
        {
            w.tabs = std::mem::take(&mut w.tabs)
                .into_iter()
                .filter_map(|mut tab| {
                    tab.root = prune(tab.root, pane)?;
                    if tab.agent_role.as_deref() == Some("coordinator") {
                        tab.agent_role = None;
                    }
                    Some(tab)
                })
                .collect();
        }
    }
}
fn request(s: &mut Server, r: Request) -> Result<Message> {
    refresh_upgrade_lease(s);
    if s.draining && !upgrade_control(&r) {
        bail!("Daemon is preparing an upgrade");
    }
    if !matches!(&r, Request::State) {
        refresh_pane_metadata(s);
    }
    match r {
        Request::PrepareUpgradeApproved {
            token,
            incarnation: approved_incarnation,
            build,
            panes,
            sessions,
        } => {
            uuid::Uuid::parse_str(&token).context("Invalid upgrade ownership token")?;
            if approved_incarnation != incarnation() || build != env!("JAM_BUILD_ID") {
                bail!("Server changed after confirmation. Confirm the upgrade again");
            }
            if s.upgrade_lease
                .as_ref()
                .is_some_and(|lease| lease.token != token)
                || (s.draining && s.upgrade_lease.is_none())
            {
                bail!("Another server upgrade is in progress");
            }
            if sessions.len() != panes.len() || panes.iter().any(|id| !sessions.contains_key(id)) {
                bail!(
                    "Upgrade approval is missing terminal session identities. Confirm the upgrade again"
                );
            }
            if s.coordination_inflight != 0
                || s.panes.iter().any(|(id, handle)| {
                    let pane = handle.info.lock().unwrap();
                    !pane.exited
                        && (!panes.contains(id)
                            || pane.session_generation.as_ref() != sessions.get(id))
                })
                || s.operations
                    .values()
                    .any(|operation| matches!(operation, Message::Operation { done: false, .. }))
            {
                bail!(
                    "New terminals or operations started after confirmation. Confirm the upgrade again"
                );
            }
            s.set_draining(true);
            s.upgrade_lease = Some(UpgradeLease {
                token,
                expires: std::time::Instant::now() + std::time::Duration::from_secs(120),
            });
            return Ok(Message::Ok);
        }
        Request::CommitUpgrade { token } => {
            let hold = s
                .upgrade_boot
                .as_ref()
                .context("Missing upgrade boot hold")?;
            if hold.token != token {
                bail!("This client does not own the upgrade boot hold");
            }
            let path = state_dir().join(UPGRADE_BOOT_FILE);
            if let Some(on_disk) = upgrade_boot_hold(&path)? {
                if on_disk.token != token {
                    bail!("Upgrade boot hold changed. Automatic recovery is required");
                }
                fs::remove_file(&path)?;
            }
            // Retain the in-memory hold until removal is durable. The owning
            // token may retry if a previous directory fsync failed after unlink.
            fs::File::open(state_dir())?.sync_all()?;
            let hold = s.upgrade_boot.take().unwrap();
            s.set_draining(false);
            // A takeover of an unresponsive server uses its saved layout.
            // A regular upgrade restores live pane IDs from its approved plan.
            if !hold.resume_panes.is_empty() {
                restart_saved_agents(s, Some(&hold.resume_panes))?;
            }
            return Ok(Message::Ok);
        }
        Request::AbortUpgrade { token } => {
            if let Some(lease) = &s.upgrade_lease {
                if lease.token != token {
                    bail!("This client does not own the server upgrade");
                }
                s.upgrade_lease = None;
                s.set_draining(false);
            } else if s.draining {
                bail!("This client does not own the server upgrade");
            }
            return Ok(Message::Ok);
        }
        Request::PrepareUpgrade => {
            if s.upgrade_lease.is_some() {
                bail!("Another server upgrade is in progress");
            }
            if s.coordination_inflight != 0
                || s.panes.values().any(|p| !p.info.lock().unwrap().exited)
                || s.operations
                    .values()
                    .any(|m| matches!(m, Message::Operation { done: false, .. }))
            {
                bail!("Running panes or operations prevent upgrade");
            }
            s.set_draining(true);
            return Ok(Message::Ok);
        }
        Request::Guarded {
            revision,
            request: inner,
        } => {
            if revision != s.state.revision {
                bail!("Layout changed on another client. Refresh and retry");
            }
            return request(s, *inner);
        }
        Request::InputStream => bail!("Input stream requires a dedicated connection"),
        Request::Client { .. } => bail!("nested client envelope"),
        Request::HostInfo => return host_info(),
        Request::Directory { path } => {
            let home = std::env::var("HOME")?;
            let path = if path == "~" {
                home
            } else if let Some(rest) = path.strip_prefix("~/") {
                format!("{home}/{rest}")
            } else {
                path
            };
            let path = std::fs::canonicalize(path)?;
            let mut directories = Vec::new();
            for entry in std::fs::read_dir(&path)? {
                let entry = entry?;
                if entry.path().is_dir() {
                    directories.push(entry.file_name().to_string_lossy().into_owned());
                }
            }
            directories.sort();
            return Ok(Message::Directory {
                path: path.to_string_lossy().into_owned(),
                directories,
            });
        }
        Request::Configure { settings } => {
            crate::config::apply_host_settings(settings)?;
            let providers: std::collections::BTreeMap<_, _> = ["claude", "codex"]
                .into_iter()
                .map(|p| (p, crate::coordination::provider_executable(p).is_ok()))
                .collect();
            return Ok(Message::Settings(
                serde_json::json!({"providers": providers}),
            ));
        }
        Request::CloseTab { tab } => {
            let w = s
                .state
                .workspaces
                .iter()
                .find(|w| w.tabs.iter().any(|t| t.id == tab))
                .context("Tab missing")?;
            let workspace = w.id.clone();
            let mut panes = Vec::new();
            w.tabs
                .iter()
                .find(|t| t.id == tab)
                .unwrap()
                .root
                .ids(&mut panes);
            for pane in panes {
                request(s, Request::Close { pane })?;
            }
            if let Some(w) = s.state.workspaces.iter_mut().find(|w| w.id == workspace) {
                w.tabs.retain(|t| t.id != tab);
            }
            remove_empty_workspaces(&mut s.state, &crate::config::host_settings());
        }
        Request::NewPane { tab } => {
            let w = s
                .state
                .workspaces
                .iter()
                .find(|w| w.tabs.iter().any(|t| t.id == tab))
                .context("Tab missing")?;
            if !matches!(
                w.tabs.iter().find(|t| t.id == tab).unwrap().root,
                Node::Empty
            ) {
                bail!("Tab already has panes")
            }
            let (workspace, path) = (w.id.clone(), w.last_cwd.as_ref().unwrap_or(&w.path).clone());
            let pane = new_pane(s, path, workspace.clone())?;
            let w = s
                .state
                .workspaces
                .iter_mut()
                .find(|w| w.id == workspace)
                .unwrap();
            w.tabs.iter_mut().find(|t| t.id == tab).unwrap().root = Node::Leaf(pane.clone());
            w.active_pane = Some(pane);
        }
        Request::Capabilities => {
            return Ok(Message::Capabilities(vec![
                "clipboard_images".into(),
                "approved_upgrade_v1".into(),
                "workspace_containers".into(),
                "input_stream".into(),
                "layout_revision".into(),
                "remote_files".into(),
                "move_tab".into(),
                "workspace_names".into(),
                "workspace_creation_v1".into(),
                "workspace_order".into(),
                "move_pane".into(),
                "git_summary".into(),
                "agent_usage".into(),
                "coordination_v1".into(),
                "coordinator_surface".into(),
                "agent_recovery_v1".into(),
                "workspace_initial_tab".into(),
                "coordination_models".into(),
                "app_settings_v1".into(),
                "worktree_templates".into(),
            ]));
        }
        Request::StoreClipboardImage { pane, png } => {
            let handle = s
                .panes
                .get(&pane)
                .context("The terminal is no longer running")?;
            if handle.info.lock().unwrap().exited {
                bail!("The terminal is no longer running");
            }
            let path = crate::clipboard::store(&png)?;
            return Ok(Message::ClipboardImage {
                path: path.to_string_lossy().into_owned(),
            });
        }
        Request::MovePane { pane, destination } => {
            if s.state
                .panes
                .iter()
                .any(|p| p.id == pane && p.agent_role.as_deref() == Some("coordinator"))
            {
                bail!("The coordinator has its own surface and cannot be moved into a tab")
            }
            s.state = crate::pane_move::moved(&s.state, &pane, destination)?;
            remove_empty_workspaces(&mut s.state, &crate::config::host_settings());
            for p in &s.state.panes {
                if let Some(handle) = s.panes.get(&p.id) {
                    handle.info.lock().unwrap().workspace = p.workspace.clone();
                }
            }
        }
        Request::ReorderWorkspaces { workspaces } => {
            let supplied: std::collections::HashSet<_> = workspaces.iter().collect();
            if supplied.len() != s.state.workspaces.len()
                || supplied.len() != workspaces.len()
                || s.state.workspaces.iter().any(|w| !supplied.contains(&w.id))
            {
                bail!("workspace order must contain every workspace exactly once")
            }
            s.state
                .workspaces
                .sort_by_key(|w| workspaces.iter().position(|id| id == &w.id).unwrap());
        }
        Request::MoveTab {
            tab,
            workspace,
            index,
        } => {
            let target = s
                .state
                .workspaces
                .iter()
                .position(|w| w.id == workspace)
                .context("target workspace not found")?;
            let source = s
                .state
                .workspaces
                .iter()
                .position(|w| w.tabs.iter().any(|t| t.id == tab))
                .context("tab not found")?;
            let old_index = s.state.workspaces[source]
                .tabs
                .iter()
                .position(|t| t.id == tab)
                .unwrap();
            let destination_len =
                s.state.workspaces[target].tabs.len() - usize::from(source == target);
            if index > destination_len {
                bail!("tab index out of range")
            }
            let moving = s.state.workspaces[source].tabs.remove(old_index);
            let mut ids = Vec::new();
            moving.root.ids(&mut ids);
            for pane in &mut s.state.panes {
                if ids.contains(&pane.id) {
                    pane.workspace = workspace.clone();
                    if let Some(handle) = s.panes.get(&pane.id) {
                        handle.info.lock().unwrap().workspace = workspace.clone();
                    }
                }
            }
            s.state.workspaces[target].tabs.insert(index, moving);
            remove_empty_workspaces(&mut s.state, &crate::config::host_settings());
        }
        Request::State => {
            let before = serde_json::to_vec(&s.state)?;
            refresh_pane_metadata(s);
            update_workspace_names(&mut s.state);
            let contexts: Vec<_> = s
                .state
                .workspaces
                .iter()
                .map(|w| {
                    s.contexts
                        .resolve(&crate::workspaces::active_path(&s.state, w))
                })
                .collect();
            for (workspace, context) in s.state.workspaces.iter_mut().zip(contexts) {
                workspace.context = Some(context);
            }
            if before != serde_json::to_vec(&s.state)? {
                s.state.revision += 1;
                save(s)?;
            }
            return Ok(Message::State(s.state.clone()));
        }
        Request::CreateWorkspace { path, name } => {
            create_workspace(s, path, name, true, None)?;
        }
        Request::NewWorkspace { path } => {
            create_workspace(s, path, String::new(), false, None)?;
        }
        Request::FocusPane { pane } => {
            let p = s
                .state
                .panes
                .iter()
                .find(|p| p.id == pane)
                .context("pane missing")?;
            // A coordinator is a sibling surface. Focusing it must not replace
            // the directory/selection remembered for ordinary workspace tabs.
            if p.agent_role.as_deref() == Some("coordinator") {
                return Ok(Message::Ok);
            }
            let workspace = s
                .state
                .workspaces
                .iter_mut()
                .find(|w| w.id == p.workspace)
                .context("workspace missing")?;
            if workspace.active_pane.as_ref() == Some(&pane) {
                return Ok(Message::Ok);
            }
            workspace.active_pane = Some(pane);
        }
        Request::WorkspaceClose { workspace, group } => {
            let w = s
                .state
                .workspaces
                .iter()
                .find(|w| w.id == workspace)
                .context("Workspace missing")?
                .clone();
            if !group
                && s.state
                    .workspaces
                    .iter()
                    .any(|x| x.parent.as_ref() == Some(&w.id))
            {
                bail!("Closing a workspace with open worktrees requires group=true")
            }
            let ids: Vec<String> = s
                .state
                .workspaces
                .iter()
                .filter(|x| x.id == workspace || (group && x.parent.as_ref() == Some(&w.id)))
                .map(|x| x.id.clone())
                .collect();
            let panes: Vec<String> = s
                .state
                .panes
                .iter()
                .filter(|p| ids.contains(&p.workspace))
                .map(|p| p.id.clone())
                .collect();
            for pane in panes {
                request(s, Request::Close { pane })?;
            }
            s.state.workspaces.retain(|w| !ids.contains(&w.id));
        }
        Request::Operation { id } => {
            return s.operations.get(&id).cloned().context("Operation missing");
        }
        Request::Coordination(v) if v["action"] == "start" || v["action"] == "open" => {
            let workspace = v["workspace"]
                .as_str()
                .context("Missing workspace")?
                .to_owned();
            let w = s
                .state
                .workspaces
                .iter()
                .find(|w| w.id == workspace)
                .context("Workspace missing")?
                .clone();
            let parent_agent = v["parent"]
                .as_str()
                .map(|parent| crate::coordination::Board::open()?.get(parent))
                .transpose()?;
            let provider = v["provider"]
                .as_str()
                .or_else(|| parent_agent.as_ref().and_then(|a| a["provider"].as_str()))
                .or_else(|| w.coordinator.as_ref().and_then(|a| a["provider"].as_str()))
                .or(Some("claude"))
                .context(if v["action"] == "open" {
                    "No coordinator exists in this workspace"
                } else {
                    "Missing provider"
                })?;
            if !crate::config::agent_enabled(provider) {
                bail!("{provider} integration is disabled in Settings")
            }
            crate::coordination::provider_executable(provider)?;
            let mut board = crate::coordination::Board::open()?;
            let role = v["role"].as_str().unwrap_or("coordinator");
            let history = board.snapshot()?;
            if v["operation"].is_string()
                && history["agents"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|a| a["operation"] == v["operation"] && a["closed"] == true)
            {
                bail!("This coordinator was closed. Start a new coordinator.");
            }
            if let Some(a) = history["agents"].as_array().unwrap().iter().find(|a| {
                a["closed"] != true
                    && ((role == "coordinator"
                        && a["workspace"] == workspace
                        && a["role"] == role
                        && a["closed"] != true)
                        || (v["operation"].is_string() && a["operation"] == v["operation"]))
            }) {
                if v["action"] == "start"
                    && !(v["operation"].is_string() && a["operation"] == v["operation"])
                {
                    bail!(
                        "This workspace already has a coordinator. Open the existing coordinator instead."
                    )
                }
                let mut a = a.clone();
                let pane_id = a["pane"].as_str().context("Missing agent pane")?.to_owned();
                a["paused"] = serde_json::json!(false);
                board.put("agent", &a)?;
                if !s.state.panes.iter().any(|p| p.id == pane_id) {
                    s.state.panes.push(Pane {
                        session_generation: None,
                        launch_error: None,
                        agent_provider: None,
                        agent_source: None,
                        agent_resume: None,
                        agent_role: None,
                        id: pane_id.clone(),
                        title: "Coordinator".into(),
                        cwd: a["path"].as_str().unwrap_or(&w.path).into(),
                        status: "unknown".into(),
                        exited: true,
                        reports: vec![],
                        workspace: workspace.clone(),
                    });
                }
                if a["started"] == true && a["session"].is_null() {
                    a["launch_error"] = serde_json::json!(
                        "The previous conversation was not linked to Jam. Choose a saved session or start a new conversation."
                    );
                    board.put("agent", &a)?;
                } else {
                    let _ = request(s, Request::Restart { pane: pane_id });
                }
                s.agents = board.agents()?;
                refresh_pane_metadata(s);
                save(s)?;
                return Ok(Message::Coordination(
                    serde_json::json!({"agent":a,"State":s.state}),
                ));
            }
            if v["action"] == "open" {
                bail!("No coordinator exists in this workspace")
            }
            let pane_id = id();
            let root = crate::workspaces::owner(&s.state, &workspace)?;
            let path = crate::workspaces::active_path(&s.state, &w);
            let mut a = board.add(crate::coordination::AgentSpec {
                workspace: &workspace,
                root: &root,
                pane: &pane_id,
                provider,
                role,
                path: &path,
                parent: v["parent"].as_str(),
                complex: v["complex"].as_bool().unwrap_or(false),
            })?;
            a["operation"] = v["operation"].clone();
            board.put("agent", &a)?;
            let pane = Pane {
                session_generation: None,
                launch_error: None,
                agent_provider: None,
                agent_source: None,
                agent_resume: None,
                agent_role: None,
                id: pane_id.clone(),
                title: role.into(),
                cwd: path,
                status: "unknown".into(),
                exited: true,
                reports: vec![],
                workspace: workspace.clone(),
            };
            s.state.panes.push(pane);
            let w = s
                .state
                .workspaces
                .iter_mut()
                .find(|w| w.id == workspace)
                .unwrap();
            if role != "coordinator" {
                let label = if role == "reviewer" {
                    "Reviewer"
                } else {
                    "Worker"
                };
                let name = v["tab_name"]
                    .as_str()
                    .filter(|name| !name.trim().is_empty())
                    .map(str::to_owned)
                    .unwrap_or_else(|| {
                        let number = (1..)
                            .find(|n| !w.tabs.iter().any(|t| t.name == format!("{label} {n}")))
                            .unwrap();
                        format!("{label} {number}")
                    });
                w.tabs.push(Tab {
                    agent_role: Some(role.into()),
                    id: id(),
                    name,
                    root: Node::Leaf(pane_id.clone()),
                });
            }
            s.agents = board.agents()?;
            refresh_pane_metadata(s);
            save(s)?;
            request(s, Request::Restart { pane: pane_id })?;
            refresh_pane_metadata(s);
            save(s)?;
            return Ok(Message::Coordination(
                serde_json::json!({"agent":a,"State":s.state}),
            ));
        }
        Request::NewTab { workspace } => {
            let w = s
                .state
                .workspaces
                .iter()
                .find(|w| w.id == workspace)
                .context("workspace not found")?;
            let path = w.last_cwd.as_ref().unwrap_or(&w.path).clone();
            let p = new_pane(s, path, workspace.clone())?;
            let w = s
                .state
                .workspaces
                .iter_mut()
                .find(|w| w.id == workspace)
                .unwrap();
            let number = (1..)
                .find(|n| !w.tabs.iter().any(|t| t.name == format!("Tab {n}")))
                .unwrap();
            w.tabs.push(Tab {
                agent_role: None,
                id: id(),
                name: format!("Tab {number}"),
                root: Node::Leaf(p),
            });
        }
        Request::Split { pane, vertical } => {
            if s.state
                .panes
                .iter()
                .any(|p| p.id == pane && p.agent_role.as_deref() == Some("coordinator"))
            {
                bail!("The coordinator has its own surface and cannot be split")
            }
            let path = s
                .state
                .panes
                .iter()
                .find(|p| p.id == pane)
                .context("pane not found")?
                .cwd
                .clone();
            let workspace = s
                .state
                .panes
                .iter()
                .find(|p| p.id == pane)
                .unwrap()
                .workspace
                .clone();
            let p = new_pane(s, path, workspace)?;
            for w in &mut s.state.workspaces {
                for t in &mut w.tabs {
                    split(&mut t.root, &pane, &p, vertical);
                }
            }
        }
        Request::Restart { pane } => {
            if crate::coordination::Board::open()?
                .agent_for_pane(&pane)?
                .is_some_and(|a| a["paused"] == true || a["closed"] == true)
            {
                return Ok(Message::State(s.state.clone()));
            }
            let mut replacement = s
                .state
                .panes
                .iter()
                .find(|p| p.id == pane)
                .context("pane not found")?
                .clone();
            if s.panes
                .get(&pane)
                .is_some_and(|h| !h.info.lock().unwrap().exited)
            {
                return Ok(Message::State(s.state.clone()));
            }
            replacement.exited = false;
            replacement.launch_error = None;
            let handle = match spawn(replacement.clone()) {
                Ok(handle) => handle,
                Err(error) => {
                    if let Ok(mut board) = crate::coordination::Board::open()
                        && let Ok(Some(agent)) = board.agent_for_pane(&pane)
                    {
                        let _ = board.action(serde_json::json!({"action":"launch_failed", "agent":agent["id"], "error":format!("{error:#}")}));
                    }
                    let p = s.state.panes.iter_mut().find(|p| p.id == pane).unwrap();
                    p.launch_error = Some(format!("{error:#}"));
                    s.state.revision += 1;
                    save(s)?;
                    return Err(error);
                }
            };
            *s.state.panes.iter_mut().find(|p| p.id == pane).unwrap() = replacement;
            if let Some(previous) = s.change_handle(pane.clone(), Some(handle)) {
                // End the exited actor after installing its replacement. Existing
                // subscribers must reconnect, not keep the old terminal's last frame.
                let _ = ask(&previous, Request::Close { pane });
            }
        }
        Request::Ratio { tab, path, ratio } => {
            if !ratio.is_finite() || !(0.1..=0.9).contains(&ratio) {
                bail!("invalid split ratio")
            }
            let t = s
                .state
                .workspaces
                .iter_mut()
                .flat_map(|w| w.tabs.iter_mut())
                .find(|t| t.id == tab)
                .context("tab not found")?;
            let mut n = &mut t.root;
            for right in path {
                n = match n {
                    Node::Split { a, b, .. } => {
                        if right {
                            b
                        } else {
                            a
                        }
                    }
                    _ => bail!("invalid split path"),
                }
            }
            if let Node::Split { ratio: r, .. } = n {
                *r = ratio
            } else {
                bail!("not a split")
            }
        }
        Request::Rename { id, name } => {
            for w in &mut s.state.workspaces {
                if w.id == id {
                    w.auto_name = Some(name.trim().is_empty());
                    w.name = name.trim().to_owned()
                }
                for t in &mut w.tabs {
                    if t.id == id {
                        t.name = name.clone()
                    }
                }
            }
        }
        Request::Close { pane } => {
            stop_pane(s, &pane)?;
            if !s.state.panes.iter().any(|p| p.id == pane) {
                bail!("pane not found")
            }
            crate::coordination::Board::open()?.close_pane(&pane)?;
            s.agents.retain(|a| a["pane"] != pane);
            s.change_handle(pane.clone(), None);
            s.state.panes.retain(|p| p.id != pane);
            for w in &mut s.state.workspaces {
                if w.coordinator.as_ref().is_some_and(|a| a["pane"] == pane) {
                    w.coordinator = None;
                }
            }
            remove_pane_from_layout(&mut s.state, &pane, &crate::config::host_settings());
        }
        req @ Request::Acknowledge { .. }
        | req @ Request::Mouse { .. }
        | req @ Request::Key { .. }
        | req @ Request::Paste { .. }
        | req @ Request::Status { .. }
        | req @ Request::Resize { .. }
        | req @ Request::Send { .. }
        | req @ Request::Capture { .. } => {
            let pane = match &req {
                Request::Acknowledge { pane }
                | Request::Mouse { pane, .. }
                | Request::Key { pane, .. }
                | Request::Paste { pane, .. }
                | Request::Status { pane, .. }
                | Request::Resize { pane, .. }
                | Request::Send { pane, .. }
                | Request::Capture { pane }
                | Request::Close { pane } => pane,
                _ => unreachable!(),
            };
            let h = s.panes.get(pane).context("pane is stopped or missing")?;
            let msg = ask(h, req.clone())?;
            if let Request::Status { ref state, .. } = req
                && let Some(p) = s.state.panes.iter_mut().find(|p| &p.id == pane)
            {
                p.status = state.clone();
            }
            return Ok(msg);
        }
        _ => bail!("unsupported request"),
    }
    refresh_pane_metadata(s);
    update_workspace_names(&mut s.state);
    s.state.revision += 1;
    save(s)?;
    Ok(Message::State(s.state.clone()))
}
pub fn private_dir(dir: &std::path::Path) -> Result<()> {
    if !dir.exists() {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(dir)?;
    }
    let m = fs::symlink_metadata(dir)?;
    if !m.is_dir()
        || m.file_type().is_symlink()
        || m.uid() != unsafe { libc::geteuid() }
        || m.mode() & 0o077 != 0
    {
        bail!("state/runtime directory must be an owned private directory (0700)")
    }
    Ok(())
}
fn validate_initial_tab(s: &Server, location: &WorkspaceLocation, tab: &InitialTab) -> Result<()> {
    let root = match location {
        WorkspaceLocation::Directory { .. } => None,
        WorkspaceLocation::Worktree { workspace, .. } => {
            Some(crate::workspaces::owner(&s.state, workspace)?)
        }
    };
    if let InitialTab::Agent {
        provider,
        role,
        parent,
        complex,
        ..
    } = tab
    {
        if !["claude", "codex"].contains(&provider.as_str())
            || !["worker", "coordinator", "reviewer"].contains(&role.as_str())
        {
            bail!("Invalid initial agent provider or role");
        }
        if *complex && role != "worker" {
            bail!("Complex work is only available for workers");
        }
        if !crate::config::agent_enabled(provider) {
            bail!("{provider} integration is disabled in Settings");
        }
        crate::coordination::provider_executable(provider)?;
        let board = crate::coordination::Board::open()?;
        if let Some(parent) = parent {
            let owner = board.get(parent)?;
            if owner["role"] != "coordinator"
                || owner["closed"] == true
                || root.as_ref().is_none_or(|root| owner["root"] != *root)
            {
                bail!("Parent must be an active coordinator in the owning workspace");
            }
        }
        if crate::coordination::is_worker_role(role)
            && let Some(root) = root
        {
            let workers = board
                .agents()?
                .iter()
                .filter(|a| {
                    a["root"] == root
                        && crate::coordination::is_worker_role(a["role"].as_str().unwrap_or(""))
                        && a["paused"] != true
                })
                .count();
            if workers >= crate::config::host_settings().max_agents_per_coordinator {
                bail!("Worker limit reached. Pause an idle worker first");
            }
        }
    }
    Ok(())
}

fn create_with_initial_tab(
    server: &Arc<Mutex<Server>>,
    location: WorkspaceLocation,
    initial: InitialTab,
) -> Result<Message> {
    let (path, name, owner) = match &location {
        WorkspaceLocation::Directory { path, name } => {
            (fs::canonicalize(path)?, name.clone(), None)
        }
        WorkspaceLocation::Worktree {
            workspace,
            branch,
            base,
            name,
            path,
        } => {
            let (source, owner) = {
                let s = server.lock().unwrap();
                validate_initial_tab(&s, &location, &initial)?;
                let w = s
                    .state
                    .workspaces
                    .iter()
                    .find(|w| w.id == *workspace)
                    .context("Workspace missing")?;
                (
                    crate::workspaces::active_path(&s.state, w),
                    crate::workspaces::owner(&s.state, workspace)?,
                )
            };
            let path = crate::worktree::create(
                std::path::Path::new(&source),
                branch,
                base,
                path.as_deref(),
            )?;
            (path, name.trim().to_owned(), Some(owner))
        }
    };
    if !path.is_dir() {
        bail!("Workspace must be a directory");
    }
    let identity = crate::worktree::identify(&path);
    let path = path.to_string_lossy().into_owned();
    let mut s = server.lock().unwrap();
    if s.draining {
        bail!("Daemon is preparing an idle upgrade");
    }
    validate_initial_tab(&s, &location, &initial)?;
    let workspace = id();
    let automatic = name.trim().is_empty();
    s.state.workspaces.push(Workspace {
        id: workspace.clone(),
        coordinator: None,
        parent: owner,
        context: None,
        auto_name: Some(automatic),
        active_pane: None,
        last_cwd: None,
        name: if automatic { folder_name(&path) } else { name },
        path: path.clone(),
        tabs: vec![],
        repository: identity.as_ref().map(|x| x.0.clone()),
        main_path: identity.as_ref().map(|x| x.1.clone()),
        branch: identity.map(|x| x.2),
    });
    let result = (|| -> Result<(Option<String>, String, Option<String>)> {
        match initial {
            InitialTab::Terminal { name } => {
                let pane = new_pane(&mut s, path, workspace.clone())?;
                let tab = id();
                s.state
                    .workspaces
                    .iter_mut()
                    .find(|w| w.id == workspace)
                    .unwrap()
                    .tabs
                    .push(Tab {
                        id: tab.clone(),
                        agent_role: None,
                        name: name
                            .filter(|n| !n.trim().is_empty())
                            .unwrap_or_else(|| "Tab 1".into()),
                        root: Node::Leaf(pane.clone()),
                    });
                Ok((Some(tab), pane, None))
            }
            InitialTab::Agent {
                provider,
                role,
                parent,
                complex,
                name,
            } => {
                let Message::Coordination(result) = request(
                    &mut s,
                    Request::Coordination(
                        serde_json::json!({"action":"start","workspace":workspace,"provider":provider,"role":role,"parent":parent,"complex":complex}),
                    ),
                )?
                else {
                    bail!("Initial agent did not start");
                };
                let agent = result["agent"]["id"]
                    .as_str()
                    .context("Missing initial agent ID")?
                    .to_owned();
                let pane = result["agent"]["pane"]
                    .as_str()
                    .context("Missing initial pane ID")?
                    .to_owned();
                let tab = s
                    .state
                    .workspaces
                    .iter_mut()
                    .find(|w| w.id == workspace)
                    .unwrap()
                    .tabs
                    .first_mut()
                    .map(|tab| {
                        if let Some(name) = name.filter(|n| !n.trim().is_empty()) {
                            tab.name = name;
                        }
                        tab.id.clone()
                    });
                if role != "coordinator" && tab.is_none() {
                    bail!("Initial agent tab is missing");
                }
                Ok((tab, pane, Some(agent)))
            }
        }
    })();
    let (tab, pane, agent) = match result {
        Ok(created) => created,
        Err(error) => {
            // A launched pane is recoverable; preserve it and any created checkout.
            s.state
                .workspaces
                .retain(|w| w.id != workspace || !w.tabs.is_empty() || w.coordinator.is_some());
            return Err(error).with_context(|| {
                format!(
                    "Could not initialize workspace at {}",
                    s.state
                        .workspaces
                        .iter()
                        .find(|w| w.id == workspace)
                        .map(|w| w.path.as_str())
                        .unwrap_or("the requested directory")
                )
            });
        }
    };
    s.state
        .workspaces
        .iter_mut()
        .find(|w| w.id == workspace)
        .unwrap()
        .active_pane = Some(pane.clone());
    s.state.revision += 1;
    save(&s)?;
    Ok(Message::CreatedWorkspace {
        workspace,
        tab,
        pane,
        agent,
    })
}

static GIT_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
fn open_checkout(
    server: &Arc<Mutex<Server>>,
    path: &str,
    name: &str,
    owner: Option<&str>,
    proposed: Option<&str>,
) -> Result<String> {
    let path = std::fs::canonicalize(path)?;
    let repo = crate::worktree::inspect(&path)?;
    if !repo
        .checkouts
        .iter()
        .any(|c| std::path::Path::new(&c.path) == path)
    {
        bail!("Not a registered checkout")
    }
    let mut s = server.lock().unwrap();
    let owner = if let Some(owner) = owner {
        crate::workspaces::owner(&s.state, owner)?
    } else if let Some(main) = s
        .state
        .workspaces
        .iter()
        .find(|w| w.parent.is_none() && w.path == repo.main_path)
    {
        main.id.clone()
    } else {
        create_workspace(&mut s, repo.main_path.clone(), String::new(), false, None)?
    };
    let path = path.to_string_lossy().into_owned();
    if let Some(existing) = s
        .state
        .workspaces
        .iter()
        .find(|w| w.path == path && (w.parent.as_ref() == Some(&owner) || w.id == owner))
    {
        return Ok(existing.id.clone());
    }
    create_workspace(&mut s, path, name.trim().to_owned(), false, proposed)?;
    let child = s.state.workspaces.last_mut().unwrap();
    child.parent = Some(owner);
    let id = child.id.clone();
    s.state.revision += 1;
    save(&s)?;
    Ok(id)
}
fn host_info() -> Result<Message> {
    Ok(Message::HostInfo {
        release_version: Some(crate::RELEASE_VERSION.into()),
        home: std::env::var("HOME")?,
        protocol: VERSION.into(),
        build: env!("JAM_BUILD_ID").into(),
        engine: "ghostty/483ea3d:text".into(),
        incarnation: incarnation().into(),
    })
}

// Terminal traffic must not wait for an unrelated pane's actor or a metadata read.
fn input_pane(request: &Request) -> Option<&str> {
    match request {
        Request::Key { pane, .. }
        | Request::Send { pane, .. }
        | Request::Paste { pane, .. }
        | Request::Mouse { pane, .. }
        | Request::Capture { pane }
        | Request::Acknowledge { pane } => Some(pane),
        _ => None,
    }
}
fn live_handle(routes: &Mutex<PaneRoutes>, pane: &str) -> Result<Handle> {
    let routes = routes.lock().unwrap();
    if routes.draining {
        bail!("Daemon is preparing an upgrade");
    }
    routes
        .panes
        .get(pane)
        .cloned()
        .context("pane is stopped or missing")
}
fn request_input(routes: &Mutex<PaneRoutes>, input: Request) -> Message {
    let result = (|| {
        let handle = live_handle(
            routes,
            input_pane(&input).context("Invalid terminal input")?,
        )?;
        ask(&handle, input)
    })();
    result.unwrap_or_else(|e| Message::Error(format!("{e:#}")))
}

pub fn serve() -> Result<()> {
    let sock = socket();
    let parent = sock.parent().context("socket parent")?;
    private_dir(parent)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(parent.join("server.lock"))?;
    lock.try_lock_exclusive().context("jamd already running")?;
    lock.set_permissions(fs::Permissions::from_mode(0o600))?;
    if sock.exists() {
        fs::remove_file(&sock)?;
    }
    let mut state: State = match fs::read(state_dir().join("layout.json")) {
        Ok(bytes) => serde_json::from_slice(&bytes)
            .context("invalid saved layout. Preserve it before repairing")?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => State::default(),
        Err(e) => return Err(e.into()),
    };
    for p in &mut state.panes {
        p.exited = true;
        p.session_generation = None;
        p.status = "unknown".into();
        p.reports.clear();
        p.agent_provider = None;
        p.agent_source = None;
        p.agent_resume = None;
        p.agent_role = None;
        if let Some(path) = cwd_path(&p.cwd) {
            p.cwd = path;
        }
    }
    if !state.workspaces.is_empty() && !state_dir().join("layout-v1.backup.json").exists() {
        std::fs::copy(
            state_dir().join("layout.json"),
            state_dir().join("layout-v1.backup.json"),
        )?;
    }
    for w in &mut state.workspaces {
        if let Some((repo, main, branch)) = crate::worktree::identify(std::path::Path::new(&w.path))
        {
            w.repository = Some(repo);
            w.main_path = Some(main);
            w.branch = Some(branch);
        }
    }
    for w in &state.workspaces {
        let mut ids = Vec::new();
        for t in &w.tabs {
            t.root.ids(&mut ids);
        }
        for p in &mut state.panes {
            if ids.contains(&p.id) {
                p.workspace = w.id.clone();
            }
        }
    }
    update_workspace_names(&mut state);
    crate::workspaces::migrate(&mut state);
    let upgrade_boot = upgrade_boot_hold(&state_dir().join(UPGRADE_BOOT_FILE))?;
    private_dir(&state_dir())?;
    let monitor_board = crate::coordination::Board::open()?;
    let mut agents = monitor_board.agents()?;
    for agent in &mut agents {
        // Legacy closed tabs were recorded as paused, with no corresponding pane.
        if agent["paused"] == true
            && agent["closed"] != true
            && !state.panes.iter().any(|p| agent["pane"] == p.id)
        {
            agent["closed"] = true.into();
            monitor_board.put("agent", agent)?;
        }
    }
    let server = Arc::new(Mutex::new(Server {
        agents,
        contexts: crate::workspaces::RepositoryContexts::default(),
        owners: HashMap::new(),
        draining: upgrade_boot.is_some(),
        upgrade_boot,
        upgrade_lease: None,
        coordination_inflight: 0,
        state,
        panes: Arc::default(),
        routes: Arc::default(),
        operations: HashMap::new(),
    }));
    let routes = {
        let mut s = server.lock().unwrap();
        let draining = s.draining;
        s.set_draining(draining);
        s.routes.clone()
    };
    refresh_pane_metadata(&mut server.lock().unwrap());
    save(&server.lock().unwrap())?;
    let listener = UnixListener::bind(&sock)?;
    fs::set_permissions(&sock, fs::Permissions::from_mode(0o600))?;
    // Stable metadata is readable without entering the request/state lock.
    let metadata = parent.join(format!("release-{}.tmp", crate::id()));
    fs::write(
        &metadata,
        serde_json::to_vec(&serde_json::json!({
            "release_version": crate::RELEASE_VERSION,
            "protocol": VERSION, "build": crate::SERVER_BUILD,
            "pid": std::process::id(),
        }))?,
    )?;
    fs::set_permissions(&metadata, fs::Permissions::from_mode(0o600))?;
    fs::rename(&metadata, parent.join("release.json"))?;
    crate::coordination::start_delivery()?;
    {
        let mut s = server.lock().unwrap();
        if s.upgrade_boot.is_none() {
            restart_saved_agents(&mut s, None)?;
        }
    }
    let monitor = server.clone();
    thread::spawn(move || {
        loop {
            thread::sleep(std::time::Duration::from_millis(500));
            let Ok(agents) = monitor_board.agents() else {
                continue;
            };
            let mut s = monitor.lock().unwrap();

            refresh_upgrade_lease(&mut s);
            if s.draining {
                continue;
            }
            for a in agents {
                let Some(pane) = a["pane"].as_str() else {
                    continue;
                };
                if a["launch_error"].is_string() {
                    continue;
                }
                if crate::recovery::should_restart(&a)
                    && s.panes
                        .get(pane)
                        .is_some_and(|h| h.info.lock().unwrap().exited)
                {
                    let _ = request(&mut s, Request::Restart { pane: pane.into() });
                }
            }
        }
    });
    // Limit outstanding handshakes and ordinary RPCs. Attached panes and
    // ordered input streams release their slot once they become subscriptions.
    let database = crate::coordination::BoardWorker::start()?;
    struct Slot(Sender<()>);
    impl Drop for Slot {
        fn drop(&mut self) {
            let _ = self.0.try_send(());
        }
    }
    fn pool(size: usize) -> (Sender<()>, crossbeam_channel::Receiver<()>) {
        let (tx, rx) = bounded(size);
        for _ in 0..size {
            tx.send(()).unwrap();
        }
        (tx, rx)
    }
    let (slots, available) = pool(64);
    // Reserve admission for health checks and pane traffic during a backlog of
    // layout or database requests. Rejected RPCs have not started executing.
    let (ordinary_slots, ordinary_available) = pool(48);
    for stream in listener.incoming() {
        let mut stream = stream?;
        available.recv()?;
        let slot = Slot(slots.clone());
        let ordinary_slots = ordinary_slots.clone();
        let ordinary_available = ordinary_available.clone();
        let database = database.clone();
        let routes = routes.clone();
        let server = server.clone();
        thread::spawn(move || {
            let result = (|| -> Result<()> {
                use std::os::fd::AsRawFd;
                #[cfg(target_os = "macos")]
                {
                    let (mut uid, mut gid) = (0, 0);
                    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0
                        || uid != unsafe { libc::geteuid() }
                    {
                        bail!("peer user mismatch")
                    }
                }
                #[cfg(target_os = "linux")]
                {
                    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
                    let mut len = std::mem::size_of_val(&cred) as libc::socklen_t;
                    if unsafe {
                        libc::getsockopt(
                            stream.as_raw_fd(),
                            libc::SOL_SOCKET,
                            libc::SO_PEERCRED,
                            (&mut cred as *mut libc::ucred).cast(),
                            &mut len,
                        )
                    } != 0
                        || cred.uid != unsafe { libc::geteuid() }
                    {
                        bail!("peer user mismatch")
                    }
                }
                stream.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
                stream.set_write_timeout(Some(std::time::Duration::from_secs(10)))?;
                write_frame(&mut stream, &Message::Hello(VERSION.into()))?;
                let incoming: Request = read_frame(&mut stream)?;
                let (client, r) = match incoming {
                    Request::Client { client, request } => (client, *request),
                    request => (String::new(), request),
                };
                if matches!(r, Request::HostInfo) {
                    write_frame(&mut stream, &host_info()?)?;
                    return Ok(());
                }
                if input_pane(&r).is_some() {
                    write_frame(&mut stream, &request_input(&routes, r))?;
                    return Ok(());
                }
                if matches!(r, Request::InputStream) {
                    if routes.lock().unwrap().draining {
                        bail!("Daemon is preparing an upgrade");
                    }
                    drop(slot);
                    stream.set_read_timeout(None)?;
                    write_frame(&mut stream, &Message::Ok)?;
                    loop {
                        let incoming: Request = read_frame(&mut stream)?;
                        let request = match incoming {
                            Request::Client { request, .. } => *request,
                            r => r,
                        };
                        if !matches!(
                            request,
                            Request::Key { .. }
                                | Request::Send { .. }
                                | Request::Paste { .. }
                                | Request::Mouse { .. }
                        ) {
                            bail!("Invalid input stream request");
                        }
                        let reply = request_input(&routes, request);
                        write_frame(&mut stream, &reply)?;
                    }
                }
                if let Request::Attach { pane } = r {
                    let h = live_handle(&routes, &pane)?;
                    let (tx, rx) = bounded(128);
                    h.tx.send_timeout(Op::Attach(tx), std::time::Duration::from_secs(2))
                        .map_err(pane_send_error)?;
                    stream.set_write_timeout(Some(std::time::Duration::from_secs(30)))?;
                    drop(slot);
                    let (gone, disconnected) = bounded(1);
                    let mut watch = stream.try_clone()?;
                    watch.set_read_timeout(None)?;
                    thread::spawn(move || {
                        let mut byte = [0];
                        let _ = watch.read(&mut byte);
                        let _ = gone.send(());
                    });
                    let result = (|| -> Result<()> {
                        loop {
                            crossbeam_channel::select! {
                                recv(disconnected) -> _ => break,
                                recv(rx) -> message => match message { Ok(m)=>write_frame(&mut stream,&m)?, Err(_)=>break },
                            }
                        }
                        Ok(())
                    })();
                    let _ = stream.shutdown(std::net::Shutdown::Both);
                    result?;
                    return Ok(());
                }
                if ordinary_available.try_recv().is_err() {
                    write_frame(
                        &mut stream,
                        &Message::Error("Server is busy. Try again.".into()),
                    )?;
                    return Ok(());
                }
                let _ordinary = Slot(ordinary_slots);
                {
                    let mut state = server.lock().unwrap();
                    refresh_upgrade_lease(&mut state);
                    if state.draining && !upgrade_control(&r) {
                        write_frame(
                            &mut stream,
                            &Message::Error("Daemon is preparing an upgrade".into()),
                        )?;
                        return Ok(());
                    }
                }
                if matches!(r, Request::AcquireGui) {
                    write_frame(&mut stream, &Message::Ok)?;
                    return Ok(());
                }
                if let Request::FocusPane { ref pane } = r {
                    server
                        .lock()
                        .unwrap()
                        .owners
                        .insert(pane.clone(), client.clone());
                }
                if let Request::Resize { ref pane, .. } = r {
                    let mut state = server.lock().unwrap();
                    let owner = state.owners.entry(pane.clone()).or_insert(client.clone());
                    if *owner != client {
                        write_frame(&mut stream, &Message::Ok)?;
                        return Ok(());
                    }
                }
                if matches!(r, Request::State) {
                    let (revision, panes) = {
                        let s = server.lock().unwrap();
                        (
                            s.state.revision,
                            s.state
                                .panes
                                .iter()
                                .map(|p| p.id.clone())
                                .collect::<std::collections::HashSet<_>>(),
                        )
                    };
                    let pending_agents = database.submit(move |board| {
                        let mut agents = board.agents()?;
                        for agent in &mut agents {
                            if agent["archived"] == true
                                && agent["paused"] == true
                                && agent["closed"] != true
                                && agent["pane"].as_str().is_some_and(|p| !panes.contains(p))
                            {
                                board.update_agent(
                                    agent["id"].as_str().context("Missing agent ID")?,
                                    |a| {
                                        if a["paused"] == true {
                                            a["closed"] = true.into();
                                        }
                                    },
                                )?;
                                *agent =
                                    board.get(agent["id"].as_str().context("Missing agent ID")?)?;
                            }
                        }
                        Ok(agents)
                    });
                    // Layout uses the most recent committed snapshot while the writer
                    // is busy. A slow coordination transaction must not freeze polling.
                    let agents = pending_agents
                        .ok()
                        .and_then(|pending| {
                            pending
                                .recv_timeout(std::time::Duration::from_millis(80))
                                .ok()
                        })
                        .transpose()?;
                    let reply = {
                        let mut s = server.lock().unwrap();
                        if s.state.revision == revision
                            && let Some(agents) = agents
                        {
                            s.agents = agents;
                        }
                        request(&mut s, Request::State)?
                    };
                    write_frame(&mut stream, &reply)?;
                    return Ok(());
                }
                if let Request::AgentModels {
                    ref provider,
                    refresh,
                } = r
                {
                    write_frame(
                        &mut stream,
                        &Message::AgentModels(crate::model_catalog::load(provider, refresh)?),
                    )?;
                    return Ok(());
                }
                if let Request::Coordination(ref value) = r {
                    let reply = (|| -> Result<Message> {
                        if value["action"] == "start" || value["action"] == "open" {
                            return request(&mut server.lock().unwrap(), r.clone());
                        }
                        let admission = CoordinationAdmission::acquire(&server)?;
                        let result = (|| -> Result<Message> {
                            if !matches!(
                                value["action"].as_str(),
                                Some(
                                    "register"
                                        | "instructions"
                                        | "sessions"
                                        | "recover"
                                        | "pause"
                                        | "resume"
                                )
                            ) {
                                let value = value.clone();
                                return database
                                    .run(move |board| board.action(value))
                                    .map(Message::Coordination);
                            }
                            let mut board = crate::coordination::Board::open()?;
                            if value["action"] == "sessions" {
                                let agent =
                                    board.get(value["agent"].as_str().context("Missing agent")?)?;
                                return Ok(Message::Coordination(crate::codex_session::sessions(
                                    &agent,
                                )?));
                            }
                            if value["action"] == "register"
                                && let Some(pane) = value["pane"].as_str()
                                && board.agent_for_pane(pane)?.is_none()
                            {
                                let s = server.lock().unwrap();
                                let p = s
                                    .state
                                    .panes
                                    .iter()
                                    .find(|p| p.id == pane)
                                    .context("Registration requires a live Jam pane")?;
                                let root = crate::workspaces::owner(&s.state, &p.workspace)?;
                                let mut a = board.add(crate::coordination::AgentSpec {
                                    workspace: &p.workspace,
                                    root: &root,
                                    pane,
                                    provider: value["provider"].as_str().unwrap_or("claude"),
                                    role: "worker",
                                    path: &p.cwd,
                                    parent: None,
                                    complex: false,
                                })?;
                                a["verified_launch"] = serde_json::json!(false);
                                board.put("agent", &a)?;
                            }
                            if value["action"] == "recover" {
                                let agent =
                                    board.get(value["agent"].as_str().context("Missing agent")?)?;
                                let mut s = server.lock().unwrap();
                                if agent["pane"]
                                    .as_str()
                                    .and_then(|id| s.panes.get(id))
                                    .is_some_and(|h| !h.info.lock().unwrap().exited)
                                {
                                    bail!(
                                        "Stop the coordinator before recovering its conversation"
                                    );
                                }
                                let result = board.action(value.clone())?;
                                let pane = result["pane"].as_str().context("Missing agent pane")?;
                                request(&mut s, Request::Restart { pane: pane.into() })?;
                                return Ok(Message::Coordination(result));
                            }
                            let result = board.action(value.clone())?;
                            if value["action"] == "pause"
                                && let Some(pane) = result["pane"].as_str()
                            {
                                let mut state = server.lock().unwrap();
                                stop_pane(&state, pane)?;
                                if let Some(handle) = state.change_handle(pane.into(), None)
                                    && let Some(saved) =
                                        state.state.panes.iter_mut().find(|p| p.id == pane)
                                {
                                    *saved = handle.info.lock().unwrap().clone();
                                    saved.exited = true;
                                }
                            }
                            if value["action"] == "resume"
                                && let Some(pane) = result["pane"].as_str()
                            {
                                request(
                                    &mut server.lock().unwrap(),
                                    Request::Restart { pane: pane.into() },
                                )?;
                            }
                            Ok(Message::Coordination(result))
                        })();
                        drop(admission);
                        result
                    })()
                    .unwrap_or_else(|e| Message::Error(format!("{e:#}")));
                    write_frame(&mut stream, &reply)?;
                    return Ok(());
                }
                if let Request::AuthorizeUsage { ref profile } = r {
                    let reply = match crate::usage::authorize(profile) {
                        Ok(()) => Message::Ok,
                        Err(error) => Message::Error(error.to_string()),
                    };
                    write_frame(&mut stream, &reply)?;
                    return Ok(());
                }
                if matches!(r, Request::AgentUsage) {
                    let roots = {
                        let s = server.lock().unwrap();
                        s.panes
                            .values()
                            .filter(|h| !h.info.lock().unwrap().exited)
                            .map(|h| h.root_pid.load(std::sync::atomic::Ordering::Relaxed))
                            .collect()
                    };
                    write_frame(
                        &mut stream,
                        &Message::AgentUsage(crate::usage::snapshot(roots)),
                    )?;
                    return Ok(());
                }
                if let Request::GitSummary { ref workspace } = r {
                    let path = {
                        let s = server.lock().unwrap();
                        let w = s
                            .state
                            .workspaces
                            .iter()
                            .find(|w| &w.id == workspace)
                            .context("Workspace missing")?;
                        if w.parent.is_some() {
                            w.path.clone()
                        } else {
                            crate::workspaces::active_path(&s.state, w)
                        }
                    };
                    let path = std::path::Path::new(&path);
                    let mut summary = crate::worktree::diff_summary(path)?;
                    summary.review = crate::reviews::lookup(path);
                    write_frame(&mut stream, &Message::GitSummary(summary))?;
                    return Ok(());
                }
                if let Request::Worktrees { ref workspace } = r {
                    let path = {
                        let s = server.lock().unwrap();
                        let w = s
                            .state
                            .workspaces
                            .iter()
                            .find(|w| &w.id == workspace)
                            .context("Workspace missing")?;
                        crate::workspaces::active_path(&s.state, w)
                    };
                    write_frame(
                        &mut stream,
                        &Message::Worktrees(crate::worktree::inspect(std::path::Path::new(&path))?),
                    )?;
                    return Ok(());
                }
                if let Request::CreateWorkspaceWithTab { location, tab } = r.clone() {
                    let operation = id();
                    let initial = Message::Operation {
                        id: operation.clone(),
                        done: false,
                        error: None,
                        workspace: None,
                    };
                    {
                        let mut s = server.lock().unwrap();
                        if s.draining {
                            bail!("Daemon is preparing an idle upgrade");
                        }
                        validate_initial_tab(&s, &location, &tab)?;
                        s.operations.insert(operation.clone(), initial.clone());
                    }
                    let shared = server.clone();
                    thread::spawn(move || {
                        let _git = GIT_LOCK.lock().unwrap();
                        let result = create_with_initial_tab(&shared, location, tab)
                            .unwrap_or_else(|error| Message::Error(format!("{error:#}")));
                        shared.lock().unwrap().operations.insert(operation, result);
                    });
                    write_frame(&mut stream, &initial)?;
                    return Ok(());
                }
                let (creation_id, r) = match r {
                    Request::WorkspaceCreation {
                        id,
                        incarnation: expected,
                        request,
                    } => {
                        uuid::Uuid::parse_str(&id).context("Invalid workspace creation ID")?;
                        if expected != incarnation() {
                            bail!(
                                "Server restarted during workspace creation. Check the workspace list."
                            );
                        }
                        if !matches!(
                            *request,
                            Request::NewWorkspace { .. }
                                | Request::CreateWorkspace { .. }
                                | Request::WorktreeCreate { .. }
                                | Request::WorktreeOpen { .. }
                        ) {
                            bail!("Invalid workspace creation request");
                        }
                        (Some(id), *request)
                    }
                    request => (None, request),
                };
                if creation_id.is_some()
                    || matches!(
                        r,
                        Request::WorktreeCreate { .. }
                            | Request::WorktreeOpen { .. }
                            | Request::WorktreeRemove { .. }
                    )
                {
                    let op = creation_id.clone().unwrap_or_else(id);
                    let initial = Message::Operation {
                        id: op.clone(),
                        done: false,
                        error: None,
                        workspace: None,
                    };
                    let creation_context = {
                        let mut s = server.lock().unwrap();
                        if s.draining {
                            bail!("Daemon is preparing an idle upgrade");
                        }
                        if let Some(existing) = s.operations.get(&op).cloned() {
                            drop(s);
                            write_frame(&mut stream, &existing)?;
                            return Ok(());
                        }
                        if s.state.workspaces.iter().any(|w| w.id == op) {
                            bail!("Workspace creation ID already exists");
                        }
                        let context = if let Request::WorktreeCreate { workspace, .. } = &r {
                            let w = s
                                .state
                                .workspaces
                                .iter()
                                .find(|w| &w.id == workspace)
                                .context("Workspace missing")?;
                            Some((
                                crate::workspaces::active_path(&s.state, w),
                                crate::workspaces::owner(&s.state, workspace)?,
                            ))
                        } else {
                            None
                        };
                        s.operations.insert(op.clone(), initial.clone());
                        context
                    };
                    let shared = server.clone();
                    let directory_name = match &r {
                        Request::CreateWorkspace { name, .. } => Some(name.clone()),
                        _ => None,
                    };
                    thread::spawn(move || {
                        let _git = GIT_LOCK.lock().unwrap();
                        let result = (|| -> Result<String> {
                            match r {
                                Request::NewWorkspace { path }
                                | Request::CreateWorkspace { path, .. } => {
                                    let mut s = shared.lock().unwrap();
                                    let workspace = create_workspace(
                                        &mut s,
                                        path,
                                        directory_name.clone().unwrap_or_default(),
                                        directory_name.is_some(),
                                        creation_id.as_deref(),
                                    )?;
                                    s.state.revision += 1;
                                    save(&s)?;
                                    Ok(workspace)
                                }
                                Request::WorktreeCreate {
                                    workspace: _,
                                    branch,
                                    base,
                                    name,
                                    path,
                                } => {
                                    let (source, owner) =
                                        creation_context.context("Missing creation context")?;
                                    let target = crate::worktree::create(
                                        std::path::Path::new(&source),
                                        &branch,
                                        &base,
                                        path.as_deref(),
                                    )?;
                                    open_checkout(
                                        &shared,
                                        &target.to_string_lossy(),
                                        &name,
                                        Some(&owner),
                                        creation_id.as_deref(),
                                    )
                                }
                                Request::WorktreeOpen { path, workspace } => open_checkout(
                                    &shared,
                                    &path,
                                    "",
                                    workspace.as_deref(),
                                    creation_id.as_deref(),
                                ),
                                Request::WorktreeRemove { workspace, force } => {
                                    if !crate::config::host_settings().allow_workspace_deletion {
                                        bail!("Deleting checkout files is disabled in Settings")
                                    }
                                    let path = shared
                                        .lock()
                                        .unwrap()
                                        .state
                                        .workspaces
                                        .iter()
                                        .find(|w| w.id == workspace)
                                        .context("Workspace missing")?
                                        .path
                                        .clone();
                                    if shared
                                        .lock()
                                        .unwrap()
                                        .state
                                        .workspaces
                                        .iter()
                                        .any(|w| w.parent.as_ref() == Some(&workspace))
                                    {
                                        bail!(
                                            "Close this workspace's open worktrees before deleting its checkout"
                                        );
                                    }
                                    if shared
                                        .lock()
                                        .unwrap()
                                        .state
                                        .workspaces
                                        .iter()
                                        .any(|w| w.id != workspace && w.path == path)
                                    {
                                        bail!(
                                            "Checkout is open in another workspace. Close those sessions before deleting it"
                                        );
                                    }
                                    // Preflight before stopping sessions; Git repeats safety checks at removal.
                                    let repo =
                                        crate::worktree::inspect(std::path::Path::new(&path))?;
                                    let c = repo
                                        .checkouts
                                        .iter()
                                        .find(|c| c.path == path)
                                        .context("Checkout missing")?;
                                    if c.main || c.locked {
                                        bail!("Primary or locked checkout cannot be deleted")
                                    }
                                    if !force
                                        && !crate::worktree::is_clean(std::path::Path::new(&path))?
                                    {
                                        bail!(
                                            "Checkout has local changes. Force confirmation is required"
                                        )
                                    }
                                    let state = shared.lock().unwrap();
                                    let panes: Vec<_> = state
                                        .state
                                        .panes
                                        .iter()
                                        .filter(|p| p.workspace == workspace)
                                        .map(|p| p.id.clone())
                                        .collect();
                                    for pane in panes {
                                        stop_pane(&state, &pane)?;
                                    }
                                    drop(state);
                                    crate::worktree::remove(std::path::Path::new(&path), force)?;
                                    let mut state = shared.lock().unwrap();
                                    request(
                                        &mut state,
                                        Request::WorkspaceClose {
                                            workspace: workspace.clone(),
                                            group: false,
                                        },
                                    )?;
                                    Ok(workspace)
                                }
                                _ => unreachable!(),
                            }
                        })();
                        let (workspace, error) = match result {
                            Ok(w) => (Some(w), None),
                            Err(e) => (None, Some(e.to_string())),
                        };
                        shared.lock().unwrap().operations.insert(
                            op.clone(),
                            Message::Operation {
                                id: op,
                                done: true,
                                error,
                                workspace,
                            },
                        );
                    });
                    write_frame(&mut stream, &initial)?;
                    return Ok(());
                }
                if matches!(r, Request::Stop | Request::StopUpgrade { .. }) {
                    let mut s = server.lock().unwrap();
                    if let Request::StopUpgrade { token } = &r {
                        claim_upgrade_stop(&mut s, token)?;
                    }
                    s.set_draining(true);
                    crate::recovery::stopping();
                    for (id, h) in s.panes.iter() {
                        let _ = ask(h, Request::Close { pane: id.clone() });
                    }
                    write_frame(&mut stream, &Message::Ok)?;
                    std::process::exit(0)
                }
                let reply = request(&mut server.lock().unwrap(), r)
                    .unwrap_or_else(|e| Message::Error(format!("{e:#}")));
                write_frame(&mut stream, &reply)?;
                Ok(())
            })();
            if let Err(e) = result {
                let _ = write_frame(&mut stream, &Message::Error(format!("{e:#}")));
            }
        });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_zombie_only_group_eperm_is_verified_before_reaping() {
        let pair = native_pty_system().openpty(PtySize::default()).unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "exit 0"]);
        let mut process = Process::new(pair.slave.spawn_command(command).unwrap());
        drop(pair.slave);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !process.exited_without_reaping().unwrap() {
            assert!(
                std::time::Instant::now() < deadline,
                "fixture root did not exit"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        let pid = process.child.process_id().unwrap() as i32;
        assert_eq!(process.owned_group, Some(pid));
        // Reproduce the hosted failure against an actual unreaped Darwin child.
        assert_eq!(unsafe { libc::killpg(pid, libc::SIGKILL) }, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::EPERM)
        );
        assert!(group_has_no_live_members(pid, pid).unwrap());
        process.terminate().unwrap();
        assert!(process.owned_group.is_none());
        assert!(process.child.try_wait().unwrap().is_some());
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_live_group_member_prevents_empty_group_classification() {
        let pair = native_pty_system().openpty(PtySize::default()).unwrap();
        let mut command = CommandBuilder::new("/bin/sh");
        command.args(["-c", "sleep 60 & wait"]);
        let mut process = Process::new(pair.slave.spawn_command(command).unwrap());
        drop(pair.slave);
        let pid = process.child.process_id().unwrap() as i32;
        // Passing no excluded root lets the real running leader prove that a
        // live group must not be confused with the zombie-only kernel case.
        assert!(!group_has_no_live_members(pid, -1).unwrap());
        process.terminate().unwrap();
    }
    #[test]
    fn upgrade_lease_expiry_reopens_admission_but_cannot_authorize_stop() {
        let mut server = Server {
            agents: vec![],
            contexts: crate::workspaces::RepositoryContexts::default(),
            owners: HashMap::new(),
            draining: true,
            upgrade_boot: None,
            coordination_inflight: 0,
            upgrade_lease: Some(UpgradeLease {
                token: "old-owner".into(),
                expires: std::time::Instant::now() - std::time::Duration::from_secs(1),
            }),
            state: State::default(),
            panes: Arc::default(),
            routes: Arc::default(),
            operations: HashMap::new(),
        };
        assert!(claim_upgrade_stop(&mut server, "old-owner").is_err());
        assert!(!server.draining);
        assert!(server.upgrade_lease.is_none());
        server.set_draining(true);
        server.upgrade_lease = Some(UpgradeLease {
            token: "current-owner".into(),
            expires: std::time::Instant::now() + std::time::Duration::from_secs(120),
        });
        assert!(claim_upgrade_stop(&mut server, "old-owner").is_err());
        assert!(server.draining);
        assert!(claim_upgrade_stop(&mut server, "current-owner").is_ok());
        assert!(server.draining);
        assert!(server.upgrade_lease.is_none());
        refresh_upgrade_lease(&mut server);
        assert!(server.draining, "claimed stop must not reopen admission");
        assert!(claim_upgrade_stop(&mut server, "current-owner").is_err());
    }
    #[test]
    fn legacy_workspace_names_migrate_without_losing_custom_names() {
        let mut state: State = serde_json::from_value(serde_json::json!({
            "revision": 0,
            "workspaces": [
                {"id":"auto", "name":"project", "path":"/repo/project", "tabs":[]},
                {"id":"custom", "name":"Pinned", "path":"/repo/project", "tabs":[]}
            ],
            "panes": [
                {"id":"a", "title":"", "cwd":"/repo/project/src", "status":"unknown", "exited":false,"workspace":"auto"},
                {"id":"b", "title":"", "cwd":"/repo/project/src", "status":"unknown", "exited":false,"workspace":"custom"}
            ]
        })).unwrap();
        update_workspace_names(&mut state);
        assert_eq!(state.workspaces[0].name, "src");
        assert_eq!(state.workspaces[0].auto_name, Some(true));
        assert_eq!(state.workspaces[1].name, "Pinned");
        assert_eq!(state.workspaces[1].auto_name, Some(false));
        let mut restored: State =
            serde_json::from_slice(&serde_json::to_vec(&state).unwrap()).unwrap();
        restored.panes[0].cwd = "/tmp/next".into();
        update_workspace_names(&mut restored);
        assert_eq!(restored.workspaces[0].name, "next");
        assert_eq!(restored.workspaces[1].name, "Pinned");
    }
    #[test]
    fn cwd_reports_are_paths_not_urls() {
        assert_eq!(
            cwd_path("file://host/tmp/project%20name"),
            Some("/tmp/project name".into())
        );
        assert_eq!(cwd_path("file:///tmp/project"), Some("/tmp/project".into()));
        assert_eq!(cwd_path("/tmp/project"), Some("/tmp/project".into()));
        assert_eq!(cwd_path("https://host/file"), None);
    }
}
