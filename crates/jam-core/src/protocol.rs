use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    path::PathBuf,
};
pub const VERSION: &str = "jam/8:ghostty/483ea3d:text";
pub const SERVER_BUILD: &str = env!("JAM_BUILD_ID");
pub const MAX_FRAME: usize = 64 * 1024 * 1024;
#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct StatusRecord {
    pub id: String,
    pub state: String,
    pub app: String,
    pub title: String,
    pub message: String,
    pub kind: String,
    pub progress: Option<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProgressStep {
    pub label: String,
    pub percent: u8,
}
impl ProgressStep {
    pub fn validate(steps: &[Self]) -> Result<()> {
        if steps.len() > 10 {
            bail!("Use at most 10 progress steps");
        }
        for (index, step) in steps.iter().enumerate() {
            if step.label.is_empty()
                || step.label.trim() != step.label
                || step.label.chars().count() > 40
                || step.label.chars().any(char::is_control)
            {
                bail!("Use a short, single-line step label of 1 to 40 characters");
            }
            if step.percent > 100 {
                bail!("Progress must be between 0 and 100");
            }
            if steps[..index]
                .iter()
                .any(|previous| previous.label == step.label)
            {
                bail!("Each progress step needs a distinct label");
            }
        }
        Ok(())
    }
}
#[derive(Clone, Copy, Serialize, Deserialize, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum AgentSource {
    Registered,
    Reported,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct AgentResume {
    pub id: String,
    pub verified_launch: bool,
    pub session: Option<String>,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Pane {
    /// Identity of this live PTY, changed on every successful restart.
    #[serde(default)]
    pub session_generation: Option<String>,
    #[serde(default)]
    pub launch_error: Option<String>,
    #[serde(default)]
    pub agent_provider: Option<String>,
    #[serde(default)]
    pub agent_source: Option<AgentSource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_resume: Option<AgentResume>,
    #[serde(default)]
    pub agent_role: Option<String>,
    pub id: String,
    pub title: String,
    pub cwd: String,
    pub status: String,
    pub exited: bool,
    #[serde(default)]
    pub reports: Vec<StatusRecord>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub progress: Vec<ProgressStep>,
    #[serde(default)]
    pub workspace: String,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Node {
    Empty,
    Leaf(String),
    Split {
        vertical: bool,
        ratio: f32,
        a: Box<Node>,
        b: Box<Node>,
    },
}
impl Node {
    pub fn contains(&self, pane: &str) -> bool {
        match self {
            Self::Empty => false,
            Self::Leaf(id) => id == pane,
            Self::Split { a, b, .. } => a.contains(pane) || b.contains(pane),
        }
    }
    pub fn ids(&self, out: &mut Vec<String>) {
        match self {
            Self::Empty => {}
            Self::Leaf(id) => out.push(id.clone()),
            Self::Split { a, b, .. } => {
                a.ids(out);
                b.ids(out);
            }
        }
    }
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Tab {
    #[serde(default)]
    pub agent_role: Option<String>,
    pub id: String,
    pub name: String,
    pub root: Node,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct Workspace {
    #[serde(default)]
    pub coordinator: Option<serde_json::Value>,
    pub id: String,
    pub name: String,
    pub path: String,
    pub tabs: Vec<Tab>,
    #[serde(default)]
    pub auto_name: Option<bool>,
    #[serde(default)]
    pub active_pane: Option<String>,
    /// Most recently focused terminal directory; retained when the workspace is empty.
    #[serde(default)]
    pub last_cwd: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub main_path: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    /// Explicit owner of an opened checkout; independent of the active shell's cwd.
    #[serde(default)]
    pub parent: Option<String>,
    #[serde(default)]
    pub context: Option<RepositoryContext>,
}
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Debug)]
pub struct RepositoryContext {
    pub path: String,
    pub checkout_path: Option<String>,
    pub ready: bool,
    pub repository: Option<String>,
    pub main_path: Option<String>,
    pub branch: Option<String>,
    pub create_disabled_reason: Option<String>,
}
#[derive(Clone, Default, Serialize, Deserialize, Debug)]
pub struct State {
    #[serde(default)]
    pub workspace_model: u32,
    pub revision: u64,
    pub workspaces: Vec<Workspace>,
    pub panes: Vec<Pane>,
}
#[derive(Clone, Copy, Serialize, Deserialize, Debug)]
pub enum PanePlacement {
    Left,
    Right,
    Above,
    Below,
    Swap,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum PaneDestination {
    Pane {
        pane: String,
        placement: PanePlacement,
    },
    Tab {
        tab: String,
    },
    Workspace {
        workspace: String,
    },
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum WorkspaceLocation {
    Directory {
        path: String,
        name: String,
    },
    Worktree {
        workspace: String,
        branch: String,
        base: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        name: String,
        path: Option<String>,
    },
}
/// Permission settings forwarded to a new standalone agent. Provider credentials,
/// repository trust and command approval rules stay with their original profile.
#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum AgentPermissions {
    FullAccess,
    Codex {
        sandbox: Option<String>,
        approval: Option<String>,
    },
    Claude {
        mode: String,
    },
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum InitialTab {
    Terminal {
        name: Option<String>,
    },
    Agent {
        provider: String,
        role: String,
        #[serde(default)]
        complex: bool,
        parent: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        permissions: Option<AgentPermissions>,
        name: Option<String>,
    },
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Request {
    WorkspaceCreation {
        id: String,
        incarnation: String,
        request: Box<Request>,
    },
    AgentModels {
        provider: String,
        refresh: bool,
    },
    CreateWorkspaceWithTab {
        location: WorkspaceLocation,
        tab: InitialTab,
    },
    Client {
        client: String,
        request: Box<Request>,
    },
    HostInfo,
    Configure {
        settings: crate::config::HostSettings,
    },
    NewPane {
        tab: String,
    },
    CloseTab {
        tab: String,
    },
    PrepareUpgrade,
    PrepareUpgradeApproved {
        token: String,
        incarnation: String,
        build: String,
        panes: Vec<String>,
        sessions: std::collections::BTreeMap<String, String>,
    },
    AbortUpgrade {
        token: String,
    },
    CommitUpgrade {
        token: String,
    },
    StopUpgrade {
        token: String,
    },
    Coordination(serde_json::Value),
    AgentUsage,
    UsageResetCredits {
        profile: String,
        account: String,
    },
    RedeemUsageReset {
        profile: String,
        account: String,
        credit: Option<String>,
        attempt: String,
    },
    AuthorizeUsage {
        profile: String,
    },
    InputStream,
    Guarded {
        revision: u64,
        request: Box<Request>,
    },
    Directory {
        path: String,
    },
    GitSummary {
        workspace: String,
    },
    Worktrees {
        workspace: String,
    },
    WorktreeCreate {
        workspace: String,
        branch: String,
        base: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        name: String,
        path: Option<String>,
    },
    WorktreeOpen {
        path: String,
        #[serde(default)]
        workspace: Option<String>,
    },
    WorktreeRemove {
        workspace: String,
        force: bool,
    },
    WorkspaceClose {
        workspace: String,
        group: bool,
    },
    Operation {
        id: String,
    },
    State,
    Capabilities,
    MovePane {
        pane: String,
        destination: PaneDestination,
    },
    ReorderWorkspaces {
        workspaces: Vec<String>,
    },
    NewWorkspace {
        path: String,
    },
    FocusPane {
        pane: String,
    },
    MoveTab {
        tab: String,
        workspace: String,
        index: usize,
    },
    AcquireGui,
    Acknowledge {
        pane: String,
    },
    CreateWorkspace {
        path: String,
        name: String,
    },
    NewTab {
        workspace: String,
    },
    Split {
        pane: String,
        vertical: bool,
    },
    Rename {
        id: String,
        name: String,
    },
    Restart {
        pane: String,
    },
    Ratio {
        tab: String,
        path: Vec<bool>,
        ratio: f32,
    },
    Close {
        pane: String,
    },
    Resize {
        pane: String,
        cols: u16,
        rows: u16,
    },
    Send {
        pane: String,
        data: Vec<u8>,
    },
    Key {
        #[serde(default)]
        release: bool,
        pane: String,
        key: u32,
        text: String,
        mods: u16,
    },
    Mouse {
        pane: String,
        action: u8,
        button: u8,
        x: f32,
        y: f32,
        cell_width: u32,
        cell_height: u32,
        mods: u16,
    },
    Paste {
        pane: String,
        text: String,
    },
    FilePreview {
        pane: String,
        path: String,
        read: crate::file_preview::PreviewRead,
    },
    EditPreviewFile {
        pane: String,
        path: String,
    },
    StoreClipboardImage {
        pane: String,
        png: String,
    },
    Capture {
        pane: String,
    },
    Attach {
        pane: String,
    },
    TaskProgress {
        pane: String,
        steps: Vec<ProgressStep>,
    },
    Status {
        pane: String,
        state: String,
    },
    Stop,
}
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum Message {
    FilePreview(crate::file_preview::PreviewFile),
    AgentModels(crate::model_catalog::Catalog),
    CreatedWorkspace {
        workspace: String,
        /// Coordinators have a dedicated surface and no tab.
        tab: Option<String>,
        pane: String,
        agent: Option<String>,
    },
    HostInfo {
        #[serde(default)]
        release_version: Option<String>,
        home: String,
        protocol: String,
        build: String,
        engine: String,
        incarnation: String,
    },
    Directory {
        path: String,
        directories: Vec<String>,
    },
    GitSummary(crate::worktree::GitSummary),
    Coordination(serde_json::Value),
    AgentUsage(Vec<crate::usage::Report>),
    UsageResetCredits(crate::usage::ResetCredits),
    UsageResetOutcome(crate::usage::ResetOutcome),
    Capabilities(Vec<String>),
    Settings(serde_json::Value),
    Worktrees(crate::worktree::Repository),
    Operation {
        id: String,
        done: bool,
        error: Option<String>,
        workspace: Option<String>,
    },
    Hello(String),
    State(State),
    Ok,
    Error(String),
    Capture(String),
    ClipboardImage {
        path: String,
    },
    Snapshot {
        seq: u64,
        cols: u16,
        rows: u16,
        bytes: Vec<u8>,
    },
    Output {
        seq: u64,
        bytes: Vec<u8>,
    },
    Geometry {
        seq: u64,
        cols: u16,
        rows: u16,
    },
    Exited,
}
pub fn write_frame<T: Serialize>(w: &mut impl Write, value: &T) -> Result<()> {
    let b = rmp_serde::to_vec_named(value)?;
    if b.len() > MAX_FRAME {
        bail!("frame too large");
    }
    w.write_all(&(b.len() as u32).to_be_bytes())?;
    w.write_all(&b)?;
    Ok(())
}
pub fn read_frame<T: for<'a> Deserialize<'a>>(r: &mut impl Read) -> Result<T> {
    let mut n = [0; 4];
    r.read_exact(&mut n)?;
    let n = u32::from_be_bytes(n) as usize;
    if n > MAX_FRAME {
        bail!("frame too large");
    }
    let mut b = vec![0; n];
    r.read_exact(&mut b)?;
    Ok(rmp_serde::from_slice(&b)?)
}
/// The GUI and CLI cannot be siblings on case-insensitive macOS filesystems.
pub fn cli_path_at(executable: &std::path::Path) -> Result<PathBuf> {
    let parent = executable
        .parent()
        .context("Executable has no parent directory")?;
    if parent.file_name().is_some_and(|name| name == "MacOS") {
        let bundled = parent
            .parent()
            .context("Missing app Contents")?
            .join("Resources/bin/jam");
        if bundled.is_file() {
            return Ok(bundled);
        }
        bail!("Bundled Jam CLI is missing from Resources/bin/jam");
    }
    let sibling = parent.join("jam");
    if sibling.is_file() {
        Ok(sibling)
    } else {
        bail!("Jam CLI is missing beside the daemon")
    }
}
pub fn cli_path() -> Result<PathBuf> {
    cli_path_at(&std::env::current_exe()?)
}

pub fn state_dir() -> PathBuf {
    std::env::var_os("JAM_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                PathBuf::from(std::env::var_os("HOME").unwrap())
                    .join("Library/Application Support/Jam")
            } else {
                std::env::var_os("XDG_STATE_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(std::env::var_os("HOME").unwrap()).join(".local/state")
                    })
                    .join("jam")
            }
        })
}
pub fn socket() -> PathBuf {
    std::env::var_os("JAM_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("XDG_RUNTIME_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(std::env::temp_dir)
                .join(format!("jam-{}", unsafe { libc::geteuid() }))
                .join("jamd.sock")
        })
}
pub fn connect() -> Result<UnixStream> {
    connect_to(&socket())
}
pub fn connect_to(path: &std::path::Path) -> Result<UnixStream> {
    let mut s = UnixStream::connect(path)?;
    s.set_read_timeout(Some(std::time::Duration::from_secs(10)))?;
    match read_frame::<Message>(&mut s)? {
        Message::Hello(v) if v == VERSION => Ok(s),
        _ => bail!("incompatible jam daemon. Update the server from Machines"),
    }
}
pub fn client_id() -> &'static str {
    static ID: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ID.get_or_init(id)
}
pub fn write_request(w: &mut impl Write, request: &Request) -> Result<()> {
    write_frame(
        w,
        &Request::Client {
            client: client_id().into(),
            request: Box::new(request.clone()),
        },
    )
}
pub fn call(req: Request) -> Result<Message> {
    call_at(&socket(), req)
}
#[derive(Debug, Serialize)]
pub struct RequestTimeout {
    pub endpoint: PathBuf,
    pub operation: String,
    pub phase: &'static str,
    pub elapsed_ms: u64,
}
impl std::fmt::Display for RequestTimeout {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} server is not responding. {} {} timed out after {} ms at {}",
            if self.endpoint == socket() {
                "Local"
            } else {
                "Remote"
            },
            self.operation,
            self.phase,
            self.elapsed_ms,
            self.endpoint.display()
        )
    }
}
impl std::error::Error for RequestTimeout {}
pub(crate) fn request_error(
    path: &std::path::Path,
    req: &Request,
    started: std::time::Instant,
    phase: &'static str,
    error: anyhow::Error,
) -> anyhow::Error {
    if !error.downcast_ref::<std::io::Error>().is_some_and(|io| {
        matches!(
            io.kind(),
            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
        )
    }) {
        return error;
    }
    let operation = match req {
        Request::Key { .. } => "Key".to_owned(),
        Request::Send { .. } => "Send".to_owned(),
        Request::Mouse { .. } => "Mouse".to_owned(),
        Request::Paste { .. } => "Paste".to_owned(),
        _ => match serde_json::to_value(req).unwrap_or_default() {
            serde_json::Value::String(name) => name,
            serde_json::Value::Object(fields) => fields.keys().next().cloned().unwrap_or_default(),
            _ => "request".into(),
        },
    };
    error.context(RequestTimeout {
        endpoint: path.to_owned(),
        operation,
        phase,
        elapsed_ms: started.elapsed().as_millis() as u64,
    })
}

