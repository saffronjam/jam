use jam_core::*;
use jam_render::Renderer;
mod presentation;
use presentation::{Presentation, Viewport};

use std::{
    ffi::{CStr, CString, c_char, c_void},
    os::unix::process::CommandExt,
    thread,
};
fn frame_text(frame: &jam_core::vt::TerminalFrame) -> String {
    frame
        .cells
        .chunks(frame.cols.max(1) as usize)
        .map(|row| {
            row.iter()
                .filter(|cell| cell.wide < 2)
                .map(|cell| {
                    if cell.text.is_empty() {
                        " "
                    } else {
                        &cell.text
                    }
                })
                .collect::<String>()
        })
        .collect::<Vec<_>>()
        .join("\n")
}
fn json_result(f: impl FnOnce() -> anyhow::Result<serde_json::Value>) -> *mut c_char {
    let v = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    let v = match v {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            let mut result = serde_json::json!({"error":format!("{e:#}")});
            if let Some(required) = e.downcast_ref::<jam_core::release_version::UpgradeRequired>() {
                result["upgrade_required"] =
                    serde_json::json!({"current": required.current, "minimum": required.minimum});
            }
            result
        }
        Err(_) => serde_json::json!({"error":"native operation panicked"}),
    };
    CString::new(v.to_string()).unwrap().into_raw()
}
/// # Safety
/// request is a NUL-terminated JSON request. Run lifecycle work on a worker queue.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_server(request: *const c_char) -> *mut c_char {
    json_result(|| {
        let request: serde_json::Value = serde_json::from_str(&unsafe { string(request) })?;
        remote::interactive(request["askpass"].as_str().map(str::to_owned), || {
            server_control::manage(&request)
        })
    })
}
/// # Safety
/// request must be a NUL-terminated JSON object. Configuration work belongs on a worker queue.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_agent_integrations(request: *const c_char) -> *mut c_char {
    json_result(|| {
        let request: serde_json::Value = serde_json::from_str(&unsafe { string(request) })?;
        let action: agents::Action = serde_json::from_value(request["action"].clone())?;
        let selected: Vec<agents::Agent> = request
            .get("agents")
            .cloned()
            .map(serde_json::from_value)
            .transpose()?
            .unwrap_or_default();
        let dry_run = request["dry_run"].as_bool().unwrap_or(false);
        Ok(serde_json::to_value(agents::manage(
            action,
            &selected,
            &agents::Locations::environment()?,
            dry_run,
        )?)?)
    })
}
unsafe fn string(p: *const c_char) -> String {
    if p.is_null() {
        String::new()
    } else {
        unsafe { CStr::from_ptr(p) }.to_string_lossy().into_owned()
    }
}
/// # Safety
/// p must be null or a string returned by Jam, freed exactly once.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_free(p: *mut c_char) {
    if !p.is_null() {
        drop(unsafe { CString::from_raw(p) });
    }
}
/// # Safety
/// p must be null or point to a NUL-terminated UTF-8 request for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_request(p: *const c_char) -> *mut c_char {
    unsafe { jam_request_endpoint(std::ptr::null(), p) }
}
/// # Safety
/// Strings must be null or NUL-terminated UTF-8 for the duration of the call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_request_endpoint(
    endpoint: *const c_char,
    p: *const c_char,
) -> *mut c_char {
    json_result(|| {
        let mut value: serde_json::Value = serde_json::from_str(&unsafe { string(p) })?;
        let focus_key = value
            .get("FocusPane")
            .and_then(|v| v["pane"].as_str())
            .map(str::to_owned);
        let endpoint = jam_core::remote::request_endpoint(&value)
            .unwrap_or_else(|| unsafe { string(endpoint) });
        let path = if endpoint.is_empty() {
            socket()
        } else {
            jam_core::remote::local_socket(&endpoint)?
        };
        if !endpoint.is_empty() {
            jam_core::remote::scope(&mut value, &endpoint, false)?;
        }
        let revision = value
            .as_object_mut()
            .and_then(|v| v.remove("_revision"))
            .and_then(|v| v.as_u64());
        let mut req: Request = serde_json::from_value(value)?;
        if let Some(revision) = revision
            && matches!(
                req,
                Request::MovePane { .. }
                    | Request::MoveTab { .. }
                    | Request::Ratio { .. }
                    | Request::ReorderWorkspaces { .. }
            )
        {
            req = Request::Guarded {
                revision,
                request: Box::new(req),
            };
        }
        let reply = if matches!(
            req,
            Request::Key { .. }
                | Request::Send { .. }
                | Request::Paste { .. }
                | Request::Mouse { .. }
        ) {
            jam_core::input::send(&path, req)?
        } else {
            call_at(&path, req)?
        };
        if let Message::HostInfo {
            ref incarnation, ..
        } = reply
        {
            input::observe_server(&path, incarnation);
        }
        let mut response = serde_json::to_value(reply)?;
        if let Some(key) = focus_key {
            *FOCUS.lock().unwrap().entry(key).or_default() += 1;
        }
        if !endpoint.is_empty() {
            jam_core::remote::scope(&mut response, &endpoint, true)?;
        }
        Ok(response)
    })
}
/// # Safety
/// p must point to a NUL-terminated JSON command for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_remote(p: *const c_char) -> *mut c_char {
    json_result(|| {
        let v: serde_json::Value = serde_json::from_str(&unsafe { string(p) })?;
        let id = v["id"].as_str().unwrap_or("");
        match v["action"].as_str().unwrap_or("") {
            "enable_persistence" => {
                jam_core::remote::enable_persistence(id)?;
                Ok(serde_json::json!({"ok":true}))
            }
            "rename" => {
                jam_core::remote::rename(id, v["name"].as_str().unwrap_or("").into())?;
                Ok(serde_json::json!({"ok":true}))
            }
            "list" => Ok(serde_json::to_value(jam_core::remote::list()?)?),
            "add" => Ok(serde_json::to_value(jam_core::remote::add(
                v["name"].as_str().unwrap_or("").into(),
                v["destination"].as_str().unwrap_or("").into(),
            )?)?),
            "connect" => {
                let (path, persistent) = jam_core::remote::interactive(
                    v["askpass"].as_str().map(str::to_owned),
                    || {
                        jam_core::remote::ensure(
                            id,
                            std::path::Path::new(v["artifacts"].as_str().unwrap_or("")),
                        )
                    },
                )?;
                let host = call_at(&path, Request::HostInfo)?;
                if let Message::HostInfo {
                    ref incarnation, ..
                } = host
                {
                    input::observe_server(&path, incarnation);
                }
                let mut reply = serde_json::to_value(host)?;
                reply
                    .as_object_mut()
                    .unwrap()
                    .insert("persistent".into(), persistent.into());
                Ok(reply)
            }
            "install" => {
                jam_core::remote::install(
                    id,
                    std::path::Path::new(v["archive"].as_str().unwrap_or("")),
                )?;
                Ok(serde_json::json!({"ok":true}))
            }
            "disconnect" => {
                jam_core::remote::disconnect(id);
                Ok(serde_json::json!({"ok":true}))
            }
            "remove" => {
                jam_core::remote::disconnect(id);
                jam_core::remote::remove(id)?;
                Ok(serde_json::json!({"ok":true}))
            }
            _ => anyhow::bail!("Unknown remote command"),
        }
    })
}
/// # Safety
/// path must be null or point to a NUL-terminated daemon path for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_start(path: *const c_char) -> *mut c_char {
    json_result(|| {
        if let Err(e) = connect() {
            if std::os::unix::net::UnixStream::connect(socket()).is_ok() {
                return Err(e);
            }
            let exe = unsafe { string(path) };
            let dir = state_dir();
            jam_core::server::private_dir(&dir)?;
            let log = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(dir.join("jamd.log"))?;
            let mut cmd = std::process::Command::new(exe);
            cmd.stdin(std::process::Stdio::null())
                .stdout(log.try_clone()?)
                .stderr(log);
            unsafe {
                cmd.pre_exec(|| {
                    if libc::setsid() < 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
            let mut child = cmd.spawn()?;
            thread::spawn(move || {
                let _ = child.wait();
            });
            for _ in 0..100 {
                if connect().is_ok() {
                    Renderer::prepare()?;
                    return Ok(serde_json::json!({"ok":true}));
                }
                thread::sleep(std::time::Duration::from_millis(50));
            }
            anyhow::bail!("daemon did not start; inspect jamd.log")
        };
        Renderer::prepare()?;
        Ok(serde_json::json!({"ok":true}))
    })
}

static FOCUS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, u64>>> =
    std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));

