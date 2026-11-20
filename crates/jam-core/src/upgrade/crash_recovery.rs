//! Recovery is a new, local-only operation. Saved interruption consent is never
//! replayed, and a responding daemon is never stopped by this module.
use super::*;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pending {
    pub journal: PathBuf,
    pub version: String,
    pub phase: String,
    pub error: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Permit {
    created: u64,
    app_pids: Vec<i32>,
}
fn terminal(phase: &str) -> bool {
    matches!(
        phase,
        "complete" | "complete_with_pending" | "rolled_back" | "recovered" | "abandoned"
    )
}
fn private_directory(path: &Path) -> Result<()> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        bail!("Recovery directory must be private and owned by this user");
    }
    Ok(())
}
fn transaction(path: &Path) -> Result<()> {
    transaction_at(path, &crate::state_dir().join("upgrades"))
}
fn transaction_at(path: &Path, expected: &Path) -> Result<()> {
    let parent = path.parent().context("Missing recovery transaction")?;
    if path.file_name() != Some(std::ffi::OsStr::new("journal.json"))
        || parent.parent() != Some(expected)
    {
        bail!("Recovery journal is outside this Jam profile");
    }
    let name = parent
        .file_name()
        .and_then(|s| s.to_str())
        .context("Invalid transaction name")?;
    uuid::Uuid::parse_str(
        name.strip_prefix("apply-")
            .context("Invalid recovery transaction")?,
    )?;
    private_directory(expected)?;
    private_directory(parent)
}
/// Enumerate interrupted transactions without creating files, downloading
/// releases, changing skipped versions, or attempting an installation.
pub fn pending() -> Result<Vec<Pending>> {
    pending_at(&crate::state_dir().join("upgrades"))
}
fn pending_at(directory: &Path) -> Result<Vec<Pending>> {
    match directory.symlink_metadata() {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Err(error) => return Err(error.into()),
        Ok(_) => private_directory(directory)?,
    }
    // A running helper may temporarily have a nonterminal journal while it
    // relaunches the app. Do not offer recovery for its live transaction.
    let lock = match fs::File::open(directory.join("install.lock")) {
        Ok(lock) => Some(lock),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    if let Some(lock) = &lock {
        match lock.try_lock_exclusive() {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => return Ok(vec![]),
            Err(error) => return Err(error.into()),
        }
    }
    let mut result = vec![];
    for entry in fs::read_dir(directory)? {
        let path = entry?.path().join("journal.json");
        if transaction_at(&path, directory).is_err() || !path.exists() {
            continue;
        }
        let journal = read_journal(&path)?;
        if !terminal(&journal.phase) {
            result.push(Pending {
                journal: path,
                version: journal.staged.release.version,
                phase: journal.phase,
                error: journal.error,
            });
        }
    }
    result.sort_by(|a, b| a.journal.cmp(&b.journal));
    Ok(result)
}
/// Called only after fresh interruption confirmation. Its short-lived permit
/// authorizes this detached repair, not another upgrade or any remote restart.
pub fn launch_recovery(path: &Path) -> Result<PathBuf> {
    transaction(path)?;
    let journal = read_journal(path)?;
    if terminal(&journal.phase) {
        return Ok(path.into());
    }
    let directory = path.parent().unwrap();
    write_json(
        &directory.join("recovery-permit.json"),
        &Permit {
            created: SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(),
            app_pids: app_pids(&journal.plan.installation)?,
        },
    )?;
    let helper = directory.join(format!("jam-recovery-{}", crate::id()));
    fs::copy(crate::cli_path()?, &helper)?;
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .mode(0o600)
        .open(directory.join("recovery.log"))?;
    let mut command = if cfg!(target_os = "linux") {
        let mut command = Command::new("systemd-run");
        command
            .args(["--user", "--collect", "--quiet", "--unit"])
            .arg(format!("jam-recovery-{}", crate::id()));
        for (key, value) in &journal.environment {
            if PROFILE_ENV.contains(&key.as_str()) {
                command.arg(format!("--setenv={key}={value}"));
            }
        }
        command.arg(helper);
        command
    } else {
        Command::new(helper)
    };
    command
        .arg("__upgrade-recover")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log);
    for (key, value) in &journal.environment {
        if PROFILE_ENV.contains(&key.as_str()) {
            command.env(key, value);
        }
    }
    use std::os::unix::process::CommandExt;
    unsafe {
        command.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = command.spawn()?;
    if cfg!(target_os = "linux") && !child.wait()?.success() {
        bail!("Could not start detached recovery service");
    }
    Ok(path.into())
}
fn permit(path: &Path) -> Result<Permit> {
    let permit_path = path.parent().unwrap().join("recovery-permit.json");
    let mut options = fs::OpenOptions::new();
    let file = options
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&permit_path)?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        bail!("Invalid recovery confirmation");
    }
    let mut bytes = vec![];
    file.take(65536).read_to_end(&mut bytes)?;
    let permit: Permit = serde_json::from_slice(&bytes)?;
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    if permit.created > now || now - permit.created > 30 {
        bail!("Recovery confirmation expired; confirm recovery again");
    }
    fs::remove_file(permit_path)?;
    Ok(permit)
}
pub(super) fn daemon_present(socket: &Path) -> Result<bool> {
    match std::os::unix::net::UnixStream::connect(socket) {
        Ok(_) => Ok(true),
        Err(error)
            if matches!(
                error.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::ConnectionRefused
            ) =>
        {
            Ok(false)
        }
        Err(error) => {
            Err(error).context("Could not establish local daemon ownership; left unchanged")
        }
    }
}
#[cfg(target_os = "macos")]
fn installed_version(path: &Path) -> Result<semver::Version> {
    let marker: serde_json::Value =
        serde_json::from_slice(&fs::read(path.join("Contents/Resources/release.json"))?)?;
    Ok(semver::Version::parse(
        marker["version"]
            .as_str()
            .context("Missing installed release")?,
    )?)
}
fn finish(path: &Path, journal: &mut Journal, message: &str) -> Result<()> {
    journal.error = Some(message.into());
    save_journal(path, journal, "recovered")
}
fn local_restore(path: &Path, journal: &Journal, cli: &Path, preserve_intent: bool) -> Result<()> {
    let mut command = Command::new(cli);
    command
        .arg("__upgrade-restore")
        .arg(path)
        .arg("--local-only");
    if preserve_intent {
        command.arg("--preserve-intent");
    }
    for (key, value) in &journal.environment {
        if PROFILE_ENV.contains(&key.as_str()) {
            command.env(key, value);
        }
    }
    run(&mut command)
}
fn prepare_boot(journal: &Journal, state: &Path) -> Result<()> {
    let boot = state.join(crate::server::UPGRADE_BOOT_FILE);
    if boot.exists() {
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&boot)?)?;
        if value["token"].as_str() != Some(journal.token.as_str()) {
            bail!("Another transaction owns local startup; left unchanged");
        }
    } else {
        write_json(&boot, &serde_json::json!({"token":journal.token}))?;
    }
    Ok(())
}
/// A fresh permit is consumed once. An old upgrade's token is used only for
/// aborting its still-owned drain; it never authorizes a stop or remote update.
pub fn recover(path: &Path) -> Result<()> {
    transaction(path)?;
    let lock = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW)
        .open(root()?.join("install.lock"))?;
    lock.try_lock_exclusive()
        .context("Another Jam upgrade or recovery is running")?;
    let mut journal = read_journal(path)?;
    if terminal(&journal.phase) {
        return Ok(());
    }
    let permit = permit(path)?;
    let result = recover_local(
        path,
        &mut journal,
        &permit,
        &crate::state_dir(),
        &crate::socket(),
    );
    if let Err(ref error) = result {
        journal.error = Some(format!("Recovery needs attention: {error:#}"));
        // Keep the original phase: a retry must still distinguish verified new
        // state from an interrupted migration, even when repair itself fails.
        write_json(path, &journal)?;
    }
    result
}
#[cfg(target_os = "macos")]
fn recover_local(
    path: &Path,
    journal: &mut Journal,
    permit: &Permit,
    state: &Path,
    socket: &Path,
) -> Result<()> {
    let destination = journal.plan.installation.clone();
    let installed = installed_version(&destination)?;
    let requested = semver::Version::parse(&journal.staged.release.version)?;
    let preserve_intent = journal.phase == "staged"
        || matches!(
            journal.phase.as_str(),
            "local_verified" | "complete" | "complete_with_pending"
        )
        || (journal.phase == "prepared"
            && !path
                .parent()
                .unwrap()
                .join("checkpoint/complete.json")
                .exists());
    if installed > requested {
        return finish(
            path,
            journal,
            "A newer Jam installation superseded this upgrade; it was left unchanged.",
        );
    }
    if daemon_present(socket)? {
        let expected = destination.join("Contents/MacOS/jamd");
        let crate::Message::HostInfo {
            release_version,
            build,
            ..
        } = probe(socket, crate::Request::HostInfo)?
        else {
            bail!("Could not verify the running daemon; left unchanged");
        };
        if crate::protocol::local_daemon_executable(socket)? != expected
            || release_version.as_deref() != Some(installed.to_string().as_str())
        {
            bail!("Another installation owns the local daemon; left unchanged");
        }
        let output = Command::new(&expected).arg("--build-info").output()?;
        let metadata: serde_json::Value = serde_json::from_slice(&output.stdout)?;
        if !output.status.success() || metadata["build"].as_str() != Some(build.as_str()) {
            bail!("Running daemon build differs from the installed release; left unchanged");
        }
        let boot = state.join(".upgrade-boot.json");
        if boot.exists() {
            let metadata = boot.symlink_metadata()?;
            if !metadata.is_file()
                || metadata.uid() != unsafe { libc::geteuid() }
                || metadata.mode() & 0o077 != 0
            {
                bail!("Cannot verify interrupted startup ownership; left unchanged");
            }
            let value: serde_json::Value = serde_json::from_slice(&fs::read(&boot)?)?;
            if value["token"].as_str() != Some(journal.token.as_str()) {
                bail!("Another transaction owns local startup; left unchanged");
            }
            save_journal(path, journal, "local_verified")?;
            local_restore(
                path,
                journal,
                &destination.join("Contents/Resources/bin/jam"),
                false,
            )?;
            return finish(
                path,
                journal,
                "Verified local startup recovered. Remote updates require fresh confirmation.",
            );
        }
        if installed < requested {
            owned_call(
                socket,
                &expected,
                crate::Request::AbortUpgrade {
                    token: journal.token.clone(),
                },
            )?;
        }
        return finish(
            path,
            journal,
            "Running local sessions were preserved. Remote updates require fresh confirmation.",
        );
    }
    if installed == requested
        && !matches!(
            journal.phase.as_str(),
            "local_verified" | "complete" | "complete_with_pending"
        )
    {
        let previous = candidate_path(path, journal)?;
        if installed_version(&previous)? >= installed {
            bail!("The previous app cannot be verified; installed app and state were retained");
        }
        if !path
            .parent()
            .unwrap()
            .join("checkpoint/complete.json")
            .is_file()
        {
            bail!("State checkpoint is incomplete; installed app and state were retained");
        }
        validated_checkpoint_files(&path.parent().unwrap().join("checkpoint"))?;
        let pids = app_pids(&destination)?;
        if pids.iter().any(|pid| !permit.app_pids.contains(pid)) {
            bail!("Another Jam app started after recovery confirmation; left unchanged");
        }
        for pid in pids {
            if process_path(pid).as_ref() == Some(&destination.join("Contents/MacOS/Jam")) {
                unsafe {
                    libc::kill(pid, libc::SIGTERM);
                }
            }
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        while !app_pids(&destination)?.is_empty() {
            if std::time::Instant::now() >= deadline {
                bail!("Jam did not quit; files were left unchanged");
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        if daemon_present(socket)? {
            bail!("A local daemon started during recovery; left unchanged");
        }
        preserve_interrupted_state(path, state)?;
        save_journal(path, journal, "recovering_previous")?;
        exchange(&destination, &previous)?;
        restore_checkpoint_at(path, state)?;
    } else if installed < requested && journal.phase == "recovering_previous" {
        if !path
            .parent()
            .unwrap()
            .join("checkpoint/complete.json")
            .is_file()
        {
            bail!("State checkpoint is incomplete; retained application and state");
        }
        restore_checkpoint_at(path, state)?;
    }
    if journal.plan.local_running {
        prepare_boot(journal, state)?;
        local_restore(
            path,
            journal,
            &destination.join("Contents/Resources/bin/jam"),
            preserve_intent,
        )?;
    }
    finish(
        path,
        journal,
        "Local installation recovered. Remote updates require fresh confirmation.",
    )?;
    if !journal.plan.app_pids.is_empty() {
        relaunch(journal)?;
    }
    Ok(())
}
#[cfg(not(target_os = "macos"))]
fn recover_local(
    path: &Path,
    journal: &mut Journal,
    _permit: &Permit,
    state: &Path,
    socket: &Path,
) -> Result<()> {
    let base = journal
        .plan
        .installation
        .parent()
        .context("Missing server installation")?;
    let current = fs::canonicalize(base.join("current"))?;
    let receipt = base
        .join("transactions")
        .join(format!("{}.json", journal.token));
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(current.join("manifest.json"))?)?;
    if daemon_present(socket)? {
        let target = state_target("", "Local", socket)?;
        if crate::protocol::local_daemon_executable(socket)? != current.join("jamd")
            || target.version.as_deref() != metadata["release_version"].as_str()
            || metadata["server_build"].as_str() != Some(target.build.as_str())
        {
            bail!("Another server owns the local socket; left unchanged");
        }
        let boot = state.join(crate::server::UPGRADE_BOOT_FILE);
        let held = boot.exists();
        if held {
            let marker: serde_json::Value = serde_json::from_slice(&fs::read(&boot)?)?;
            if marker["token"].as_str() != Some(journal.token.as_str()) {
                bail!("Another transaction owns local startup; left unchanged");
            }
            save_journal(path, journal, "local_verified")?;
        } else if target.version.as_deref() != Some(journal.staged.release.version.as_str()) {
            owned_call(
                socket,
                &current.join("jamd"),
                crate::Request::AbortUpgrade {
                    token: journal.token.clone(),
                },
            )?;
        }
        if receipt.exists() {
            // This also finalizes a verified Linux installer receipt. Already
            // committed daemons never receive old pane restart requests.
            local_restore(path, journal, &current.join("jam"), !held)?;
        }
        return finish(
            path,
            journal,
            "Running local sessions were preserved. Remote updates require fresh confirmation.",
        );
    }
    if !receipt.exists() {
        return finish(
            path,
            journal,
            "Upgrade stopped before installation; local state was left unchanged.",
        );
    }
    let extracted = unpack(&journal.staged)?;
    run(Command::new("python3")
        .arg(extracted.join("install.py"))
        .arg(&extracted)
        .arg(format!("--recover-transaction={}", journal.token))
        .arg("--server-stopped"))?;
    // The receipt distinguishes rollback from health-verified activation. Its
    // validated local recovery never replays remote installation consent.
    let recovered: serde_json::Value = serde_json::from_slice(&fs::read(&receipt)?)?;
    let preserve = recovered["phase"] != "rolled_back";
    if journal.plan.local_running {
        prepare_boot(journal, state)?;
    }
    local_restore(path, journal, &base.join("current/jam"), preserve)?;
    finish(
        path,
        journal,
        "Local server recovered. Remote updates require fresh confirmation.",
    )
}
#[cfg(target_os = "macos")]
pub(super) fn preserve_interrupted_state(path: &Path, state: &Path) -> Result<()> {
    let backup = path.parent().unwrap().join("interrupted-state");
    if backup.join("complete.json").exists() {
        return Ok(());
    }
    crate::server::private_dir(&backup)?;
    for name in CHECKPOINT_FILES {
        if state.join(name).exists() {
            fs::copy(state.join(name), backup.join(name))?;
            fs::File::open(backup.join(name))?.sync_all()?;
        }
    }
    write_json(&backup.join("complete.json"), &true)
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    fn bundle(path: &Path, version: &str) {
        fs::create_dir_all(path.join("Contents/Resources/bin")).unwrap();
        fs::create_dir_all(path.join("Contents/MacOS")).unwrap();
        write_json(
            &path.join("Contents/Resources/release.json"),
            &serde_json::json!({"version":version}),
        )
        .unwrap();
        let cli = path.join("Contents/Resources/bin/jam");
        fs::write(
            &cli,
            b"#!/bin/sh\nprintf '%s\\n' \"$@\" > \"$(dirname \"$0\")/restore-arguments\"\n[ \"$1\" = __upgrade-restore ] && [ \"$3\" = --local-only ]\n",
        )
        .unwrap();
        fs::set_permissions(cli, fs::Permissions::from_mode(0o700)).unwrap();
    }
    fn data(state: &Path, label: &str) {
        fs::create_dir_all(state).unwrap();
        fs::write(state.join("layout.json"), label).unwrap();
        let db = rusqlite::Connection::open(state.join("coordination.sqlite")).unwrap();
        db.execute_batch("CREATE TABLE IF NOT EXISTS evidence(value TEXT); DELETE FROM evidence;")
            .unwrap();
        db.execute("INSERT INTO evidence VALUES(?)", [label])
            .unwrap();
    }
    fn evidence(state: &Path) -> String {
        let db = rusqlite::Connection::open(state.join("coordination.sqlite")).unwrap();
        db.query_row("SELECT value FROM evidence", [], |row| row.get(0))
            .unwrap()
    }
    fn fixture(root: &Path, phase: &str, new: bool) -> (PathBuf, Journal, PathBuf, PathBuf) {
        let transaction = root.join("transaction");
        fs::create_dir(&transaction).unwrap();
        let path = transaction.join("journal.json");
        let installation = root.join("Jam.app");
        bundle(&installation, if new { "0.0.2" } else { "0.0.1" });
        let state = root.join("state");
        data(&state, "old");
        let journal = Journal {
            schema: 1,
            token: crate::id(),
            staged: Staged {
                directory: root.join("stage"),
                archive: root.join("unused.zip"),
                release: Release {
                    schema: 1,
                    version: "0.0.2".into(),
                    protocol: crate::VERSION.into(),
                    min_macos: "14.0".into(),
                    assets: vec![],
                },
            },
            plan: Plan {
                installation,
                local_running: true,
                targets: vec![],
                offline: vec![],
                app_pids: vec![],
            },
            phase: phase.into(),
            error: None,
            environment: Default::default(),
        };
        write_json(&path, &journal).unwrap();
        let socket = root.join("absent.sock");
        (path, journal, state, socket)
    }
    #[test]
    fn automatic_rollback_crash_after_exchange_has_durable_intent_for_fresh_recovery() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut journal, state, socket) = fixture(dir.path(), "activated", true);
        journal.plan.local_running = false;
        checkpoint_at(&path, &state).unwrap();
        data(&state, "migrated");
        bundle(&candidate_path(&path, &journal).unwrap(), "0.0.1");
        let error = rollback_files_at(&path, &mut journal, &state, &socket, || {
            bail!("injected termination after exchange")
        })
        .unwrap_err();
        assert!(error.to_string().contains("injected termination"));
        let mut persisted: Journal = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted.phase, "recovering_previous");
        assert_eq!(
            installed_version(&persisted.plan.installation)
                .unwrap()
                .to_string(),
            "0.0.1"
        );
        assert_eq!(evidence(&state), "migrated");
        recover_local(
            &path,
            &mut persisted,
            &Permit {
                created: 0,
                app_pids: vec![],
            },
            &state,
            &socket,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(state.join("layout.json")).unwrap(),
            "old"
        );
        assert_eq!(evidence(&state), "old");
        assert_eq!(
            evidence(&path.parent().unwrap().join("interrupted-state")),
            "migrated"
        );
    }
    #[test]
    fn checkpoint_copy_failure_after_automatic_exchange_can_be_retried_safely() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut journal, state, socket) = fixture(dir.path(), "activated", true);
        journal.plan.local_running = false;
        checkpoint_at(&path, &state).unwrap();
        data(&state, "migrated");
        bundle(&candidate_path(&path, &journal).unwrap(), "0.0.1");
        let checkpoint = path.parent().unwrap().join("checkpoint/layout.json");
        assert!(
            rollback_files_at(&path, &mut journal, &state, &socket, || {
                fs::remove_file(&checkpoint)?;
                fs::create_dir(&checkpoint)?;
                Ok(())
            })
            .is_err()
        );
        let mut persisted: Journal = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted.phase, "recovering_previous");
        assert_eq!(
            installed_version(&persisted.plan.installation)
                .unwrap()
                .to_string(),
            "0.0.1"
        );
        fs::remove_dir(&checkpoint).unwrap();
        fs::write(&checkpoint, "old").unwrap();
        recover_local(
            &path,
            &mut persisted,
            &Permit {
                created: 0,
                app_pids: vec![],
            },
            &state,
            &socket,
        )
        .unwrap();
        assert_eq!(
            fs::read_to_string(state.join("layout.json")).unwrap(),
            "old"
        );
        assert_eq!(evidence(&state), "old");
    }
    #[test]
    fn automatic_rollback_never_sends_stop_to_foreign_responding_kernel() {
        use std::sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        };
        let dir = tempfile::tempdir().unwrap();
        let (path, mut journal, state, socket) = fixture(dir.path(), "activated", true);
        checkpoint_at(&path, &state).unwrap();
        data(&state, "migrated");
        bundle(&candidate_path(&path, &journal).unwrap(), "0.0.1");
        fs::write(
            journal.plan.installation.join("Contents/MacOS/jamd"),
            b"not the connected kernel",
        )
        .unwrap();
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        listener.set_nonblocking(true).unwrap();
        let running = Arc::new(AtomicBool::new(true));
        let done = running.clone();
        let (sent, received) = std::sync::mpsc::channel();
        let server = std::thread::spawn(move || {
            while done.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        stream.set_nonblocking(false).unwrap();
                        stream
                            .set_read_timeout(Some(Duration::from_millis(100)))
                            .unwrap();
                        let _ = crate::write_frame(
                            &mut stream,
                            &crate::Message::Hello(crate::VERSION.into()),
                        );
                        if let Ok(request) = crate::read_frame::<crate::Request>(&mut stream) {
                            sent.send(request).unwrap();
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(1))
                    }
                    Err(error) => panic!("{error}"),
                }
            }
        });
        let result = rollback_files_at(&path, &mut journal, &state, &socket, || Ok(()));
        running.store(false, Ordering::SeqCst);
        server.join().unwrap();
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Another installation")
        );
        assert!(
            received.try_iter().next().is_none(),
            "No request, particularly Stop, may reach the foreign kernel"
        );
        assert_eq!(
            installed_version(&journal.plan.installation)
                .unwrap()
                .to_string(),
            "0.0.2"
        );
        assert_eq!(evidence(&state), "migrated");
        let persisted: Journal = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(persisted.phase, "activated");
    }
    #[test]
    fn repair_after_staging_or_stop_preserves_old_state() {
        for phase in ["staged", "prepared"] {
            let dir = tempfile::tempdir().unwrap();
            let (path, mut journal, state, socket) = fixture(dir.path(), phase, false);
            if phase == "prepared" {
                checkpoint_at(&path, &state).unwrap();
            }
            recover_local(
                &path,
                &mut journal,
                &Permit {
                    created: 0,
                    app_pids: vec![],
                },
                &state,
                &socket,
            )
            .unwrap();
            assert_eq!(
                installed_version(&journal.plan.installation)
                    .unwrap()
                    .to_string(),
                "0.0.1"
            );
            assert_eq!(
                fs::read_to_string(state.join("layout.json")).unwrap(),
                "old"
            );
            assert_eq!(evidence(&state), "old");
            assert_eq!(journal.phase, "recovered");
            let arguments = fs::read_to_string(
                journal
                    .plan
                    .installation
                    .join("Contents/Resources/bin/restore-arguments"),
            )
            .unwrap();
            assert_eq!(arguments.contains("--preserve-intent"), phase == "staged");
        }
    }
    #[test]
    fn crash_after_swap_restores_old_layout_and_board_and_retains_interrupted_state() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut journal, state, socket) = fixture(dir.path(), "activated", true);
        checkpoint_at(&path, &state).unwrap();
        data(&state, "failed-new-migration");
        bundle(&candidate_path(&path, &journal).unwrap(), "0.0.1");
        recover_local(
            &path,
            &mut journal,
            &Permit {
                created: 0,
                app_pids: vec![],
            },
            &state,
            &socket,
        )
        .unwrap();
        assert_eq!(
            installed_version(&journal.plan.installation)
                .unwrap()
                .to_string(),
            "0.0.1"
        );
        assert_eq!(evidence(&state), "old");
        assert_eq!(
            fs::read_to_string(state.join("layout.json")).unwrap(),
            "old"
        );
        let retained = path.parent().unwrap().join("interrupted-state");
        assert_eq!(evidence(&retained), "failed-new-migration");
        assert_eq!(
            installed_version(&candidate_path(&path, &journal).unwrap())
                .unwrap()
                .to_string(),
            "0.0.2"
        );
    }
    #[test]
    fn crash_after_health_keeps_new_layout_and_board() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut journal, state, socket) = fixture(dir.path(), "local_verified", true);
        checkpoint_at(&path, &state).unwrap();
        data(&state, "verified-new-work");
        bundle(&candidate_path(&path, &journal).unwrap(), "0.0.1");
        recover_local(
            &path,
            &mut journal,
            &Permit {
                created: 0,
                app_pids: vec![],
            },
            &state,
            &socket,
        )
        .unwrap();
        assert_eq!(
            installed_version(&journal.plan.installation)
                .unwrap()
                .to_string(),
            "0.0.2"
        );
        assert_eq!(evidence(&state), "verified-new-work");
        assert!(
            fs::read_to_string(
                journal
                    .plan
                    .installation
                    .join("Contents/Resources/bin/restore-arguments")
            )
            .unwrap()
            .contains("--preserve-intent")
        );
        assert_eq!(
            fs::read_to_string(state.join("layout.json")).unwrap(),
            "verified-new-work"
        );
    }
    #[test]
    fn incomplete_checkpoint_cannot_replace_application_or_state() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut journal, state, socket) = fixture(dir.path(), "activated", true);
        bundle(&candidate_path(&path, &journal).unwrap(), "0.0.1");
        data(&state, "new");
        assert!(
            recover_local(
                &path,
                &mut journal,
                &Permit {
                    created: 0,
                    app_pids: vec![]
                },
                &state,
                &socket
            )
            .is_err()
        );
        assert_eq!(
            installed_version(&journal.plan.installation)
                .unwrap()
                .to_string(),
            "0.0.2"
        );
        assert_eq!(evidence(&state), "new");
    }
    #[test]
    fn responding_unrelated_daemon_is_never_stopped_or_rolled_back() {
        let dir = tempfile::tempdir().unwrap();
        let (path, mut journal, state, socket) = fixture(dir.path(), "activated", true);
        checkpoint_at(&path, &state).unwrap();
        data(&state, "new-work");
        bundle(&candidate_path(&path, &journal).unwrap(), "0.0.1");
        let listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let peer = std::thread::spawn(move || {
            // Presence check, read-only metadata probe, and kernel identity
            // lookup each connect once. No Stop message is ever accepted.
            drop(listener.accept().unwrap());
            let (mut stream, _) = listener.accept().unwrap();
            crate::write_frame(&mut stream, &crate::Message::Hello(crate::VERSION.into())).unwrap();
            assert!(matches!(
                crate::read_frame::<crate::Request>(&mut stream).unwrap(),
                crate::Request::HostInfo
            ));
            crate::write_frame(
                &mut stream,
                &crate::Message::HostInfo {
                    home: "/irrelevant".into(),
                    protocol: crate::VERSION.into(),
                    build: "unrelated".into(),
                    release_version: Some("0.0.2".into()),
                    engine: "test".into(),
                    incarnation: crate::id(),
                },
            )
            .unwrap();
            drop(stream);
            drop(listener.accept().unwrap());
        });
        let error = recover_local(
            &path,
            &mut journal,
            &Permit {
                created: 0,
                app_pids: vec![],
            },
            &state,
            &socket,
        )
        .unwrap_err();
        assert!(error.to_string().contains("Another installation"));
        peer.join().unwrap();
        assert_eq!(evidence(&state), "new-work");
        assert_eq!(
            installed_version(&journal.plan.installation)
                .unwrap()
                .to_string(),
            "0.0.2"
        );
    }
    #[test]
    fn recovery_confirmation_expires_and_is_single_use() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("journal.json");
        let permit_path = dir.path().join("recovery-permit.json");
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        write_json(
            &permit_path,
            &Permit {
                created: now - 31,
                app_pids: vec![],
            },
        )
        .unwrap();
        assert!(permit(&path).is_err());
        write_json(
            &permit_path,
            &Permit {
                created: now,
                app_pids: vec![],
            },
        )
        .unwrap();
        assert!(permit(&path).is_ok());
        assert!(permit(&path).is_err());
    }
}

#[cfg(test)]
mod detection_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    #[test]
    fn detection_does_not_create_an_absent_profile() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("absent/upgrades");
        assert!(pending_at(&path).unwrap().is_empty());
        assert!(!temporary.path().join("absent").exists());
    }
    #[test]
    fn live_helper_suppresses_incomplete_journal_without_reading_it() {
        let temporary = tempfile::tempdir().unwrap();
        fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o700)).unwrap();
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(temporary.path().join("install.lock"))
            .unwrap();
        lock.lock_exclusive().unwrap();
        let transaction = temporary.path().join(format!("apply-{}", crate::id()));
        fs::create_dir(&transaction).unwrap();
        fs::write(transaction.join("journal.json"), b"not yet complete").unwrap();
        assert!(pending_at(temporary.path()).unwrap().is_empty());
        assert_eq!(
            fs::read(transaction.join("journal.json")).unwrap(),
            b"not yet complete"
        );
        assert_eq!(lock.metadata().unwrap().len(), 0);
    }
}