pub fn call_at(path: &std::path::Path, req: Request) -> Result<Message> {
    let started = std::time::Instant::now();
    let diagnostic = |phase, error| request_error(path, &req, started, phase, error);
    let mut s = connect_to(path).map_err(|e| diagnostic("handshake", e))?;
    if matches!(
        req,
        Request::UsageResetCredits { .. } | Request::RedeemUsageReset { .. }
    ) {
        // Opening Codex and reading/verifying the account can each take 20s.
        s.set_read_timeout(Some(std::time::Duration::from_secs(65)))?;
    }
    s.set_write_timeout(Some(std::time::Duration::from_secs(10)))?;
    write_request(&mut s, &req).map_err(|e| diagnostic("write", e))?;
    let m = read_frame(&mut s).map_err(|e| diagnostic("reply", e))?;
    if let Message::Error(e) = m {
        bail!("{e}")
    }
    Ok(m)
}
pub fn id() -> String {
    uuid::Uuid::new_v4().to_string()
}

/// Explicit shutdown remains available for known earlier protocol versions.
/// Stop and Ok retain their representation; other incompatible operations do not run.
pub fn stop_at(path: &std::path::Path) -> Result<Message> {
    stop_with_fallback(path, false)
}
/// Explicit local shutdown may terminate an unresponsive daemon, after checking
/// kernel peer credentials, its executable and its private daemon lock. Never
/// use this for an SSH-forwarded socket: its peer is the tunnel, not the daemon.
pub fn stop_local_at(path: &std::path::Path) -> Result<Message> {
    stop_with_fallback(path, true)
}
fn stop_with_fallback(path: &std::path::Path, local: bool) -> Result<Message> {
    stop_verified(path, local, None)
}
pub(crate) fn process_start(pid: i32) -> Result<String> {
    #[cfg(target_os = "macos")]
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
        anyhow::ensure!(read == size, "Could not verify server process identity");
        Ok(format!(
            "{}.{}",
            info.pbi_start_tvsec, info.pbi_start_tvusec
        ))
    }
    #[cfg(target_os = "linux")]
    {
        let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"))?;
        Ok(stat
            .rsplit_once(')')
            .context("Invalid process identity")?
            .1
            .split_whitespace()
            .nth(19)
            .context("Missing process start time")?
            .to_owned())
    }
}
pub(crate) fn stop_local_expected(
    path: &std::path::Path,
    pid: i32,
    executable: &std::path::Path,
    start: &str,
) -> Result<Message> {
    stop_verified(path, true, Some((pid, executable, start)))
}
fn stop_verified(
    path: &std::path::Path,
    local: bool,
    expected: Option<(i32, &std::path::Path, &str)>,
) -> Result<Message> {
    let mut stream = UnixStream::connect(path)?;
    if let Some((pid, executable, start)) = expected {
        anyhow::ensure!(
            peer_identity(&stream)?.0 == pid
                && local_daemon_executable_on(&stream)? == executable
                && process_start(pid)? == start,
            "Server identity changed before shutdown"
        );
    }
    stream.set_read_timeout(Some(std::time::Duration::from_secs(if local {
        3
    } else {
        10
    })))?;
    let result = (|| -> Result<Message> {
        match read_frame::<Message>(&mut stream)? {
            Message::Hello(v)
                if v.starts_with("jam/1:")
                    || v == "jam/2:ghostty/1.0.1:text"
                    || v == "jam/3:ghostty/483ea3d:text"
                    || v == "jam/4:ghostty/483ea3d:text"
                    || v == "jam/5:ghostty/483ea3d:text"
                    || v == "jam/6:ghostty/483ea3d:text"
                    || v == "jam/7:ghostty/483ea3d:text"
                    || v == VERSION => {}
            _ => bail!("unrecognized daemon protocol"),
        }
        write_frame(&mut stream, &Request::Stop)?;
        let reply = read_frame(&mut stream)?;
        match reply {
            Message::Ok => Ok(reply),
            Message::Error(e) => bail!(e),
            _ => bail!("unexpected shutdown response"),
        }
    })();
    match result {
        Err(error)
            if local
                && error.chain().any(|cause| {
                    cause.downcast_ref::<std::io::Error>().is_some_and(|e| {
                        matches!(
                            e.kind(),
                            std::io::ErrorKind::TimedOut | std::io::ErrorKind::WouldBlock
                        )
                    })
                }) =>
        {
            terminate_local_peer(path, &stream).with_context(|| {
                format!("Local Jam server did not respond to shutdown: {error:#}")
            })?;
            Ok(Message::Ok)
        }
        other => other,
    }
}