static LAST_FRAMES: std::sync::Mutex<
    std::collections::VecDeque<(String, std::sync::Arc<jam_core::vt::TerminalFrame>)>,
> = std::sync::Mutex::new(std::collections::VecDeque::new());
fn cache_frame(pane: String, frame: std::sync::Arc<jam_core::vt::TerminalFrame>) {
    let mut frames = LAST_FRAMES.lock().unwrap();
    frames.retain(|(id, _)| id != &pane);
    if !frame.cells.is_empty() && frame.cells.len() <= 1_000_000 {
        frames.push_back((pane, frame));
    }
    while frames.len() > 32 || frames.iter().map(|(_, f)| f.cells.len()).sum::<usize>() > 1_000_000
    {
        frames.pop_front();
    }
}
struct View {
    presentation: Presentation,
    theme_changed: bool,
    renderer: Renderer,
    mirror: jam_core::client::Mirror,
    pane: String,
    key: String,
    focus_revision: u64,
    last_size: (u16, u16),
    revision: u64,
    generation: u64,
    last_blink: std::time::Instant,
    blink: bool,
}
/// # Safety
/// nsview must be a live NSView retained until jam_view_drop, on the main thread; pane must be NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_view_new(
    nsview: *mut c_void,
    pane: *const c_char,
    width: u32,
    height: u32,
) -> *mut c_void {
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| -> anyhow::Result<View> {
            let pane = unsafe { string(pane) };
            let renderer = unsafe { Renderer::appkit(nsview, width, height) }?;
            let (endpoint, raw) = pane.split_once('|').unwrap_or(("", &pane));
            let path = if endpoint.is_empty() {
                socket()
            } else {
                jam_core::remote::local_socket(endpoint)?
            };
            let mirror = jam_core::client::Mirror::at(raw.into(), path);
            if let Some((_, frame)) = LAST_FRAMES
                .lock()
                .unwrap()
                .iter()
                .find(|(id, _)| id == &pane)
            {
                let mut display = mirror.display.lock().unwrap();
                if display.generation == 0 {
                    display.frame = frame.clone();
                    display.revision += 1;
                }
            }
            Ok(View {
                presentation: Presentation::default(),
                theme_changed: false,
                renderer,
                mirror,
                pane: raw.into(),
                key: pane.clone(),
                focus_revision: 0,
                last_size: (0, 0),
                revision: 0,
                generation: 0,
                last_blink: std::time::Instant::now(),
                blink: true,
            })
        }))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("renderer initialization panicked")));
    match result {
        Ok(v) => Box::into_raw(Box::new(v)).cast(),
        Err(e) => {
            eprintln!("jam view: {e:#}");
            std::ptr::null_mut()
        }
    }
}
/// # Safety
/// ptr must be a live Jam view handle, called exclusively on AppKit's main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_view_invalidate(ptr: *mut c_void) {
    if let Some(view) = unsafe { ptr.cast::<View>().as_mut() } {
        view.presentation.invalidate();
    }
}
/// # Safety
/// ptr must be a live Jam view handle, called exclusively on AppKit's main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_view_set_colors(ptr: *mut c_void, foreground: u32, background: u32) {
    if ptr.is_null() {
        return;
    }
    let rgb = |color: u32| [(color >> 16) as u8, (color >> 8) as u8, color as u8];
    let view = unsafe { &mut *ptr.cast::<View>() };
    view.theme_changed |= view.renderer.set_theme(rgb(foreground), rgb(background));
}
/// # Safety
/// ptr must be a live Jam view handle, called exclusively on AppKit main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_view_draw(
    ptr: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut c_char {
    json_result(|| {
        if ptr.is_null() || !scale.is_finite() || scale <= 0.0 {
            anyhow::bail!("invalid view geometry")
        };
        let v = unsafe { &mut *ptr.cast::<View>() };
        let size = (
            ((width as f32 / (8.4 * scale).round()) as u16).clamp(1, 1000),
            ((height as f32 / (19.0 * scale).round()) as u16).clamp(1, 1000),
        );
        let generation = v.mirror.display.lock().unwrap().generation;
        if generation != v.generation {
            v.generation = generation;
            v.last_size = (0, 0);
        }
        let focus_revision = *FOCUS.lock().unwrap().get(&v.key).unwrap_or(&0);
        if focus_revision != v.focus_revision {
            v.focus_revision = focus_revision;
            v.last_size = (0, 0);
        }
        let resized = size != v.last_size;
        if resized
            && v.mirror.request(Request::Resize {
                pane: v.pane.clone(),
                cols: size.0,
                rows: size.1,
            })
        {
            v.last_size = size;
        }
        let d = v.mirror.display.lock().unwrap();
        let error = d.error.clone();
        let ready = v.presentation.ready(
            (d.frame.cols, d.frame.rows),
            size,
            std::time::Instant::now(),
        );
        let screen_text = (ready && d.revision != v.revision).then(|| frame_text(&d.frame));
        let meta = serde_json::json!({"text":screen_text,"error":error,"ready":ready,"mouse":d.frame.mouse_tracking,"alternate":d.frame.alternate,"cols":d.frame.cols,"rows":d.frame.rows,"cursor_x":d.frame.cursor.0,"cursor_y":d.frame.cursor.1});
        if !ready {
            return Ok(meta);
        }
        let viewport = Viewport(width, height, scale);
        let blink = v.last_blink.elapsed() >= std::time::Duration::from_millis(500);
        if d.revision != v.revision
            || blink
            || resized
            || v.theme_changed
            || v.presentation.changed(viewport)
        {
            let f = d.frame.clone();
            let revision = d.revision;
            drop(d);
            if blink {
                v.blink = !v.blink;
                v.last_blink = std::time::Instant::now();
            }
            // Use the same rounded cell metrics as input and PTY sizing. Scaling
            // an old frame to fit changes the font and causes a second visible jump.
            v.renderer
                .render_with_cursor(&f, width, height, scale, !f.cursor_blink || v.blink)?;
            v.presentation.did_present(viewport);
            v.revision = revision;
            v.theme_changed = false;
        }
        Ok(meta)
    })
}
/// # Safety
/// ptr must be a live Jam view handle on the main thread; action must be NUL-terminated.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_view_action(
    ptr: *mut c_void,
    action: *const c_char,
    x: i32,
    y: i32,
) -> *mut c_char {
    json_result(|| {
        if ptr.is_null() {
            anyhow::bail!("missing terminal view")
        }
        let v = unsafe { &mut *ptr.cast::<View>() };
        let action = unsafe { string(action) };
        match action.as_str() {
            "clear_selection" => v.mirror.clear_selection(),
            "bottom" => v.mirror.bottom(),
            "scroll" => v.mirror.scroll(y as isize),
            "select" | "extend" => {
                let cols = v.mirror.display.lock().unwrap().frame.cols.max(1) as usize;
                v.mirror.select(
                    y.max(0) as usize * cols + x.max(0) as usize,
                    action == "extend",
                )
            }
            "copy" => return Ok(serde_json::json!({"text":v.mirror.selected_text()})),
            _ => anyhow::bail!("unknown view action"),
        };
        Ok(serde_json::json!({"ok":true}))
    })
}
/// # Safety
/// p must be null or a live Jam view handle, dropped exactly once on AppKit main thread.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_view_drop(p: *mut c_void) {
    if !p.is_null() {
        let view = unsafe { Box::from_raw(p.cast::<View>()) };
        cache_frame(
            view.key.clone(),
            view.mirror.display.lock().unwrap().frame.clone(),
        );
        drop(view)
    }
}

