use std::sync::Arc;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    window::{Window, WindowId},
};
struct App {
    window: Option<Arc<Window>>,
    renderer: Option<jam_render::Renderer>,
}
impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(Window::default_attributes().with_title("Jam renderer validation"))
                .unwrap(),
        );
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window.clone()).unwrap();
        self.renderer = Some(
            jam_render::Renderer::with_surface(instance, Some(surface), size.width, size.height)
                .unwrap(),
        );
        window.request_redraw();
        self.window = Some(window);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        if matches!(event, WindowEvent::RedrawRequested) {
            let mut t = jam_core::vt::terminal(40, 10).unwrap();
            t.vt_write(
                b"Jam Linux surface: real terminal state\r\n\x1b[32mVulkan + glyphon\x1b[0m",
            );
            let frame = jam_core::vt::frame(&t).unwrap();
            let size = self.window.as_ref().unwrap().inner_size();
            self.renderer
                .as_mut()
                .unwrap()
                .render(&frame, size.width, size.height, 1.0)
                .unwrap();
            if self.renderer.as_ref().unwrap().presented_frames() > 0 {
                event_loop.exit();
            } else {
                self.window.as_ref().unwrap().request_redraw();
            }
        }
    }
}
fn main() {
    EventLoop::new()
        .unwrap()
        .run_app(&mut App {
            window: None,
            renderer: None,
        })
        .unwrap();
}