#[cfg(target_os = "macos")]
pub(crate) fn peer_identity(stream: &UnixStream) -> Result<(libc::pid_t, libc::uid_t)> {
    use std::os::fd::AsRawFd;
    let (mut uid, mut gid, mut pid) = (0, 0, 0);
    let mut length = std::mem::size_of_val(&pid) as libc::socklen_t;
    if unsafe { libc::getpeereid(stream.as_raw_fd(), &mut uid, &mut gid) } != 0
        || unsafe {
            libc::getsockopt(
                stream.as_raw_fd(),
                libc::SOL_LOCAL,
                libc::LOCAL_PEERPID,
                (&mut pid as *mut libc::pid_t).cast(),
                &mut length,
            )
        } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok((pid, uid))
}
#[cfg(target_os = "linux")]
pub(crate) fn peer_identity(stream: &UnixStream) -> Result<(libc::pid_t, libc::uid_t)> {
    use std::os::fd::AsRawFd;
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut length = std::mem::size_of_val(&cred) as libc::socklen_t;
    if unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            (&mut cred as *mut libc::ucred).cast(),
            &mut length,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok((cred.pid, cred.uid))
}
fn peer_executable(pid: libc::pid_t) -> Result<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let length =
            unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), path.len() as u32) };
        if length <= 0 {
            return Err(std::io::Error::last_os_error().into());
        }
        let end = path.iter().position(|b| *b == 0).unwrap_or(path.len());
        use std::os::unix::ffi::OsStrExt;
        Ok(std::ffi::OsStr::from_bytes(&path[..end]).into())
    }
    #[cfg(target_os = "linux")]
    {
        Ok(std::fs::read_link(format!("/proc/{pid}/exe"))?)
    }
}
fn terminate_local_peer(path: &std::path::Path, stream: &UnixStream) -> Result<()> {
    use fs2::FileExt;
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let (pid, uid) = peer_identity(stream)?;
    anyhow::ensure!(
        uid == unsafe { libc::geteuid() } && pid > 1 && pid != unsafe { libc::getpid() },
        "Refusing to terminate an unverified daemon peer"
    );
    let executable = peer_executable(pid)?;
    anyhow::ensure!(
        matches!(
            executable
                .file_name()
                .and_then(|n| n.to_str())
                .map(|name| name.trim_end_matches(" (deleted)")),
            Some("jam" | "jamd")
        ),
        "The unresponsive socket is not owned by a Jam daemon"
    );
    let parent = path.parent().context("Missing socket directory")?;
    let directory = std::fs::symlink_metadata(parent)?;
    anyhow::ensure!(
        directory.is_dir() && directory.uid() == uid && directory.mode() & 0o077 == 0,
        "Invalid local daemon socket directory"
    );
    let lock_path = parent.join("server.lock");
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(lock_path)?;
    let meta = lock.metadata()?;
    anyhow::ensure!(
        meta.is_file() && meta.uid() == uid && meta.mode() & 0o022 == 0,
        "Invalid local daemon lock"
    );
    match lock.try_lock_exclusive() {
        Ok(()) => {
            let _ = lock.unlock();
            bail!("No daemon holds the local server lock");
        }
        Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
        Err(e) => return Err(e.into()),
    }
    eprintln!(
        "Local Jam server is unresponsive. Stopping its verified process. Saved layout is preserved."
    );
    for signal in [libc::SIGTERM, libc::SIGKILL] {
        if unsafe { libc::kill(pid, signal) } != 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::ESRCH) {
                return Err(error.into());
            }
        }
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            match UnixStream::connect(path) {
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
                    ) =>
                {
                    return Ok(());
                }
                Ok(probe) => match peer_identity(&probe) {
                    Ok((peer, _)) if peer != pid => return Ok(()),
                    // Darwin can lose a connecting socket's peer while the
                    // verified daemon exits. Keep polling for refusal rather
                    // than reporting that transient probe as a failed stop.
                    Err(error)
                        if error
                            .downcast_ref::<std::io::Error>()
                            .is_some_and(|e| e.kind() == std::io::ErrorKind::NotConnected) => {}
                    Err(error) => return Err(error),
                    _ => {}
                },
                Err(e) => return Err(e.into()),
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        anyhow::ensure!(
            peer_executable(pid)? == executable,
            "Daemon process identity changed during shutdown"
        );
    }
    bail!("Local Jam server did not stop")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_unbounded_and_truncated_frames() {
        assert!(read_frame::<Message>(&mut &((MAX_FRAME as u32) + 1).to_be_bytes()[..]).is_err());
        assert!(read_frame::<Message>(&mut &[0, 0, 0, 8, 1, 2][..]).is_err());
        let mut b = Vec::new();
        write_frame(&mut b, &Request::State).unwrap();
        assert!(matches!(
            read_frame::<Request>(&mut b.as_slice()).unwrap(),
            Request::State
        ));
    }
}