/// # Safety
/// request must be a NUL-terminated UTF-8 string valid for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_config(request: *const c_char) -> *mut c_char {
    json_result(|| {
        let text = unsafe { string(request) };
        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&text)
            && let Some(template) = v["worktree_template"].as_str()
        {
            let template = jam_core::worktree_template::normalize(template)?;
            let path = jam_core::worktree_template::render(
                &template,
                v["repo"].as_str().unwrap_or("jam"),
                v["worktree"].as_str().unwrap_or("feature/login"),
                std::path::Path::new(v["home"].as_str().unwrap_or("/home/example")),
            )?;
            return Ok(serde_json::json!({"template":template,"path":path}));
        }
        if text == "snapshot" {
            return jam_core::config::snapshot();
        }
        if text == "revision" {
            return Ok(
                serde_json::json!({"revision":jam_core::config::revision_at(&jam_core::config::path())?}),
            );
        }
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&text)
            && value.get("expected_revision").is_some()
        {
            let c: jam_core::config::Config = serde_json::from_value(value["config"].clone())?;
            jam_core::config::save_checked(
                &c,
                value["expected_revision"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing revision"))?,
            )?;
            return jam_core::config::snapshot();
        }
        if text == "defaults" {
            return Ok(serde_json::to_value(jam_core::config::Config::default())?);
        }
        if text != "get" {
            let c: jam_core::config::Config = serde_json::from_str(&text)?;
            jam_core::config::save(&c)?;
        }
        Ok(serde_json::to_value(jam_core::config::load()?)?)
    })
}

