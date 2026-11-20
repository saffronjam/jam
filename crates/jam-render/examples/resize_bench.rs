//! Reproducible offscreen resize profiling. Includes real GPU work, not a UI FPS estimate.
use jam_core::vt;
use jam_render::Renderer;
use std::{hint::black_box, sync::Arc, time::Instant};
fn report(name: &str, mut samples: Vec<f64>) {
    samples.sort_by(f64::total_cmp);
    println!(
        "{name}: median={:.3}ms p95={:.3}ms",
        samples[samples.len() / 2],
        samples[samples.len() * 95 / 100]
    );
}
fn main() -> anyhow::Result<()> {
    let mut terminal = vt::terminal(120, 40)?;
    for _ in 0..80 {
        terminal.vt_write("resize workload: 界 café \x1b[32mgreen\x1b[0m\r\n".as_bytes());
    }
    let frame = Arc::new(vt::frame(&terminal)?);
    let mut deep = Vec::new();
    let mut shared = Vec::new();
    for _ in 0..1000 {
        let start = Instant::now();
        black_box((*frame).clone());
        deep.push(start.elapsed().as_secs_f64() * 1000.0);
        let start = Instant::now();
        black_box(Arc::clone(&frame));
        shared.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    report("former owned-frame copy", deep);
    report("shared immutable frame", shared);
    for count in [2, 4] {
        let mut panes = Vec::new();
        for _ in 0..count {
            let mut t = vt::terminal(80, 24)?;
            for _ in 0..40 {
                t.vt_write(b"terminal output during resize\r\n")
            }
            panes.push((t, Renderer::headless(800, 500)?));
        }
        let mut samples = Vec::new();
        for n in 0..60 {
            let start = Instant::now();
            for (t, r) in &mut panes {
                t.resize(70 + (n % 20), 24 + (n % 5), 0, 0)?;
                let f = vt::frame(t)?;
                r.render(&f, 800, 500, 1.0)?;
            }
            samples.push(start.elapsed().as_secs_f64() * 1000.0);
            for (_, r) in &panes {
                black_box(r.read_pixels()?);
            }
        }
        report(
            &format!("{count} panes: VT reflow + frame extraction + GPU submission"),
            samples,
        );
    }
    Ok(())
}
