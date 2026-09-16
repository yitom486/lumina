//! Platform-independent input arbitration for the native mpv surface.
//!
//! Coordinates passed through this module are surface client coordinates with
//! a top-left origin. They are deliberately kept in native surface pixels so
//! the values can be forwarded to mpv's physical-pixel OSD without another
//! platform-specific scale conversion.

/// The default video zone is the only place where a single/double click is
/// surfaced as a playback gesture. OSC hit-testing remains in the Lua script.
pub(crate) const OSC_BOTTOM_ZONE_PX: i32 = 140;
pub(crate) const OSC_MARGIN_PX: i32 = 16;
pub(crate) const OSC_BOTTOM_H_PX: i32 = 68;
pub(crate) const OSC_GAP_PX: i32 = 12;
pub(crate) const OSC_SEEK_H_PX: i32 = 10;
pub(crate) const OSC_SEEK_HIT_PAD_PX: i32 = 12;

/// Demo OSC topbar placeholder estimate (px). Keep this in sync with the
/// metrics at the top of `native/osc/lumina-osc.lua`.
pub(crate) const TOP_ZONE_PX: i32 = 64;
pub(crate) const DRAG_THRESHOLD_PX: i32 = 4;

/// A click that has completed its press/release pair but is waiting for the
/// platform double-click window to expire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PendingClick {
    pub(crate) x: i32,
    pub(crate) y: i32,
}

/// Shared mouse state used by Win32, X11 and AppKit surface adapters.
///
/// Platform modules own delivery of native events and timers. This type owns
/// the state transitions that must remain identical on every platform.
#[derive(Debug, Default)]
pub(crate) struct MouseArbiter {
    pub(crate) left_down: bool,
    right_down: bool,
    dragged: bool,
    suppress_left_up: bool,
    down_at: Option<(i32, i32)>,
    pub(crate) last_pos: (i32, i32),
    pending_click: Option<PendingClick>,
}

impl MouseArbiter {
    pub(crate) fn move_to(&mut self, x: i32, y: i32) -> bool {
        self.last_pos = (x, y);
        if let Some((down_x, down_y)) = self.down_at {
            let dx = i64::from(x) - i64::from(down_x);
            let dy = i64::from(y) - i64::from(down_y);
            if dx * dx + dy * dy >= i64::from(DRAG_THRESHOLD_PX * DRAG_THRESHOLD_PX) {
                self.dragged = true;
            }
        }
        self.left_down && self.dragged
    }

    /// Starts a press and returns a previous pending click that must be
    /// delivered first when the platform reports a second ordinary click
    /// rather than a dedicated double-click event.
    pub(crate) fn left_press(&mut self, x: i32, y: i32) -> (Option<PendingClick>, bool) {
        let pending = self.pending_click.take();
        if self.left_down {
            return (pending, false);
        }
        self.left_down = true;
        self.dragged = false;
        self.suppress_left_up = false;
        self.down_at = Some((x, y));
        self.last_pos = (x, y);
        (pending, true)
    }

    pub(crate) fn left_release(&mut self, x: i32, y: i32) -> (bool, Option<PendingClick>) {
        self.last_pos = (x, y);
        if self.suppress_left_up {
            self.suppress_left_up = false;
            return (false, None);
        }
        if !self.left_down {
            return (false, None);
        }
        self.left_down = false;
        self.down_at = None;
        let dragged = self.dragged;
        self.dragged = false;
        if dragged {
            (true, None)
        } else {
            let click = PendingClick { x, y };
            self.pending_click = Some(click);
            (true, Some(click))
        }
    }

    /// Cancels a pending single click and marks the trailing up as belonging
    /// to the double click. The bool is a safety fallback for native event
    /// sequences that deliver the double-click message before the first up.
    pub(crate) fn double_click(&mut self, x: i32, y: i32) -> bool {
        let had_down = self.left_down;
        self.pending_click = None;
        self.left_down = false;
        self.down_at = None;
        self.dragged = false;
        self.suppress_left_up = true;
        self.last_pos = (x, y);
        had_down
    }

    pub(crate) fn take_timer_click(&mut self) -> Option<PendingClick> {
        self.pending_click.take()
    }

    pub(crate) fn right_press(&mut self, x: i32, y: i32) -> bool {
        self.last_pos = (x, y);
        if self.right_down {
            false
        } else {
            self.right_down = true;
            true
        }
    }

    pub(crate) fn right_release(&mut self, x: i32, y: i32) -> bool {
        self.last_pos = (x, y);
        if self.right_down {
            self.right_down = false;
            true
        } else {
            false
        }
    }

    pub(crate) fn cancel(&mut self) -> bool {
        let had_left_down = self.left_down;
        self.left_down = false;
        self.right_down = false;
        self.dragged = false;
        self.suppress_left_up = false;
        self.down_at = None;
        self.pending_click = None;
        had_left_down
    }