/// # Safety
/// ptr must be a live Jam view handle on the main thread; request must be NUL-terminated.
/// This only queues work or reads a small result summary; history scanning uses background workers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_view_find(ptr: *mut c_void, request: *const c_char) -> *mut c_char {
    json_result(|| {
        anyhow::ensure!(!ptr.is_null(), "missing terminal view");
        let v = unsafe { &mut *ptr.cast::<View>() };
        let r: serde_json::Value = serde_json::from_str(&unsafe { string(request) })?;
        match r["action"].as_str().unwrap_or("poll") {
            "start" => v.mirror.find(
                r["id"].as_u64().unwrap_or(0),
                r["query"].as_str().unwrap_or("").to_string(),
            ),
            "select" => v.mirror.find_select(
                r["id"].as_u64().unwrap_or(0),
                r["index"].as_u64().unwrap_or(0) as usize,
            ),
            "cancel" => {
                v.mirror.search.cancel();
                v.mirror.clear_selection();
            }
            _ => {}
        }
        Ok(serde_json::to_value(v.mirror.search.status())?)
    })
}

/// Shared release updater API. All actions may block and must run off the UI thread.
/// Staging is non-disruptive; launch requires an explicit confirmation flag.
/// # Safety
/// request must point to a NUL-terminated UTF-8 JSON value for this call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn jam_upgrade(request: *const c_char) -> *mut c_char {
    json_result(|| {
        let value: serde_json::Value = serde_json::from_str(&unsafe { string(request) })?;
        match value["action"].as_str().unwrap_or("") {
            "eligibility" => Ok(serde_json::json!({
                "installation": jam_core::upgrade::managed_installation()?,
            })),
            "check" => Ok(serde_json::to_value(jam_core::upgrade::check()?)?),
            "pending" => Ok(serde_json::to_value(
                jam_core::upgrade::crash_recovery::pending()?,
            )?),
            "recover" => {
                anyhow::ensure!(
                    value["confirmed"] == true,
                    "Recovery requires interruption confirmation"
                );
                let journal = value["journal"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing upgrade journal"))?;
                let journal = jam_core::upgrade::crash_recovery::launch_recovery(
                    std::path::Path::new(journal),
                )?;
                Ok(serde_json::json!({"journal": journal}))
            }
            "stage" => {
                let release = serde_json::from_value(value["release"].clone())?;
                let staged = jam_core::upgrade::stage(&release)?;
                let plan = jam_core::upgrade::plan(&staged)?;
                Ok(serde_json::json!({"staged": staged, "plan": plan}))
            }
            "launch" => {
                anyhow::ensure!(
                    value["confirmed"] == true,
                    "Upgrade requires interruption confirmation"
                );
                let staged = serde_json::from_value(value["staged"].clone())?;
                let plan = serde_json::from_value(value["plan"].clone())?;
                let journal = jam_core::upgrade::launch(&staged, &plan)?;
                Ok(serde_json::json!({"journal": journal}))
            }
            "status" => {
                let journal = value["journal"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("Missing upgrade journal"))?;
                jam_core::upgrade::status(std::path::Path::new(journal))
            }
            _ => anyhow::bail!("Unknown upgrade action"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn updater_launch_requires_explicit_confirmation_before_reading_plan() {
        let request =
            std::ffi::CString::new(r#"{"action":"launch","staged":null,"plan":null}"#).unwrap();
        let pointer = unsafe { jam_upgrade(request.as_ptr()) };
        assert!(!pointer.is_null());
        let result: serde_json::Value = serde_json::from_str(
            unsafe { std::ffi::CStr::from_ptr(pointer) }
                .to_str()
                .unwrap(),
        )
        .unwrap();
        unsafe { jam_free(pointer) };
        assert!(result["error"].as_str().unwrap().contains("confirmation"));
    }
    #[test]
    fn updater_recovery_requires_confirmation() {
        let request =
            std::ffi::CString::new(r#"{"action":"recover","journal":"/nonexistent"}"#).unwrap();
        let pointer = unsafe { jam_upgrade(request.as_ptr()) };
        let result: serde_json::Value = serde_json::from_str(
            unsafe { std::ffi::CStr::from_ptr(pointer) }
                .to_str()
                .unwrap(),
        )
        .unwrap();
        unsafe { jam_free(pointer) };
        assert!(result["error"].as_str().unwrap().contains("confirmation"));
    }
    #[test]
    fn updater_rejects_unknown_actions() {
        let request = std::ffi::CString::new(r#"{"action":"force-install"}"#).unwrap();
        let pointer = unsafe { jam_upgrade(request.as_ptr()) };
        let result: serde_json::Value = serde_json::from_str(
            unsafe { std::ffi::CStr::from_ptr(pointer) }
                .to_str()
                .unwrap(),
        )
        .unwrap();
        unsafe { jam_free(pointer) };
        assert!(
            result["error"]
                .as_str()
                .unwrap()
                .contains("Unknown upgrade action")
        );
    }
    #[test]
    fn recent_frames_are_pane_scoped_and_bounded() {
        let mut terminal = jam_core::vt::terminal(2, 2).unwrap();
        terminal.vt_write(b"x");
        let frame = std::sync::Arc::new(jam_core::vt::frame(&terminal).unwrap());
        for i in 0..40 {
            cache_frame(format!("test-pane-{i}"), frame.clone());
        }
        let frames = LAST_FRAMES.lock().unwrap();
        assert_eq!(frames.len(), 32);
        assert!(frames.iter().all(|(id, _)| id != "test-pane-0"));
        assert!(
            frames
                .iter()
                .find(|(id, _)| id == "test-pane-39")
                .unwrap()
                .1
                .cells[0]
                .text
                .contains('x')
        );
        assert!(frames.iter().all(|(id, _)| id != "other-pane"));
    }
}

#[cfg(test)]
mod release_error_tests {
    use super::*;
    #[test]
    fn newer_remote_errors_expose_upgrade_requirement() {
        let ptr = json_result(|| {
            jam_core::release_version::require_client_at_least(Some("0.0.2"), "0.0.1")?;
            Ok(serde_json::Value::Null)
        });
        let text = unsafe { CString::from_raw(ptr) };
        let reply: serde_json::Value = serde_json::from_slice(text.as_bytes()).unwrap();
        assert_eq!(reply["upgrade_required"]["minimum"], "0.0.2");
        assert_eq!(reply["upgrade_required"]["current"], "0.0.1");
        assert!(reply["error"].as_str().unwrap().contains("left unchanged"));
    }
}
