use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
pub struct Viewport(pub u32, pub u32, pub f32);

/// The first snapshot may precede the daemon's resize response. Do not flash it
/// at the wrong size. A passive client must still display a pane whose geometry
/// belongs to another client, so waiting is bounded.
#[derive(Default)]
pub struct Presentation {
    waiting_since: Option<Instant>,
    presented: bool,
    viewport: Option<Viewport>,
}

impl Presentation {
    pub fn ready(&mut self, frame: (u16, u16), target: (u16, u16), now: Instant) -> bool {
        let since = *self.waiting_since.get_or_insert(now);
        frame.0 > 0
            && frame.1 > 0
            && (self.presented
                || frame == target
                || now.duration_since(since) >= Duration::from_millis(250))
    }

    pub fn changed(&self, viewport: Viewport) -> bool {
        self.viewport != Some(viewport)
    }

    pub fn invalidate(&mut self) {
        self.viewport = None;
    }

    pub fn did_present(&mut self, viewport: Viewport) {
        self.presented = true;
        self.viewport = Some(viewport);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_frame_waits_for_geometry_but_cached_frames_do_not() {
        let now = Instant::now();
        let mut presentation = Presentation::default();
        assert!(!presentation.ready((80, 24), (100, 30), now));
        assert!(presentation.ready((100, 30), (100, 30), now));
        let viewport = Viewport(800, 570, 1.0);
        presentation.did_present(viewport);
        assert!(!presentation.changed(viewport));
        // Reattachment and a sub-cell resize both require another GPU submission.
        presentation.invalidate();
        assert!(presentation.changed(viewport));
        assert!(presentation.ready((100, 30), (120, 40), now));
        presentation.did_present(viewport);
        assert!(presentation.changed(Viewport(801, 570, 1.0)));
        assert!(presentation.changed(Viewport(800, 570, 1.01)));
    }

    #[test]
    fn geometry_owned_by_another_client_does_not_leave_a_blank_view() {
        let now = Instant::now();
        let mut presentation = Presentation::default();
        assert!(!presentation.ready((80, 24), (100, 30), now));
        assert!(presentation.ready((80, 24), (100, 30), now + Duration::from_millis(250)));
        assert!(!presentation.ready((0, 0), (100, 30), now + Duration::from_secs(1)));
    }
}
