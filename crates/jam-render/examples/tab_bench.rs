//! Renderer construction/first-frame benchmark, isolated from user terminals.
use jam_core::vt;
use jam_render::Renderer;
use std::time::Instant;
fn main() -> anyhow::Result<()> {
    daemon_benchmark()?;
    let mut samples = Vec::new();
    let mut draw = Vec::new();
    let mut terminal = vt::terminal(80, 24)?;
    terminal.vt_write(b"ready\r\n");
    let frame = vt::frame(&terminal)?;
    for i in 0..21 {
        let start = Instant::now();
        let mut renderer = Renderer::headless(800, 500)?;
        let elapsed = start.elapsed().as_secs_f64() * 1000.;
        if i == 0 {
            println!("cold renderer: {elapsed:.3}ms");
        } else {
            samples.push(elapsed);
        }
        let start = Instant::now();
        renderer.render(&frame, 800, 500, 1.)?;
        if i > 0 {
            draw.push(start.elapsed().as_secs_f64() * 1000.);
        }
    }
    for (name, mut values) in [("warm renderer", samples), ("first frame submission", draw)] {
        values.sort_by(f64::total_cmp);
        println!(
            "{name}: median={:.3}ms p95={:.3}ms",
            values[values.len() / 2],
            values[values.len() * 95 / 100]
        );
    }
    Ok(())
}

struct BenchDaemon {
    child: std::process::Child,
    dir: std::path::PathBuf,
    socket: std::path::PathBuf,
}
impl Drop for BenchDaemon {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
impl BenchDaemon {
    fn request(&self, request: jam_core::Request) -> anyhow::Result<jam_core::Message> {
        let mut stream = jam_core::connect_to(&self.socket)?;
        jam_core::write_frame(&mut stream, &request)?;
        jam_core::read_frame(&mut stream)
    }
}
fn daemon_benchmark() -> anyhow::Result<()> {
    use jam_core::{Message, Request};
    use std::os::unix::fs::PermissionsExt;
    for shell in ["/bin/sh", "/bin/zsh"] {
        if !std::path::Path::new(shell).exists() {
            continue;
        }
        let dir = std::env::temp_dir().join(format!(
            "jam-bench-{}-{}",
            std::process::id(),
            shell.rsplit('/').next().unwrap()
        ));
        std::fs::create_dir(&dir)?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        let socket = dir.join("socket");
        let child = std::process::Command::new("target/debug/jamd")
            .env("JAM_SOCKET", &socket)
            .env("JAM_STATE_DIR", dir.join("state"))
            .env("SHELL", shell)
            .spawn()?;
        let daemon = BenchDaemon { child, dir, socket };
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        while !daemon.socket.exists() {
            anyhow::ensure!(Instant::now() < deadline, "benchmark daemon did not start");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let Message::State(state) = daemon.request(Request::CreateWorkspace {
            path: daemon.dir.to_string_lossy().into_owned(),
            name: "benchmark".into(),
        })?
        else {
            anyhow::bail!("workspace creation failed")
        };
        let workspace = state.workspaces[0].id.clone();
        let (mut create, mut attach) = (Vec::new(), Vec::new());
        for i in 0..20 {
            let start = Instant::now();
            let Message::State(state) = daemon.request(Request::NewTab {
                workspace: workspace.clone(),
            })?
            else {
                anyhow::bail!("new tab failed")
            };
            create.push(start.elapsed().as_secs_f64() * 1000.);
            anyhow::ensure!(
                state.workspaces[0].tabs.len() == i + 2,
                "tab count mismatch"
            );
            let jam_core::Node::Leaf(pane) = &state.workspaces[0].tabs.last().unwrap().root else {
                unreachable!()
            };
            let start = Instant::now();
            anyhow::ensure!(
                matches!(
                    daemon.request(Request::Attach { pane: pane.clone() })?,
                    Message::Snapshot { .. }
                ),
                "snapshot missing"
            );
            attach.push(start.elapsed().as_secs_f64() * 1000.);
        }
        for (label, mut samples) in [
            ("creation with durable save", create),
            ("attach snapshot", attach),
        ] {
            samples.sort_by(f64::total_cmp);
            println!(
                "{shell} {label}: median={:.3}ms p95={:.3}ms",
                samples[samples.len() / 2],
                samples[samples.len() * 95 / 100]
            );
        }
        let _ = daemon.request(Request::Stop);
    }
    Ok(())
}