    pub(crate) fn capture_changed(&mut self) -> bool {
        let had_left_down = self.left_down;
        if had_left_down {
            self.cancel();
        }
        had_left_down
    }
}

/// Whether a click at client y belongs to the video gesture zone.
pub(crate) fn should_emit_surface_click(y: i32, height: i32) -> bool {
    if height <= 0 {
        return true;
    }
    if y < TOP_ZONE_PX {
        return false;
    }
    y < height - OSC_BOTTOM_ZONE_PX
}

/// The seek bar is an immediate control and never enters the single/double
/// click disambiguation timer.
pub(crate) fn should_seek_immediately(x: i32, y: i32, width: i32, height: i32) -> bool {
    if width <= 2 * OSC_MARGIN_PX || height <= OSC_MARGIN_PX + OSC_BOTTOM_H_PX {
        return false;
    }
    let bar_x = OSC_MARGIN_PX;
    let bar_w = width - 2 * OSC_MARGIN_PX;
    let bar_y = height - OSC_MARGIN_PX - OSC_BOTTOM_H_PX;
    let seek_x = bar_x + OSC_GAP_PX;
    let seek_w = bar_w - 2 * OSC_GAP_PX;
    let seek_y = bar_y + OSC_GAP_PX;
    let hit_y = seek_y - OSC_SEEK_HIT_PAD_PX;
    let hit_h = OSC_SEEK_H_PX + 2 * OSC_SEEK_HIT_PAD_PX;
    seek_w > 0 && x >= seek_x && x <= seek_x + seek_w && y >= hit_y && y <= hit_y + hit_h
}

#[cfg(test)]
mod tests {
    use super::{
        should_emit_surface_click, should_seek_immediately, MouseArbiter, PendingClick, TOP_ZONE_PX,
    };

    #[test]
    fn surface_click_bottom_zone_exemption() {
        let height = 600;
        let cases: [(i32, bool); 11] = [
            (height - 1, false),
            (height - 140, false),
            (height - 141, true),
            (height + 50, false),
            (-10, false),
            (0, false),
            (32, false),
            (TOP_ZONE_PX - 1, false),
            (TOP_ZONE_PX, true),
            (TOP_ZONE_PX + 1, true),
            (100, true),
        ];
        for (y, expected) in cases {
            assert_eq!(
                should_emit_surface_click(y, height),
                expected,
                "y={y} height={height}"
            );
        }
        assert!(should_emit_surface_click(0, 0), "height=0 fail-open");
        assert!(
            should_emit_surface_click(0, -100),
            "negative height fail-open"
        );
    }

    #[test]
    fn seek_bar_click_is_immediate_and_outside_surface_gesture_zone() {
        let width = 1000;
        let height = 700;
        assert!(should_seek_immediately(500, 628, width, height));
        assert!(!should_seek_immediately(500, 600, width, height));
        assert!(!should_seek_immediately(20, 628, width, height));
        assert!(should_emit_surface_click(500, height));
    }

    #[test]
    fn double_click_cancels_first_single_click_and_suppresses_trailing_up() {
        let mut input = MouseArbiter::default();
        assert_eq!(input.left_press(10, 20), (None, true));
        assert_eq!(
            input.left_release(10, 20),
            (true, Some(PendingClick { x: 10, y: 20 }))
        );
        assert!(!input.double_click(10, 20));
        assert_eq!(input.take_timer_click(), None);
        assert_eq!(input.left_release(10, 20), (false, None));
    }

    #[test]
    fn capture_change_after_mouse_up_keeps_pending_single_click() {
        let mut input = MouseArbiter::default();
        assert_eq!(input.left_press(10, 20), (None, true));
        assert_eq!(
            input.left_release(10, 20),
            (true, Some(PendingClick { x: 10, y: 20 }))
        );
        assert!(!input.capture_changed());
        assert_eq!(
            input.take_timer_click(),
            Some(PendingClick { x: 10, y: 20 })
        );
    }

    #[test]
    fn dragging_forwards_release_but_does_not_create_single_click() {
        let mut input = MouseArbiter::default();
        assert_eq!(input.left_press(10, 20), (None, true));
        assert!(!input.move_to(12, 20));
        assert!(input.move_to(20, 20));
        assert_eq!(input.left_release(20, 20), (true, None));
        assert_eq!(input.take_timer_click(), None);
    }

    #[test]
    fn stray_left_up_is_ignored() {
        let mut input = MouseArbiter::default();
        assert_eq!(input.left_release(10, 20), (false, None));
    }

    #[test]
    fn duplicate_left_down_does_not_create_an_unpaired_input_event() {
        let mut input = MouseArbiter::default();
        assert_eq!(input.left_press(10, 20), (None, true));
        assert_eq!(input.left_press(10, 20), (None, false));
        assert!(input.left_release(10, 20).0);
    }
}
