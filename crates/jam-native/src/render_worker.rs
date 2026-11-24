use crate::Viewport;
use jam_core::vt::TerminalFrame;
use jam_render::Renderer;
use std::sync::{Arc, Condvar, Mutex};

pub struct Frame {
    pub terminal: Arc<TerminalFrame>,
    pub revision: u64,
    pub viewport: Viewport,
    pub theme: Option<([u8; 3], [u8; 3])>,
    pub cursor: bool,
}

#[derive(Default)]
pub struct RenderResult {
    pub text: Option<String>,
    pub presented: bool,
    pub error: Option<String>,
}

#[derive(Default)]
struct Queue {
    frame: Option<Frame>,
    result: RenderResult,
    stopped: bool,
}

pub struct RenderWorker(Arc<(Mutex<Queue>, Condvar)>);

impl RenderWorker {
    pub fn new(factory: impl FnOnce() -> anyhow::Result<Renderer> + Send + 'static) -> Self {
        let worker = Self(Arc::default());
        let queue = worker.0.clone();
        std::thread::spawn(move || {
            let (lock, signal) = &*queue;
            let mut renderer = match factory() {
                Ok(renderer) => renderer,
                Err(error) => {
                    lock.lock().unwrap().result.error = Some(error.to_string());
                    return;
                }
            };
            let mut last_revision = None;
            loop {
                let frame = {
                    let mut state = signal
                        .wait_while(lock.lock().unwrap(), |state| {
                            state.frame.is_none() && !state.stopped
                        })
                        .unwrap();
                    if state.stopped {
                        return;
                    }
                    state.frame.take().unwrap()
                };
                let Viewport(width, height, scale) = frame.viewport;
                if let Some((foreground, background)) = frame.theme {
                    renderer.set_theme(foreground, background);
                }
                let result = renderer.render_with_cursor(
                    &frame.terminal,
                    width,
                    height,
                    scale,
                    frame.cursor,
                );
                let text = if last_revision != Some(frame.revision) {
                    last_revision = Some(frame.revision);
                    Some(
                        frame
                            .terminal
                            .cells
                            .chunks(frame.terminal.cols.max(1) as usize)
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
                            .join("\n"),
                    )
                } else {
                    None
                };
                let mut state = lock.lock().unwrap();
                state.result.presented |= result.is_ok();
                state.result.error = result.err().map(|error| error.to_string());
                if text.is_some() {
                    state.result.text = text;
                }
            }
        });
        worker
    }

    pub fn submit(&self, frame: Frame) {
        let (lock, signal) = &*self.0;
        lock.lock().unwrap().frame = Some(frame);
        signal.notify_one();
    }

    pub fn result(&self) -> RenderResult {
        let mut state = self.0.0.lock().unwrap();
        RenderResult {
            text: state.result.text.take(),
            presented: state.result.presented,
            error: state.result.error.clone(),
        }
    }

    pub fn discard_pending(&self) {
        self.0.0.lock().unwrap().frame = None;
    }
}

impl Drop for RenderWorker {
    fn drop(&mut self) {
        let (lock, signal) = &*self.0;
        let mut state = lock.lock().unwrap();
        state.stopped = true;
        state.frame = None;
        signal.notify_one();
    }
}