#[cfg(test)]
mod upgrade_tests {
    use super::*;
    #[test]
    fn incompatible_protocols_never_receive_ordinary_requests_or_unknown_shutdowns() {
        for version in [
            "jam/6:ghostty/483ea3d:text",
            "jam/99:ghostty/483ea3d:text",
            "jam/6:ghostty/unknown:text",
            "unrelated-service",
        ] {
            for stop in [false, true] {
                // Only the known v6 Stop codec is compatible. Even v6 must
                // reject ordinary calls, and future/unknown peers receive no bytes.
                if stop && version == "jam/6:ghostty/483ea3d:text" {
                    continue;
                }
                let dir = tempfile::tempdir().unwrap();
                let path = dir.path().join("incompatible.sock");
                let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
                let server = std::thread::spawn(move || {
                    let (mut stream, _) = listener.accept().unwrap();
                    stream
                        .set_read_timeout(Some(std::time::Duration::from_secs(2)))
                        .unwrap();
                    write_frame(&mut stream, &Message::Hello(version.into())).unwrap();
                    let mut byte = [0];
                    assert_eq!(
                        stream.read(&mut byte).unwrap(),
                        0,
                        "incompatible peer received a command"
                    );
                });
                let result = if stop {
                    stop_local_at(&path)
                } else {
                    call_at(&path, Request::State)
                };
                assert!(result.is_err());
                server.join().unwrap();
            }
        }
    }
    #[test]
    fn local_stop_never_terminates_an_unverified_peer() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("unrelated.sock");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            write_frame(&mut stream, &Message::Hello(VERSION.into())).unwrap();
            assert!(matches!(
                read_frame::<Request>(&mut stream).unwrap(),
                Request::Stop
            ));
            std::thread::sleep(std::time::Duration::from_secs(4));
        });
        let error = stop_local_at(&path).unwrap_err();
        assert!(format!("{error:#}").contains("unverified daemon peer"));
        server.join().unwrap();
    }
    #[test]
    fn explicit_stop_can_reach_earlier_versions_without_permitting_other_requests() {
        for version in [
            "jam/1:ghostty/1.0.1:text",
            "jam/2:ghostty/1.0.1:text",
            "jam/3:ghostty/483ea3d:text",
            "jam/4:ghostty/483ea3d:text",
            "jam/5:ghostty/483ea3d:text",
            "jam/6:ghostty/483ea3d:text",
            VERSION,
        ] {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("v1.sock");
            let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                write_frame(&mut stream, &Message::Hello(version.into())).unwrap();
                assert!(matches!(
                    read_frame::<Request>(&mut stream).unwrap(),
                    Request::Stop
                ));
                write_frame(&mut stream, &Message::Ok).unwrap();
            });
            assert!(matches!(stop_at(&path).unwrap(), Message::Ok));
            server.join().unwrap();
        }
    }
}

/// Kernel identity of a directly connected local daemon (never SSH tunnels).
pub(crate) fn local_daemon_executable(path: &std::path::Path) -> Result<PathBuf> {
    local_daemon_executable_on(&UnixStream::connect(path)?)
}
/// Identity check on the exact connection which will carry a control request.
pub(crate) fn local_daemon_executable_on(stream: &UnixStream) -> Result<PathBuf> {
    let (pid, uid) = peer_identity(stream)?;
    anyhow::ensure!(
        uid == unsafe { libc::geteuid() },
        "Local daemon belongs to another user"
    );
    peer_executable(pid)
}
