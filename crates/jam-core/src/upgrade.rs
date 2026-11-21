//! Authenticated release discovery and staged, recoverable managed installation.
use anyhow::{Context, Result, bail};
use ed25519_dalek::{Signature, VerifyingKey};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};
const ORIGIN: &str = crate::services::RELEASE_ORIGIN;
const LATEST: &str = crate::services::RELEASE_LATEST;
const KEY: &str = crate::services::UPDATE_KEY;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub name: String,
    pub platform: String,
    pub arch: String,
    pub sha256: String,
    pub size: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Release {
    pub schema: u32,
    pub version: String,
    pub protocol: String,
    pub min_macos: String,
    pub assets: Vec<Asset>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Check {
    pub current: String,
    pub available: bool,
    pub release: Option<Release>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Staged {
    pub directory: PathBuf,
    pub release: Release,
    pub archive: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Target {
    pub endpoint: String,
    pub name: String,
    pub panes: Vec<String>,
    #[serde(default)]
    pub sessions: std::collections::BTreeMap<String, String>,
    pub incarnation: String,
    pub build: String,
    pub version: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plan {
    pub installation: PathBuf,
    pub local_running: bool,
    pub targets: Vec<Target>,
    pub offline: Vec<String>,
    pub app_pids: Vec<i32>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Journal {
    schema: u32,
    token: String,
    staged: Staged,
    plan: Plan,
    phase: String,
    error: Option<String>,
    environment: std::collections::BTreeMap<String, String>,
}
const PROFILE_ENV: &[&str] = &[
    "HOME",
    "JAM_SOCKET",
    "JAM_STATE_DIR",
    "XDG_CONFIG_HOME",
    "XDG_RUNTIME_DIR",
    "XDG_DATA_HOME",
    "XDG_STATE_HOME",
    "JAM_DEFAULTS_SUITE",
];
#[cfg(target_os = "macos")]
fn relaunch(journal: &Journal) -> Result<()> {
    let mut command = Command::new("/usr/bin/open");
    // A different installation may share our bundle identifier. Never reuse it.
    command.arg("-n");
    for (key, value) in &journal.environment {
        if PROFILE_ENV.contains(&key.as_str()) {
            command.arg("--env").arg(format!("{key}={value}"));
        }
    }
    run(command.arg(&journal.plan.installation))
}
const CHECKPOINT_FILES: &[&str] = &[
    "layout.json",
    "layout.pre-migration.json",
    "coordination.sqlite",
    "coordination.sqlite-wal",
    "coordination.sqlite-shm",
    "app-settings.json",
    ".upgrade-boot.json",
];
#[cfg(target_os = "macos")]
fn checkpoint(path: &Path) -> Result<()> {
    checkpoint_at(path, &crate::state_dir())
}
#[cfg(any(target_os = "macos", test))]
fn checkpoint_at(path: &Path, state: &Path) -> Result<()> {
    let backup = path
        .parent()
        .context("Missing transaction directory")?
        .join("checkpoint");
    if backup.exists() {
        fs::remove_dir_all(&backup)?;
    }
    crate::server::private_dir(&backup)?;
    let mut files = std::collections::BTreeMap::new();
    for name in CHECKPOINT_FILES {
        let source = state.join(name);
        if source.exists() {
            fs::copy(source, backup.join(name))?;
            fs::File::open(backup.join(name))?.sync_all()?;
            files.insert((*name).to_owned(), file_digest(&backup.join(name))?);
        }
    }
    write_json(&backup.join("files.json"), &files)?;
    write_json(&backup.join("complete.json"), &true)
}
#[cfg(any(target_os = "macos", test))]
fn restore_checkpoint_at(path: &Path, state: &Path) -> Result<()> {
    let backup = path
        .parent()
        .context("Missing transaction directory")?
        .join("checkpoint");
    if !backup.join("complete.json").is_file() {
        return Ok(());
    }
    let files = validated_checkpoint_files(&backup)?;
    for name in CHECKPOINT_FILES {
        let target = state.join(name);
        if files.contains_key(*name) {
            fs::copy(backup.join(name), &target)?;
            fs::File::open(target)?.sync_all()?;
        } else if target.exists() {
            fs::remove_file(target)?;
        }
    }
    fs::File::open(state)?.sync_all()?;
    Ok(())
}
#[cfg(any(target_os = "macos", test))]
fn validated_checkpoint_files(backup: &Path) -> Result<std::collections::BTreeMap<String, String>> {
    let files: std::collections::BTreeMap<String, String> = serde_json::from_slice(
        &fs::read(backup.join("files.json"))
            .context("Checkpoint file index is missing; application and state were retained")?,
    )?;
    for (name, digest) in &files {
        if !CHECKPOINT_FILES.contains(&name.as_str()) {
            bail!("Unexpected checkpoint entry");
        }
        if !backup.join(name).symlink_metadata()?.is_file()
            || file_digest(&backup.join(name))? != *digest
        {
            bail!("Checkpoint file is invalid; application and state were retained");
        }
    }
    for name in CHECKPOINT_FILES {
        match backup.join(name).symlink_metadata() {
            Ok(metadata) if !metadata.is_file() || !files.contains_key(*name) => {
                bail!("Checkpoint file is invalid; application and state were retained")
            }
            Ok(_) => {}
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound && !files.contains_key(*name) => {}
            Err(error) => {
                return Err(error)
                    .context("Checkpoint file is missing; application and state were retained");
            }
        }
    }
    Ok(files)
}
#[cfg(any(target_os = "macos", test))]
fn file_digest(path: &Path) -> Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex(&hasher.finalize()))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
fn unhex(s: &str) -> Result<Vec<u8>> {
    if !s.len().is_multiple_of(2) || !s.bytes().all(|b| b.is_ascii_hexdigit()) {
        bail!("Invalid hexadecimal data");
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(Into::into))
        .collect()
}
fn authenticate(bytes: &[u8], signature: &str, key: &str) -> Result<Release> {
    let public: [u8; 32] = unhex(key.trim())?
        .try_into()
        .map_err(|_| anyhow::anyhow!("Invalid release public key"))?;
    let signature = Signature::from_slice(&unhex(signature.trim())?)?;
    VerifyingKey::from_bytes(&public)?
        .verify_strict(bytes, &signature)
        .context("Release signature verification failed")?;
    let release: Release = serde_json::from_slice(bytes)?;
    if release.schema != 1 {
        bail!("Upgrade Jam using a supported installer: unknown release manifest format");
    }
    let version = semver::Version::parse(&release.version)?;
    if !version.pre.is_empty() || !version.build.is_empty() {
        bail!("Not a stable Jam release");
    }
    let mut names = std::collections::HashSet::new();
    for asset in &release.assets {
        let expected = match (asset.platform.as_str(), asset.arch.as_str()) {
            ("macos", "aarch64") => format!("Jam-v{}-macos-arm64.zip", release.version),
            ("linux", "x86_64" | "aarch64") => {
                format!("jam-v{}-linux-{}.tar.gz", release.version, asset.arch)
            }
            _ => bail!("Unsupported release asset"),
        };
        if asset.name != expected
            || asset.sha256.len() != 64
            || unhex(&asset.sha256)?.len() != 32
            || asset.size == 0
            || asset.size > 2 * 1024 * 1024 * 1024
            || !names.insert(&asset.name)
        {
            bail!("Invalid release asset metadata");
        }
    }
    Ok(release)
}
fn get(url: &str, limit: u64) -> Result<Vec<u8>> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(crate::services::tls_config())
        .timeout_global(Some(Duration::from_secs(180)))
        .build()
        .into();
    Ok(agent
        .get(url)
        .header("User-Agent", concat!("Jam/", env!("CARGO_PKG_VERSION")))
        .call()?
        .body_mut()
        .with_config()
        .limit(limit)
        .read_to_vec()?)
}
pub fn check() -> Result<Check> {
    let current = crate::RELEASE_VERSION.to_owned();
    let bytes = match get(LATEST, 1024 * 1024) {
        Ok(bytes) => bytes,
        Err(e)
            if matches!(
                e.downcast_ref::<ureq::Error>(),
                Some(ureq::Error::StatusCode(404))
            ) =>
        {
            return Ok(Check {
                current,
                available: false,
                release: None,
            });
        }
        Err(e) => return Err(e).context("Could not check GitHub for Jam releases"),
    };
    let metadata: serde_json::Value = serde_json::from_slice(&bytes)?;
    if metadata["draft"] == true || metadata["prerelease"] == true {
        bail!("GitHub returned a non-stable release");
    }
    let version = metadata["tag_name"]
        .as_str()
        .and_then(|t| t.strip_prefix('v'))
        .context("Release has no version tag")?;
    let candidate = semver::Version::parse(version)?;
    if candidate <= semver::Version::parse(&current)? {
        return Ok(Check {
            current,
            available: false,
            release: None,
        });
    }
    if !candidate.pre.is_empty() || !candidate.build.is_empty() {
        bail!("Not a stable release");
    }
    let base = format!("{ORIGIN}/v{candidate}");
    let manifest = get(&format!("{base}/manifest.json"), 1024 * 1024)?;
    let sig = String::from_utf8(get(&format!("{base}/manifest.sig"), 1024)?)?;
    let release = authenticate(&manifest, &sig, KEY)?;
    if release.version != version {
        bail!("Release tag and signed manifest do not match");
    }
    Ok(Check {
        current,
        available: true,
        release: Some(release),
    })
}
fn platform() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}
fn root() -> Result<PathBuf> {
    let path = crate::state_dir().join("upgrades");
    crate::server::private_dir(&crate::state_dir())?;
    crate::server::private_dir(&path)?;
    Ok(path)
}
fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt;
    let tmp = path.with_extension(format!("tmp-{}", crate::id()));
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&tmp)?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    fs::rename(tmp, path)?;
    fs::File::open(path.parent().context("Missing journal parent")?)?.sync_all()?;
    Ok(())
}
fn verify_file(path: &Path, asset: &Asset) -> Result<()> {
    let mut file = fs::File::open(path)?;
    if file.metadata()?.len() != asset.size {
        bail!("Downloaded asset size differs from signed manifest");
    }
    let mut hash = Sha256::new();
    let mut buffer = [0; 65536];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hash.update(&buffer[..n]);
    }
    if hex(&hash.finalize()) != asset.sha256.to_ascii_lowercase() {
        bail!("Downloaded asset checksum differs from signed manifest");
    }
    Ok(())
}
pub fn stage(release: &Release) -> Result<Staged> {
    if semver::Version::parse(&release.version)? <= semver::Version::parse(crate::RELEASE_VERSION)?
    {
        bail!("Jam never installs an older or identical release");
    }
    if cfg!(target_os = "macos") {
        let output = Command::new("/usr/bin/sw_vers")
            .arg("-productVersion")
            .output()?;
        let parse = |s: &str| -> Result<semver::Version> {
            let mut p = s.trim().split('.').map(str::to_owned).collect::<Vec<_>>();
            while p.len() < 3 {
                p.push("0".into());
            }
            Ok(semver::Version::parse(&p.join("."))?)
        };
        if parse(std::str::from_utf8(&output.stdout)?)? < parse(&release.min_macos)? {
            bail!("This release requires macOS {} or later", release.min_macos);
        }
    }
    let directory = root()?.join(format!("stage-{}", crate::id()));
    crate::server::private_dir(&directory)?;
    // Authenticate again when given a release by an FFI/UI caller.
    let base = format!("{ORIGIN}/v{}", semver::Version::parse(&release.version)?);
    let bytes = get(&format!("{base}/manifest.json"), 1024 * 1024)?;
    let sig = String::from_utf8(get(&format!("{base}/manifest.sig"), 1024)?)?;
    let verified = authenticate(&bytes, &sig, KEY)?;
    if serde_json::to_vec(&verified)? != serde_json::to_vec(release)? {
        bail!("Release metadata changed; check for updates again");
    }
    fs::write(directory.join("manifest.json"), bytes)?;
    fs::write(directory.join("manifest.sig"), sig)?;
    let asset = release
        .assets
        .iter()
        .find(|a| a.platform == platform() && a.arch == std::env::consts::ARCH)
        .context("No release for this platform")?;
    let archive = directory.join(&asset.name);
    let body = get(&format!("{base}/{}", asset.name), asset.size)?;
    fs::write(&archive, body)?;
    verify_file(&archive, asset)?;
    // Stage every platform artifact needed by connected remotes before the
    // interruption prompt. A later offline machine still needs fresh consent.
    for remote_asset in release
        .assets
        .iter()
        .filter(|asset| asset.platform == "linux")
    {
        let remote_archive = directory.join(format!("remote-{}.tar.gz", remote_asset.arch));
        fs::write(
            &remote_archive,
            get(&format!("{base}/{}", remote_asset.name), remote_asset.size)?,
        )?;
        verify_file(&remote_archive, remote_asset)?;
    }
    let staged = Staged {
        directory,
        release: release.clone(),
        archive,
    };
    unpack(&staged)?;
    Ok(staged)
}
fn managed(exe: &Path) -> Result<PathBuf> {
    let exe = fs::canonicalize(exe)?;
    if exe
        .ancestors()
        .any(|p| p.join("Cargo.toml").exists() || p.join(".git").exists())
    {
        bail!("Development checkouts are never self-updated; install the release app first");
    }
    if cfg!(target_os = "macos") {
        let app = exe
            .ancestors()
            .find(|p| p.extension().is_some_and(|x| x == "app"))
            .context("Install the release Jam.app before upgrading")?;
        let marker: serde_json::Value = serde_json::from_slice(
            &fs::read(app.join("Contents/Resources/release.json"))
                .context("This is not a managed Jam release app")?,
        )?;
        if marker["version"] != crate::RELEASE_VERSION {
            bail!("Installed Jam version metadata does not match this CLI");
        }
        Ok(app.into())
    } else {
        let home = std::env::var_os("HOME").context("HOME is missing")?;
        let base = std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(home).join(".local/share"))
            .join("jam/servers");
        let current = fs::canonicalize(base.join("current"))?;
        if exe.parent() != Some(current.as_path()) {
            bail!("Only managed Jam server installations can self-upgrade");
        }
        Ok(current)
    }
}
pub fn managed_installation() -> Result<PathBuf> {
    managed(&std::env::current_exe()?)
}
pub fn install_cli() -> Result<PathBuf> {
    let exe = fs::canonicalize(std::env::current_exe()?)?;
    managed(&exe)?;
    let directory =
        PathBuf::from(std::env::var_os("HOME").context("HOME is missing")?).join(".local/bin");
    fs::create_dir_all(&directory)?;
    let target = directory.join("jam");
    if target.symlink_metadata().is_ok() {
        if fs::canonicalize(&target).ok().as_ref() == Some(&exe) {
            return Ok(target);
        }
        bail!(
            "{} already exists; refusing to overwrite another command",
            target.display()
        );
    }
    let link = if cfg!(target_os = "macos") {
        managed(&exe)?.join("Contents/Resources/bin/jam")
    } else {
        exe.parent().unwrap().parent().unwrap().join("current/jam")
    };
    std::os::unix::fs::symlink(link, &target)?;
    Ok(target)
}
// The upgrade probe only uses stable read-only control messages. It never
// admits ordinary mutations through an incompatible terminal protocol.
fn probe(path: &Path, request: crate::Request) -> Result<crate::Message> {
    if !matches!(request, crate::Request::HostInfo | crate::Request::State) {
        bail!("Only read-only upgrade probes are allowed");
    }
    let mut socket = std::os::unix::net::UnixStream::connect(path)?;
    socket.set_read_timeout(Some(Duration::from_secs(10)))?;
    socket.set_write_timeout(Some(Duration::from_secs(10)))?;
    if !matches!(crate::read_frame::<crate::Message>(&mut socket)?,crate::Message::Hello(ref version) if version.starts_with("jam/"))
    {
        bail!("Unrecognized server; left unchanged");
    }
    crate::write_frame(&mut socket, &request)?;
    let response = crate::read_frame(&mut socket)?;
    if let crate::Message::Error(ref error) = response {
        bail!("{error}");
    }
    Ok(response)
}
#[cfg(target_os = "macos")]
fn control(path: &Path, request: crate::Request) -> Result<()> {
    if !matches!(
        request,
        crate::Request::PrepareUpgradeApproved { .. }
            | crate::Request::AbortUpgrade { .. }
            | crate::Request::StopUpgrade { .. }
            | crate::Request::CommitUpgrade { .. }
    ) {
        bail!("Invalid upgrade control request");
    }
    let mut socket = std::os::unix::net::UnixStream::connect(path)?;
    socket.set_read_timeout(Some(Duration::from_secs(10)))?;
    socket.set_write_timeout(Some(Duration::from_secs(10)))?;
    if !matches!(crate::read_frame::<crate::Message>(&mut socket)?,crate::Message::Hello(ref version) if version.starts_with("jam/"))
    {
        bail!("Unknown daemon; left unchanged");
    }
    crate::write_frame(&mut socket, &request)?;
    match crate::read_frame::<crate::Message>(&mut socket)? {
        crate::Message::Ok => Ok(()),
        crate::Message::Error(error) => bail!("{error}"),
        _ => bail!("Unexpected upgrade admission response"),
    }
}
fn admission(target: &Target, token: &str) -> crate::Request {
    crate::Request::PrepareUpgradeApproved {
        token: token.into(),
        incarnation: target.incarnation.clone(),
        build: target.build.clone(),
        panes: target.panes.clone(),
        sessions: target.sessions.clone(),
    }
}
#[cfg(target_os = "macos")]
fn wait_stopped(path: &Path) -> Result<()> {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::os::unix::net::UnixStream::connect(path).is_ok() {
        if std::time::Instant::now() > deadline {
            bail!("Daemon did not stop");
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    Ok(())
}
fn state_target(endpoint: &str, name: &str, path: &Path) -> Result<Target> {
    let crate::Message::HostInfo {
        incarnation,
        build,
        release_version,
        ..
    } = probe(path, crate::Request::HostInfo)?
    else {
        bail!("Unexpected server metadata");
    };
    let crate::Message::State(state) = probe(path, crate::Request::State)? else {
        bail!("Unexpected server state");
    };
    let sessions=state.panes.iter().filter(|pane|!pane.exited).map(|pane| {
        Ok((pane.id.clone(),pane.session_generation.clone().context("This server cannot identify terminal sessions; explicit server migration is required")?))
    }).collect::<Result<std::collections::BTreeMap<_,_>>>()?;
    Ok(Target {
        endpoint: endpoint.into(),
        name: name.into(),
        panes: state
            .panes
            .iter()
            .filter(|p| !p.exited)
            .map(|p| p.id.clone())
            .collect(),
        sessions,
        incarnation,
        build,
        version: release_version,
    })
}
// A forwarding endpoint belongs to this process. A caller must establish its
// own transport; another GUI/CLI process's listener is not an availability signal.
fn connect_remote_target(endpoint: &str, name: &str) -> Result<(PathBuf, Target)> {
    let socket = crate::remote::connect(endpoint)?;
    let target = state_target(endpoint, name, &socket)?;
    Ok((socket, target))
}
fn revalidate_target(approved: &mut Target, current: &Target) -> Result<()> {
    if current.incarnation != approved.incarnation
        || current.build != approved.build
        || current
            .sessions
            .iter()
            .any(|(pane, generation)| approved.sessions.get(pane) != Some(generation))
    {
        bail!("Sessions changed after confirmation; confirm the upgrade again");
    }
    approved.panes.retain(|pane| current.panes.contains(pane));
    approved
        .sessions
        .retain(|pane, _| current.panes.contains(pane));
    Ok(())
}
#[cfg(any(target_os = "macos", test))]
fn revalidate_remote_target(approved: &mut Target) -> Result<Option<PathBuf>> {
    let Ok((socket, current)) = connect_remote_target(&approved.endpoint, &approved.name) else {
        return Ok(None);
    };
    revalidate_target(approved, &current)?;
    Ok(Some(socket))
}
#[cfg(any(target_os = "macos", test))]
fn revalidate_plan_targets(plan: &mut Plan) -> Result<()> {
    let mut unavailable = std::collections::HashSet::new();
    for target in &mut plan.targets {
        if !target.endpoint.is_empty() {
            if revalidate_remote_target(target)?.is_none() {
                unavailable.insert(target.endpoint.clone());
                if !plan.offline.contains(&target.name) {
                    plan.offline.push(target.name.clone());
                }
            }
        } else if std::os::unix::net::UnixStream::connect(crate::socket()).is_ok() {
            let current = state_target("", &target.name, &crate::socket())?;
            revalidate_target(target, &current)?;
        }
    }
    plan.targets
        .retain(|target| !unavailable.contains(&target.endpoint));
    Ok(())
}
#[cfg(target_os = "macos")]
fn process_path(pid: i32) -> Option<PathBuf> {
    unsafe extern "C" {
        fn proc_pidpath(pid: libc::c_int, buffer: *mut libc::c_void, size: u32) -> libc::c_int;
    }
    let mut bytes = [0u8; 4096];
    let n = unsafe { proc_pidpath(pid, bytes.as_mut_ptr().cast(), bytes.len() as u32) };
    if n <= 0 {
        return None;
    }
    let end = bytes.iter().position(|b| *b == 0).unwrap_or(n as usize);
    Some(PathBuf::from(std::str::from_utf8(&bytes[..end]).ok()?))
}
fn app_pids(_installation: &Path) -> Result<Vec<i32>> {
    #[cfg(target_os = "macos")]
    {
        let out = Command::new("/bin/ps").args(["-axo", "pid="]).output()?;
        let app = _installation.join("Contents/MacOS/Jam");
        Ok(String::from_utf8(out.stdout)?
            .split_whitespace()
            .filter_map(|p| p.parse::<i32>().ok())
            .filter(|pid| process_path(*pid).as_ref() == Some(&app))
            .collect())
    }
    #[cfg(not(target_os = "macos"))]
    {
        Ok(vec![])
    }
}
fn tree_bytes(path: &Path) -> Result<u64> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.is_file() {
        return Ok(metadata.len());
    }
    if !metadata.is_dir() {
        bail!("Unexpected staged artifact type");
    }
    let mut bytes = 0u64;
    for entry in fs::read_dir(path)? {
        bytes = bytes
            .checked_add(tree_bytes(&entry?.path())?)
            .context("Artifact size overflow")?;
    }
    Ok(bytes)
}
fn check_space(staged: &Staged, installation: &Path) -> Result<()> {
    use std::os::unix::fs::MetadataExt;
    let state = crate::state_dir();
    let parent = installation
        .parent()
        .context("Missing installation parent")?;
    let installation_bytes = tree_bytes(&staged.directory.join("extracted"))?;
    let mut checkpoint_bytes = 0u64;
    for name in CHECKPOINT_FILES {
        match fs::metadata(state.join(name)) {
            Ok(metadata) => checkpoint_bytes = checkpoint_bytes.saturating_add(metadata.len()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    let reserve = 32 * 1024 * 1024;
    let shared = fs::metadata(&state)?.dev() == fs::metadata(parent)?.dev();
    let required = if shared {
        installation_bytes.saturating_add(checkpoint_bytes)
    } else {
        installation_bytes
    };
    if fs2::available_space(parent)? < required.saturating_add(reserve)
        || fs2::available_space(&state)? < checkpoint_bytes.saturating_add(reserve)
    {
        bail!(
            "Not enough free disk space to stage the installation and preserve its state; no sessions were stopped"
        );
    }
    Ok(())
}
pub fn plan(staged: &Staged) -> Result<Plan> {
    let installation = managed(&std::env::current_exe()?)?;
    check_space(staged, &installation)?;
    let mut targets = vec![];
    let mut offline = vec![];
    let local_running = std::os::unix::net::UnixStream::connect(crate::socket()).is_ok();
    if local_running {
        let local = state_target("", "Local", &crate::socket())?;
        let expected = if cfg!(target_os = "macos") {
            installation.join("Contents/MacOS/jamd")
        } else {
            installation.join("jamd")
        };
        if crate::protocol::local_daemon_executable(&crate::socket())? != expected {
            bail!("The local daemon belongs to another installation; it was left unchanged");
        }
        if local.build != crate::SERVER_BUILD {
            bail!(
                "Another Jam build owns the local daemon; restart it explicitly from this installation before upgrading"
            );
        }
        targets.push(local);
    }
    for remote in crate::remote::list()? {
        match connect_remote_target(&remote.id, &remote.name) {
            Ok((_, target)) => targets.push(target),
            Err(_) => offline.push(remote.name),
        }
    }
    let app_pids = app_pids(&installation)?;
    Ok(Plan {
        installation,
        local_running,
        targets,
        offline,
        app_pids,
    })
}
pub fn launch(staged: &Staged, plan: &Plan) -> Result<PathBuf> {
    let upgrades = root()?;
    let lock = installation_lock(&upgrades)?;
    lock.try_lock_exclusive()
        .context("Another Jam upgrade or recovery is running")?;
    if !crash_recovery::pending_locked(&upgrades, None)?.is_empty() {
        bail!("Recover the interrupted Jam upgrade before starting another upgrade");
    }
    let directory = upgrades.join(format!("apply-{}", crate::id()));
    crate::server::private_dir(&directory)?;
    let journal = directory.join("journal.json");
    write_json(
        &journal,
        &Journal {
            schema: 1,
            token: crate::id(),
            staged: staged.clone(),
            plan: plan.clone(),
            phase: "staged".into(),
            error: None,
            environment: std::env::vars()
                .filter(|(key, _)| PROFILE_ENV.contains(&key.as_str()))
                .collect(),
        },
    )?;
    let helper = directory.join("jam-updater");
    fs::copy(crate::cli_path()?, &helper)?;
    let log = fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(directory.join("update.log"))?;
    let mut command = if cfg!(target_os = "linux") {
        let mut c = Command::new("systemd-run");
        c.args(["--user", "--collect", "--quiet", "--unit"])
            .arg(format!("jam-upgrade-{}", crate::id()));
        for key in PROFILE_ENV {
            if let Ok(value) = std::env::var(key) {
                c.arg(format!("--setenv={key}={value}"));
            }
        }
        c.arg(&helper);
        c
    } else {
        Command::new(&helper)
    };
    command
        .arg("__upgrade-apply")
        .arg(&journal)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    // The durable staged journal is now the admission barrier. Release before
    // spawning so the helper can acquire the lock without timing assumptions;
    // another launcher must see this journal before it can create its own.
    drop(lock);
    let mut child = command.spawn()?;
    if cfg!(target_os = "linux") && !child.wait()?.success() {
        bail!("Could not launch detached updater service");
    }
    Ok(journal)
}
fn run(command: &mut Command) -> Result<()> {
    let output = command.output()?;
    if !output.status.success() {
        bail!(
            "{}: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    Ok(())
}
#[cfg(target_os = "macos")]
fn sync_tree(path: &Path) -> Result<()> {
    let metadata = path.symlink_metadata()?;
    if metadata.is_symlink() {
        bail!("Unexpected symlink in staged release");
    }
    if metadata.is_dir() {
        for entry in fs::read_dir(path)? {
            sync_tree(&entry?.path())?;
        }
    }
    fs::File::open(path)?.sync_all()?;
    Ok(())
}
fn safe_names(text: &str) -> Result<()> {
    for line in text.lines() {
        let path = Path::new(line);
        if path.is_absolute()
            || path
                .components()
                .any(|c| matches!(c, std::path::Component::ParentDir))
            || line.contains('\\')
        {
            bail!("Archive contains an unsafe path");
        }
    }
    Ok(())
}
fn unpack(staged: &Staged) -> Result<PathBuf> {
    let asset = staged
        .release
        .assets
        .iter()
        .find(|a| {
            a.name
                == staged
                    .archive
                    .file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
        })
        .context("Missing staged asset")?;
    verify_file(&staged.archive, asset)?;
    let destination = staged.directory.join("extracted");
    if destination.exists() {
        fs::remove_dir_all(&destination)?;
    }
    fs::create_dir(&destination)?;
    let (listing, details) = if asset.platform == "macos" {
        (
            Command::new("/usr/bin/unzip")
                .args(["-Z", "-1"])
                .arg(&staged.archive)
                .output()?,
            Command::new("/usr/bin/zipinfo")
                .arg("-l")
                .arg(&staged.archive)
                .output()?,
        )
    } else {
        (
            Command::new("tar")
                .arg("-tzf")
                .arg(&staged.archive)
                .output()?,
            Command::new("tar")
                .arg("-tvzf")
                .arg(&staged.archive)
                .output()?,
        )
    };
    if !listing.status.success() || !details.status.success() {
        bail!("Invalid release archive");
    }
    safe_names(std::str::from_utf8(&listing.stdout)?)?;
    if std::str::from_utf8(&details.stdout)?
        .lines()
        .any(|l| l.starts_with('l') || l.starts_with('h'))
    {
        bail!("Release archives may not contain symbolic or hard links");
    }
    if asset.platform == "macos" {
        run(Command::new("/usr/bin/ditto")
            .args(["-x", "-k"])
            .arg(&staged.archive)
            .arg(&destination))?;
        let app = destination.join("Jam.app");
        let marker: serde_json::Value =
            serde_json::from_slice(&fs::read(app.join("Contents/Resources/release.json"))?)?;
        if marker["version"] != staged.release.version {
            bail!("Packaged app version differs from signed manifest");
        }
        run(Command::new("/usr/bin/codesign")
            .args(["--verify", "--deep", "--strict"])
            .arg(&app))?;
        let output = Command::new(app.join("Contents/Resources/bin/jam"))
            .arg("--version")
            .output()?;
        if !output.status.success()
            || String::from_utf8(output.stdout)?.trim() != format!("jam {}", staged.release.version)
        {
            bail!("Packaged CLI failed version verification");
        }
    } else {
        run(Command::new("tar")
            .arg("-xzf")
            .arg(&staged.archive)
            .arg("-C")
            .arg(&destination))?;
    }
    Ok(destination)
}
fn read_journal(path: &Path) -> Result<Journal> {
    use std::os::unix::fs::MetadataExt;
    let info = path.symlink_metadata()?;
    if !info.is_file() || info.uid() != unsafe { libc::geteuid() } || info.mode() & 0o077 != 0 {
        bail!("Upgrade journal must be a private, owned regular file");
    }
    let journal: Journal = serde_json::from_slice(&fs::read(path)?)?;
    if journal.schema != 1 {
        bail!("Unsupported upgrade journal");
    }
    let staged = &journal.staged;
    let verified = authenticate(
        &fs::read(staged.directory.join("manifest.json"))?,
        &fs::read_to_string(staged.directory.join("manifest.sig"))?,
        KEY,
    )?;
    if serde_json::to_vec(&verified)? != serde_json::to_vec(&staged.release)? {
        bail!("Staged release metadata changed");
    }
    Ok(journal)
}
fn save_journal(path: &Path, journal: &mut Journal, phase: &str) -> Result<()> {
    journal.phase = phase.into();
    write_json(path, journal)
}
#[cfg(target_os = "macos")]
fn exchange(a: &Path, b: &Path) -> Result<()> {
    use std::os::unix::ffi::OsStrExt;
    let parent = a.parent().context("Missing app directory")?.to_path_buf();
    let a = std::ffi::CString::new(a.as_os_str().as_bytes())?;
    let b = std::ffi::CString::new(b.as_os_str().as_bytes())?;
    if unsafe { libc::renameatx_np(libc::AT_FDCWD, a.as_ptr(), libc::AT_FDCWD, b.as_ptr(), 2) } < 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    fs::File::open(parent)?.sync_all()?;
    Ok(())
}
fn installation_lock(directory: &Path) -> Result<fs::File> {
    use std::os::unix::fs::OpenOptionsExt;
    Ok(fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(directory.join("install.lock"))?)
}
pub fn apply(path: &Path) -> Result<()> {
    std::thread::sleep(Duration::from_millis(300));
    let _ = std::env::set_current_dir("/");
    let upgrades = root()?;
    let lock = installation_lock(&upgrades)?;
    lock.try_lock_exclusive()
        .context("Another Jam upgrade is in progress")?;
    let mut journal = read_journal(path)?;
    if journal.phase != "staged" {
        bail!(
            "This upgrade attempt has already started or finished; use explicit recovery if needed"
        );
    }
    // Discovery in another process can race launch. Recheck under the same
    // exclusive lock used for application replacement, before stopping anything
    // or creating a new checkpoint. Refusal needs no rollback of this attempt.
    if !crash_recovery::pending_locked(&upgrades, Some(path))?.is_empty() {
        journal.error =
            Some("Recover the interrupted Jam upgrade before starting another upgrade".into());
        save_journal(path, &mut journal, "abandoned")?;
        bail!("Recover the interrupted Jam upgrade before starting another upgrade");
    }
    let result = apply_inner(path, &mut journal);
    if let Err(ref error) = result {
        if journal.plan.local_running {
            let expected = if cfg!(target_os = "macos") {
                journal.plan.installation.join("Contents/MacOS/jamd")
            } else {
                journal.plan.installation.join("jamd")
            };
            if let Ok(expected) = fs::canonicalize(expected) {
                let _ = owned_call(
                    &crate::socket(),
                    &expected,
                    crate::Request::AbortUpgrade {
                        token: journal.token.clone(),
                    },
                );
            }
        }
        let latest = read_journal(path)?;
        if !["local_verified", "complete", "complete_with_pending"].contains(&latest.phase.as_str())
            && let Err(rollback) = recover_previous(path, &journal)
        {
            let mut retained = read_journal(path)?;
            retained.error = Some(format!(
                "Upgrade failed: {error:#}. Recovery needs attention: {rollback:#}"
            ));
            write_json(path, &retained)?;
            return Err(rollback).context("Upgrade failed and rollback also failed; inspect the retained transaction directory");
        }
        let latest = read_journal(path)?;
        let phase = if [
            "local_verified",
            "complete",
            "complete_with_pending",
            "recovered",
            "rolled_back",
        ]
        .contains(&latest.phase.as_str())
        {
            latest.phase.clone()
        } else {
            "failed".into()
        };
        journal = latest;
        journal.error = Some(format!("{error:#}"));
        save_journal(path, &mut journal, &phase)?;
    }
    result
}
#[cfg(target_os = "macos")]
fn candidate_path(path: &Path, journal: &Journal) -> Result<PathBuf> {
    Ok(journal
        .plan
        .installation
        .parent()
        .context("Missing application parent")?
        .join(format!(
            ".Jam-upgrade-{}",
            path.parent()
                .context("Missing transaction directory")?
                .file_name()
                .context("Missing transaction ID")?
                .to_string_lossy()
        )))
}
fn owned_call(socket: &Path, executable: &Path, request: crate::Request) -> Result<crate::Message> {
    let mut stream = std::os::unix::net::UnixStream::connect(socket)?;
    stream.set_read_timeout(Some(Duration::from_secs(10)))?;
    stream.set_write_timeout(Some(Duration::from_secs(10)))?;
    // Identity belongs to this connection, not a prior path probe which another
    // installation could replace before the mutating request is delivered.
    if crate::protocol::local_daemon_executable_on(&stream)? != executable {
        bail!("Another installation owns the local daemon; it was left unchanged");
    }
    if !matches!(crate::read_frame::<crate::Message>(&mut stream)?,crate::Message::Hello(ref version) if version.starts_with("jam/"))
    {
        bail!("Unknown daemon; it was left unchanged");
    }
    crate::write_frame(&mut stream, &request)?;
    match crate::read_frame::<crate::Message>(&mut stream)? {
        crate::Message::Error(error) => bail!("{error}"),
        reply => Ok(reply),
    }
}
#[cfg(target_os = "macos")]
fn stop_owned_rollback_daemon(
    socket: &Path,
    installation: &Path,
    state: &Path,
    token: &str,
    version: &str,
) -> Result<()> {
    if !crash_recovery::daemon_present(socket)? {
        return Ok(());
    }
    let executable = fs::canonicalize(installation.join("Contents/MacOS/jamd"))?;
    let crate::Message::HostInfo {
        release_version,
        build,
        ..
    } = owned_call(socket, &executable, crate::Request::HostInfo)?
    else {
        bail!("Cannot establish daemon ownership; it was left unchanged");
    };
    if release_version.as_deref() != Some(version) {
        bail!("Another daemon version owns the socket; it was left unchanged");
    }
    let info = Command::new(&executable).arg("--build-info").output()?;
    let info_value: serde_json::Value = serde_json::from_slice(&info.stdout)?;
    if !info.status.success() || info_value["build"].as_str() != Some(build.as_str()) {
        bail!("The daemon build changed; existing sessions were left unchanged");
    }
    use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
    let boot = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(state.join(crate::server::UPGRADE_BOOT_FILE))?;
    let metadata = boot.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        bail!("Cannot verify upgrade startup ownership; existing sessions were left unchanged");
    }
    let marker: serde_json::Value = serde_json::from_reader(boot.take(65536))?;
    if marker["token"].as_str() != Some(token) {
        bail!("Another transaction owns startup; it was left unchanged");
    }
    if !matches!(
        owned_call(
            socket,
            &executable,
            crate::Request::StopUpgrade {
                token: token.into()
            }
        )?,
        crate::Message::Ok
    ) {
        bail!("Unexpected owned rollback stop response");
    }
    wait_stopped(socket)
}
#[cfg(target_os = "macos")]
fn rollback_files_at(
    path: &Path,
    journal: &mut Journal,
    state: &Path,
    socket: &Path,
    after_exchange: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let destination = journal.plan.installation.clone();
    let marker: serde_json::Value = serde_json::from_slice(&fs::read(
        destination.join("Contents/Resources/release.json"),
    )?)?;
    if marker["version"] == journal.staged.release.version {
        let previous = candidate_path(path, journal)?;
        let previous_marker: serde_json::Value =
            serde_json::from_slice(&fs::read(previous.join("Contents/Resources/release.json"))?)?;
        if semver::Version::parse(
            previous_marker["version"]
                .as_str()
                .context("Missing previous app version")?,
        )? >= semver::Version::parse(&journal.staged.release.version)?
        {
            bail!("Cannot verify previous application; installed app and state were retained");
        }
        if !path
            .parent()
            .context("Missing transaction directory")?
            .join("checkpoint/complete.json")
            .is_file()
        {
            bail!("State checkpoint is incomplete; installed app and state were retained");
        }
        validated_checkpoint_files(&path.parent().unwrap().join("checkpoint"))?;
        if !app_pids(&destination)?.is_empty() {
            bail!("Jam reopened during the upgrade; its app and sessions were left unchanged");
        }
        stop_owned_rollback_daemon(
            socket,
            &destination,
            state,
            &journal.token,
            &journal.staged.release.version,
        )?;
        crash_recovery::preserve_interrupted_state(path, state)?;
        // This durable intent must precede both namespace exchange and state
        // restoration. Recovery can finish either side of an interrupted swap.
        save_journal(path, journal, "recovering_previous")?;
        if !app_pids(&destination)?.is_empty() || crash_recovery::daemon_present(socket)? {
            bail!("Jam restarted during rollback; existing app and sessions were left unchanged");
        }
        exchange(&destination, &previous)?;
        after_exchange()?;
        restore_checkpoint_at(path, state)?;
        save_journal(path, journal, "previous_restored")?;
    } else if journal.phase == "recovering_previous" {
        if crash_recovery::daemon_present(socket)? {
            bail!("A daemon started during rollback; existing sessions were left unchanged");
        }
        if !path
            .parent()
            .context("Missing transaction directory")?
            .join("checkpoint/complete.json")
            .is_file()
        {
            bail!("State checkpoint is incomplete; retained application and state");
        }
        restore_checkpoint_at(path, state)?;
        save_journal(path, journal, "previous_restored")?;
    }
    Ok(())
}
fn recover_previous(path: &Path, journal: &Journal) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        let mut rollback = read_journal(path)?;
        rollback_files_at(
            path,
            &mut rollback,
            &crate::state_dir(),
            &crate::socket(),
            || Ok(()),
        )?;
        let destination = &journal.plan.installation;
        let mut restore = Command::new(destination.join("Contents/Resources/bin/jam"));
        restore.arg("__upgrade-restore").arg(path);
        if !path
            .parent()
            .context("Missing transaction directory")?
            .join("checkpoint/complete.json")
            .exists()
            || crash_recovery::daemon_present(&crate::socket())?
        {
            restore.arg("--preserve-intent");
        }
        run(&mut restore)?;
        if !journal.plan.app_pids.is_empty() {
            relaunch(journal)?;
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (path, journal);
    }
    Ok(())
}
fn apply_inner(path: &Path, journal: &mut Journal) -> Result<()> {
    if ["complete", "complete_with_pending"].contains(&journal.phase.as_str()) {
        return Ok(());
    }
    let extracted = unpack(&journal.staged)?;
    #[cfg(target_os = "macos")]
    {
        let destination = journal.plan.installation.clone();
        let marker: serde_json::Value = serde_json::from_slice(&fs::read(
            destination.join("Contents/Resources/release.json"),
        )?)?;
        let installed = semver::Version::parse(
            marker["version"]
                .as_str()
                .context("Missing installed version")?,
        )?;
        let requested = semver::Version::parse(&journal.staged.release.version)?;
        if installed > requested {
            bail!("A newer Jam is already installed; refusing to replace it");
        }
        let candidate = candidate_path(path, journal)?;
        if installed < requested {
            if installed != semver::Version::parse(crate::RELEASE_VERSION)? {
                bail!("Installed version changed; confirm the upgrade again");
            }
            // New sessions after consent require a fresh confirmation.
            revalidate_plan_targets(&mut journal.plan)?;
            let pids = app_pids(&destination)?;
            if pids.iter().any(|p| !journal.plan.app_pids.contains(p)) {
                bail!("Another Jam app started after confirmation; confirm the upgrade again");
            }
            if candidate.exists() {
                fs::remove_dir_all(&candidate)?;
            }
            run(Command::new("/usr/bin/ditto")
                .arg(extracted.join("Jam.app"))
                .arg(&candidate))?;
            sync_tree(&candidate)?;
            save_journal(path, journal, "prepared")?;
            if journal.plan.local_running {
                let target = journal
                    .plan
                    .targets
                    .iter()
                    .find(|target| target.endpoint.is_empty())
                    .context("Missing local upgrade consent")?;
                let expected = journal.plan.installation.join("Contents/MacOS/jamd");
                if crate::protocol::local_daemon_executable(&crate::socket())? != expected {
                    bail!("Local daemon installation changed after consent; left unchanged");
                }
                control(&crate::socket(), admission(target, &journal.token))?;
            }
            // Only stop the exact managed app processes approved in the plan.
            for pid in &pids {
                if process_path(*pid).as_ref() == Some(&destination.join("Contents/MacOS/Jam")) {
                    unsafe {
                        libc::kill(*pid, libc::SIGTERM);
                    }
                }
            }
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !app_pids(&destination)?.is_empty() {
                if std::time::Instant::now() > deadline {
                    bail!("Jam did not quit; no files were replaced");
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            if journal.plan.local_running {
                control(
                    &crate::socket(),
                    crate::Request::StopUpgrade {
                        token: journal.token.clone(),
                    },
                )?;
                wait_stopped(&crate::socket())?;
            }
            checkpoint(path)?;
            if journal.plan.local_running {
                write_json(
                    &crate::state_dir().join(crate::server::UPGRADE_BOOT_FILE),
                    &serde_json::json!({"token":journal.token}),
                )?;
            }
            exchange(&destination, &candidate)?;
            save_journal(path, journal, "activated")?;
        }
        let cli = destination.join("Contents/Resources/bin/jam");
        run(Command::new(&cli).arg("__upgrade-restore").arg(path))?;
        if !journal.plan.app_pids.is_empty() {
            relaunch(journal)?;
        }
        if candidate.exists() {
            fs::remove_dir_all(candidate)?;
        }
    }
    #[cfg(not(target_os = "macos"))]
    {
        // systemd-run places this helper in its own cgroup, outside the daemon
        // being restarted, unlike setsid alone.
        let mut installer = Command::new("python3");
        installer
            .arg(extracted.join("install.py"))
            .arg(&extracted)
            .arg("--restart-active")
            .arg(format!("--transaction-token={}", journal.token))
            .arg(format!(
                "--checkpoint-path={}",
                path.parent()
                    .context("Missing transaction directory")?
                    .join("checkpoint")
                    .display()
            ));
        if journal.plan.local_running {
            use base64::Engine;
            let target = journal
                .plan
                .targets
                .iter()
                .find(|target| target.endpoint.is_empty())
                .context("Missing local consent")?;
            installer.arg(format!(
                "--approved-state={}",
                base64::engine::general_purpose::STANDARD
                    .encode(serde_json::to_vec(&admission(target, &journal.token))?)
            ));
        } else {
            installer.arg("--server-stopped");
        }
        run(&mut installer)?;
        let cli = journal
            .plan
            .installation
            .parent()
            .context("Missing server directory")?
            .join("current/jam");
        run(Command::new(cli).arg("__upgrade-restore").arg(path))?;
    }
    Ok(())
}
pub fn restore(path: &Path) -> Result<()> {
    restore_inner(path, false, false)
}
pub fn restore_local(path: &Path) -> Result<()> {
    restore_inner(path, true, false)
}
pub fn restore_preserving_intent(path: &Path) -> Result<()> {
    restore_inner(path, true, true)
}
fn restore_inner(path: &Path, local_only: bool, preserve_intent: bool) -> Result<()> {
    let mut journal = read_journal(path)?;
    if journal.plan.local_running {
        if std::os::unix::net::UnixStream::connect(crate::socket()).is_err() {
            let exe = std::env::current_exe()?;
            let daemon = if cfg!(target_os = "macos") {
                exe.parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .parent()
                    .unwrap()
                    .join("MacOS/jamd")
            } else {
                exe.parent().unwrap().join("jamd")
            };
            let log = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path.parent().unwrap().join("daemon.log"))?;
            let mut cmd = Command::new(daemon);
            cmd.stdin(Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log);
            use std::os::unix::process::CommandExt;
            unsafe {
                cmd.pre_exec(|| {
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            cmd.spawn()?;
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(15);
        loop {
            if let Ok(crate::Message::HostInfo {
                build,
                release_version,
                ..
            }) = crate::call_at(&crate::socket(), crate::Request::HostInfo)
            {
                let expected = if cfg!(target_os = "macos") {
                    journal.plan.installation.join("Contents/MacOS/jamd")
                } else {
                    fs::canonicalize(
                        journal
                            .plan
                            .installation
                            .parent()
                            .context("Missing server parent")?
                            .join("current/jamd"),
                    )?
                };
                if crate::protocol::local_daemon_executable(&crate::socket())? != expected {
                    bail!("Another installation owns the daemon socket; it was left unchanged");
                }
                if build != crate::SERVER_BUILD
                    || release_version.as_deref() != Some(crate::RELEASE_VERSION)
                {
                    bail!("Another daemon owns the socket; it was left unchanged");
                }
                break;
            }
            if std::time::Instant::now() > deadline {
                bail!("Updated local daemon did not become healthy");
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let already_verified = [
        "local_verified",
        "complete",
        "complete_with_pending",
        "recovered",
    ]
    .contains(&journal.phase.as_str());
    let boot = crate::state_dir().join(crate::server::UPGRADE_BOOT_FILE);
    let held = if journal.plan.local_running && boot.exists() {
        let marker: serde_json::Value = serde_json::from_slice(&fs::read(&boot)?)?;
        if marker["token"] != journal.token {
            bail!("Upgrade boot ownership changed; existing work was left unchanged");
        }
        true
    } else {
        false
    };
    save_journal(path, &mut journal, "local_verified")?;
    if journal.plan.local_running && (!already_verified || held) {
        if !held && crate::RELEASE_VERSION == journal.staged.release.version {
            bail!("Upgrade boot hold is missing; recover the installation before resuming work");
        }
        if held {
            let expected = if cfg!(target_os = "macos") {
                journal.plan.installation.join("Contents/MacOS/jamd")
            } else {
                fs::canonicalize(
                    journal
                        .plan
                        .installation
                        .parent()
                        .context("Missing server parent")?
                        .join("current/jamd"),
                )?
            };
            if !matches!(
                owned_call(
                    &crate::socket(),
                    &expected,
                    crate::Request::CommitUpgrade {
                        token: journal.token.clone()
                    }
                )?,
                crate::Message::Ok
            ) {
                bail!("Unexpected owned upgrade commit response");
            }
        }
    }
    if journal.plan.local_running
        && !preserve_intent
        && (!already_verified || held)
        && let Some(local) = journal.plan.targets.iter().find(|t| t.endpoint.is_empty())
    {
        for pane in &local.panes {
            crate::call_at(
                &crate::socket(),
                crate::Request::Restart { pane: pane.clone() },
            )?;
        }
    }
    #[cfg(target_os = "linux")]
    if crate::RELEASE_VERSION == journal.staged.release.version {
        let source = journal.staged.directory.join("extracted");
        let mut command = Command::new("python3");
        command
            .arg(source.join("install.py"))
            .arg(&source)
            .arg(format!("--commit-transaction={}", journal.token));
        if !journal.plan.local_running {
            command.arg("--server-stopped");
        }
        run(&mut command)?;
    }
    if crate::RELEASE_VERSION != journal.staged.release.version {
        // Rollback restores only the previous local installation. Never update
        // remotes to a release whose local health check just failed.
        journal.error = Some("Previous Jam installation restored".into());
        return save_journal(path, &mut journal, "rolled_back");
    }
    if local_only {
        journal.error = Some(
            "Local installation recovered. Remote upgrades require a fresh confirmation.".into(),
        );
        return save_journal(path, &mut journal, "recovered");
    }
    let pending = restore_remotes(&journal);
    journal.error = (!pending.is_empty()).then(|| pending.join("\n"));
    let phase = if journal.error.is_some() {
        "complete_with_pending"
    } else {
        "complete"
    };
    save_journal(path, &mut journal, phase)
}
fn restore_remotes(journal: &Journal) -> Vec<String> {
    let mut pending = vec![];
    for target in &journal.plan.targets {
        if target.endpoint.is_empty() {
            continue;
        }
        let mut target = target.clone();
        let result = (|| -> Result<()> {
            let (_, current) = connect_remote_target(&target.endpoint, &target.name)?;
            let remote_version = current
                .version
                .as_deref()
                .context("Remote release is unknown; explicit migration required")?;
            if semver::Version::parse(remote_version)?
                >= semver::Version::parse(&journal.staged.release.version)?
            {
                return Ok(());
            }
            revalidate_target(&mut target, &current)?;
            let arch = crate::remote::server_arch(&target.endpoint)?;
            let archive = journal
                .staged
                .directory
                .join(format!("remote-{arch}.tar.gz"));
            let asset = journal
                .staged
                .release
                .assets
                .iter()
                .find(|a| a.platform == "linux" && a.arch == arch)
                .context("Missing remote release artifact")?;
            if !archive.is_file() {
                bail!("Remote artifact was not staged; run the upgrade again");
            }
            verify_file(&archive, asset)?;
            crate::remote::install_release(
                &target.endpoint,
                &archive,
                &serde_json::to_value(admission(&target, &journal.token))?,
            )?;
            crate::remote::disconnect(&target.endpoint);
            let socket = crate::remote::connect(&target.endpoint)?;
            for pane in &target.panes {
                crate::call_at(&socket, crate::Request::Restart { pane: pane.clone() })?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            pending.push(format!("{}: {error:#}", target.name));
        }
    }
    if !journal.plan.offline.is_empty() {
        pending.push(format!(
            "Offline machines require confirmation when reachable: {}",
            journal.plan.offline.join(", ")
        ));
    }
    pending
}
/// Read progress without exposing provider data or requiring another network request.
pub fn status(path: &Path) -> Result<serde_json::Value> {
    let journal = read_journal(path)?;
    Ok(
        serde_json::json!({"version":journal.staged.release.version,"phase":journal.phase,"error":journal.error}),
    )
}
pub fn cli(check_only: bool, yes: bool, json: bool) -> Result<()> {
    let result = check()?;
    if check_only || !result.available {
        if json {
            println!("{}", serde_json::to_string(&result)?);
        } else if let Some(release) = &result.release {
            println!(
                "Jam {} is available (installed {})",
                release.version, result.current
            );
        } else {
            println!("Jam {} is up to date", result.current);
        }
        return Ok(());
    }
    // Refuse developer/unmanaged locations before downloading anything.
    managed(&std::env::current_exe()?)?;
    let staged = stage(result.release.as_ref().unwrap())?;
    let plan = plan(&staged)?;
    eprintln!(
        "Upgrade Jam to {}. Restart {} app instance(s) and {} server(s).",
        staged.release.version,
        plan.app_pids.len(),
        plan.targets.len()
    );
    for target in &plan.targets {
        eprintln!(
            "  {}: {} running terminal(s)",
            target.name,
            target.panes.len()
        );
    }
    if !plan.offline.is_empty() {
        eprintln!("  Pending offline machines: {}", plan.offline.join(", "));
    }
    if !yes {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            bail!(
                "Upgrade requires interruption confirmation; use --yes to explicitly approve restarting running sessions"
            );
        }
        eprint!("Running shell jobs may end. Upgrade and restart now? [y/N] ");
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !["y", "yes"].contains(&answer.trim().to_ascii_lowercase().as_str()) {
            if json {
                println!("{}", serde_json::json!({"cancelled":true}));
            }
            return Ok(());
        }
    }
    let journal = launch(&staged, &plan)?;
    if json {
        println!("{}", serde_json::json!({"started":true,"journal":journal}));
    } else {
        println!("Upgrade started. Progress: {}", journal.display());
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn active_updater_rejects_another_launch_before_creating_a_journal() {
        use std::os::unix::fs::PermissionsExt;
        let fixture = tempfile::tempdir().unwrap();
        fs::set_permissions(fixture.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let upgrades = fixture.path().join("upgrades");
        crate::server::private_dir(&upgrades).unwrap();
        let lock = installation_lock(&upgrades).unwrap();
        lock.lock_exclusive().unwrap();
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "upgrade::tests::launch_admission_child",
                "--ignored",
            ])
            .env("JAM_UPGRADE_ADMISSION_FIXTURE", "1")
            .env("JAM_STATE_DIR", fixture.path())
            .env("JAM_SOCKET", fixture.path().join("unused.sock"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "stdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let entries: Vec<_> = fs::read_dir(&upgrades)
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(
            entries,
            vec![std::ffi::OsString::from("install.lock")],
            "A rejected concurrent launch must not leave an unfinished journal"
        );
    }
    #[test]
    #[ignore = "controlled child with a private profile and no updater helper"]
    fn launch_admission_child() {
        assert_eq!(
            std::env::var("JAM_UPGRADE_ADMISSION_FIXTURE").as_deref(),
            Ok("1")
        );
        let staged = Staged {
            directory: crate::state_dir().join("must-not-be-read"),
            archive: crate::state_dir().join("must-not-be-read.zip"),
            release: Release {
                schema: 1,
                version: "0.0.3".into(),
                protocol: crate::VERSION.into(),
                min_macos: "14.0".into(),
                assets: vec![],
            },
        };
        let plan = Plan {
            installation: crate::state_dir().join("untouched"),
            local_running: false,
            targets: vec![],
            offline: vec![],
            app_pids: vec![],
        };
        let error = launch(&staged, &plan).unwrap_err();
        assert!(
            format!("{error:#}").contains("Another Jam upgrade or recovery is running"),
            "{error:#}"
        );
    }
    use ed25519_dalek::{Signer, SigningKey};
    #[test]
    fn signed_manifest_rejects_tampering_and_unsafe_assets() {
        let key = SigningKey::from_bytes(&[7; 32]);
        let public = hex(&key.verifying_key().to_bytes());
        let mut release = Release {
            schema: 1,
            version: "0.0.2".into(),
            protocol: "jam/test".into(),
            min_macos: "14.0".into(),
            assets: vec![Asset {
                name: "Jam-v0.0.2-macos-arm64.zip".into(),
                platform: "macos".into(),
                arch: "aarch64".into(),
                sha256: "ab".repeat(32),
                size: 3,
            }],
        };
        let bytes = serde_json::to_vec(&release).unwrap();
        let signature = hex(&key.sign(&bytes).to_bytes());
        assert!(authenticate(&bytes, &signature, &public).is_ok());
        let mut damaged = bytes.clone();
        damaged[10] ^= 1;
        assert!(authenticate(&damaged, &signature, &public).is_err());
        release.assets[0].name = "../../bad.zip".into();
        let bytes = serde_json::to_vec(&release).unwrap();
        assert!(authenticate(&bytes, &hex(&key.sign(&bytes).to_bytes()), &public).is_err());
    }
    #[test]
    fn archive_paths_and_checksums_are_verified() {
        assert!(safe_names("Jam.app/Contents/MacOS/Jam\n").is_ok());
        assert!(safe_names("../escape").is_err());
        assert!(safe_names("/absolute").is_err());
        assert!(safe_names("a/../../escape").is_err());
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("archive");
        fs::write(&file, b"good").unwrap();
        let asset = Asset {
            name: "unused".into(),
            platform: "macos".into(),
            arch: "aarch64".into(),
            size: 4,
            sha256: hex(&Sha256::digest(b"good")),
        };
        assert!(verify_file(&file, &asset).is_ok());
        fs::write(&file, b"evil").unwrap();
        assert!(verify_file(&file, &asset).is_err());
    }
    #[test]
    fn upgrade_probe_is_read_only_across_protocol_versions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("socket");
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            crate::write_frame(&mut stream, &crate::Message::Hello("jam/999:future".into()))
                .unwrap();
            assert!(matches!(
                crate::read_frame::<crate::Request>(&mut stream).unwrap(),
                crate::Request::HostInfo
            ));
            crate::write_frame(
                &mut stream,
                &crate::Message::HostInfo {
                    release_version: Some("9.0.0".into()),
                    home: "/tmp".into(),
                    protocol: "jam/999:future".into(),
                    build: "future".into(),
                    engine: "future".into(),
                    incarnation: "future".into(),
                },
            )
            .unwrap();
        });
        assert!(
            matches!(probe(&path,crate::Request::HostInfo).unwrap(),crate::Message::HostInfo{release_version:Some(v),..} if v=="9.0.0")
        );
        server.join().unwrap();
        assert!(
            probe(&path, crate::Request::Stop)
                .unwrap_err()
                .to_string()
                .contains("read-only")
        );
    }
    #[cfg(target_os = "macos")]
    #[test]
    fn whole_bundle_exchange_preserves_both_versions_and_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("Jam.app");
        let staged = dir.path().join("staged.app");
        fs::create_dir(&old).unwrap();
        fs::create_dir(&staged).unwrap();
        fs::write(old.join("version"), "old").unwrap();
        fs::write(staged.join("version"), "new").unwrap();
        exchange(&old, &staged).unwrap();
        assert_eq!(fs::read_to_string(old.join("version")).unwrap(), "new");
        assert_eq!(fs::read_to_string(staged.join("version")).unwrap(), "old");
        exchange(&old, &staged).unwrap();
        assert_eq!(fs::read_to_string(old.join("version")).unwrap(), "old");
        assert!(exchange(&old, &dir.path().join("missing")).is_err());
        assert_eq!(fs::read_to_string(old.join("version")).unwrap(), "old");
    }
    #[test]
    fn journals_reject_public_permissions_before_reading_commands() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let journal = dir.path().join("journal.json");
        fs::write(&journal, "{}").unwrap();
        fs::set_permissions(&journal, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(
            read_journal(&journal)
                .unwrap_err()
                .to_string()
                .contains("private, owned")
        );
    }
    #[test]
    fn state_checkpoint_restores_old_layout_and_database_after_migration_failure() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("state");
        let transaction = dir.path().join("transaction");
        fs::create_dir(&state).unwrap();
        fs::create_dir(&transaction).unwrap();
        let journal = transaction.join("journal.json");
        fs::write(state.join("layout.json"), "old schema").unwrap();
        fs::write(state.join("coordination.sqlite"), "old db").unwrap();
        checkpoint_at(&journal, &state).unwrap();
        fs::write(state.join("layout.json"), "new unreadable schema").unwrap();
        fs::write(state.join("coordination.sqlite"), "new db").unwrap();
        fs::write(state.join("coordination.sqlite-wal"), "new wal").unwrap();
        restore_checkpoint_at(&journal, &state).unwrap();
        assert_eq!(
            fs::read_to_string(state.join("layout.json")).unwrap(),
            "old schema"
        );
        assert_eq!(
            fs::read_to_string(state.join("coordination.sqlite")).unwrap(),
            "old db"
        );
        assert!(!state.join("coordination.sqlite-wal").exists());
    }
    #[test]
    fn checkout_installation_cannot_be_overwritten() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), b"workspace").unwrap();
        let exe = dir.path().join("jam");
        fs::write(&exe, b"bin").unwrap();
        assert!(
            managed(&exe)
                .unwrap_err()
                .to_string()
                .contains("Development checkouts")
        );
    }
}

pub mod crash_recovery;

pub fn recover_cli(yes: bool, json: bool) -> Result<()> {
    let pending = crash_recovery::pending()?;
    if pending.is_empty() {
        if json {
            println!("{}", serde_json::json!({"pending":[]}));
        } else {
            println!("No interrupted Jam upgrades");
        }
        return Ok(());
    }
    for item in &pending {
        eprintln!(
            "Interrupted upgrade to {}: {} ({})",
            item.version,
            item.phase,
            item.journal.display()
        );
    }
    if !yes {
        use std::io::IsTerminal;
        if !std::io::stdin().is_terminal() {
            bail!(
                "Recovery needs fresh confirmation; use --yes to explicitly approve local recovery"
            );
        }
        eprint!("Recover local installations without replaying remote upgrades? [y/N] ");
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !["y", "yes"].contains(&answer.trim().to_ascii_lowercase().as_str()) {
            return Ok(());
        }
    }
    let journals = pending
        .iter()
        .map(|item| crash_recovery::launch_recovery(&item.journal))
        .collect::<Result<Vec<_>>>()?;
    if json {
        println!("{}", serde_json::json!({"recovering":journals}));
    } else {
        println!("Recovery started. Inspect the transaction journals for results.");
    }
    Ok(())
}

#[cfg(test)]
mod remote_tests;
