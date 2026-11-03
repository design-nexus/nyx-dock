//! Intelligent hiding: the dock steps aside while a window overlaps it.

use crate::config::{Hide, Position};
use nyx_desktop::{Desktop, Monitor};

/// The hiding that applies on a workspace: Never keeps the dock up only on floating
/// workspaces; on tiling and scrolling ones, where windows fill the screen, it hides
/// as Smart does.
pub fn mode_here(mode: Hide, floating: bool) -> Hide {
    if mode == Hide::Never && !floating { Hide::Smart } else { mode }
}

/// Room kept between a window and a dock that stays up.
pub const GAP: i32 = 8;

/// Where a floating window goes to stay clear of a dock that stays up: moved off it, and
/// made smaller if it doesn't fit. `area` is the monitor's room for windows (bars left
/// out) and `dock` the bar, both in global logical pixels. `None`: it's clear already.
pub fn clear_of_dock(position: Position, area: Rect, dock: Rect, at: [i32; 2], size: [i32; 2]) -> Option<([i32; 2], [i32; 2])> {
    let [x, y] = at;
    let [w, h] = size;
    match position {
        Position::Bottom => {
            let limit = dock.y - GAP;
            let h = h.min(limit - area.y).max(1);
            (y + size[1] > limit).then_some(([x, limit - h], [w, h]))
        }
        Position::Top => {
            let limit = dock.y + dock.h + GAP;
            let h = h.min(area.y + area.h - limit).max(1);
            (y < limit).then_some(([x, limit], [w, h]))
        }
        Position::Left => {
            let limit = dock.x + dock.w + GAP;
            let w = w.min(area.x + area.w - limit).max(1);
            (x < limit).then_some(([limit, y], [w, h]))
        }
        Position::Right => {
            let limit = dock.x - GAP;
            let w = w.min(limit - area.x).max(1);
            (x + size[0] > limit).then_some(([limit - w, y], [w, h]))
        }
    }
}

/// A rectangle in logical pixels, relative to the monitor's top-left corner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// Whether the dock should be out of the way on `monitor`, given where it sits.
/// Something that keeps it up (the pointer on it, a menu open, a drag) is the caller's
/// business: this only says what the windows want.
pub fn should_hide(mode: Hide, desktop: &Desktop, monitor: &Monitor, dock: Rect) -> bool {
    match mode {
        Hide::Never => false,
        Hide::Always => true,
        Hide::Smart => {
            let fullscreen = desktop.workspaces.iter().any(|w| w.id == monitor.active_workspace && w.has_fullscreen);
            fullscreen
                || desktop
                    .visible_windows(monitor)
                    .any(|w| w.fullscreen != 0 || w.overlaps(monitor.x + dock.x, monitor.y + dock.y, dock.w, dock.h))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyx_desktop::model::Workspace;
    use nyx_desktop::{Monitor, Window};

    fn monitor(x: i32) -> Monitor {
        Monitor {
            id: 0,
            name: "M".into(),
            description: String::new(),
            make: String::new(),
            model: String::new(),
            serial: String::new(),
            width: 1920,
            height: 1080,
            x,
            y: 0,
            scale: 1.0,
            transform: 0,
            focused: true,
            disabled: false,
            active_workspace: 1,
            special_workspace: None,
            reserved: [0, 26, 0, 0],
        }
    }

    fn win(workspace: i64, at: [i32; 2], size: [i32; 2]) -> Window {
        Window {
            address: format!("{at:?}"),
            stable_id: String::new(),
            class: "x".into(),
            initial_class: "x".into(),
            title: String::new(),
            workspace,
            monitor: 0,
            floating: true,
            fullscreen: 0,
            at,
            size,
            pid: 1,
            focus_order: 0,
            pinned: false,
            hidden: false,
            urgent: false,
            grouped: Vec::new(),
        }
    }

    // A 560×70 dock centred at the bottom of a 1920×1080 screen.
    const DOCK: Rect = Rect { x: 680, y: 1000, w: 560, h: 70 };

    fn desktop(windows: Vec<Window>) -> Desktop {
        Desktop { monitors: vec![monitor(0)], windows, connected: true, ..Default::default() }
    }

    #[test]
    fn tiled_window_to_the_bottom_hides_it() {
        let d = desktop(vec![win(1, [10, 36], [1900, 1034])]);
        assert!(should_hide(Hide::Smart, &d, &d.monitors[0], DOCK));
    }

    #[test]
    fn small_floating_window_doesnt() {
        let d = desktop(vec![win(1, [400, 200], [800, 600])]);
        assert!(!should_hide(Hide::Smart, &d, &d.monitors[0], DOCK));
        assert!(should_hide(Hide::Always, &d, &d.monitors[0], DOCK));
    }

    #[test]
    fn only_the_workspace_on_screen_counts() {
        let d = desktop(vec![win(2, [10, 36], [1900, 1034])]);
        assert!(!should_hide(Hide::Smart, &d, &d.monitors[0], DOCK));
    }

    #[test]
    fn second_monitor_offsets() {
        // The same window on a monitor placed at x = 1920 overlaps that monitor's dock.
        let mut d = desktop(vec![win(1, [1930, 36], [1900, 1034])]);
        d.monitors[0] = monitor(1920);
        assert!(should_hide(Hide::Smart, &d, &d.monitors[0], DOCK));
        d.monitors[0] = monitor(0);
        assert!(!should_hide(Hide::Smart, &d, &d.monitors[0], DOCK), "far right of a 0-based monitor");
    }

    #[test]
    fn never_only_on_floating_workspaces() {
        assert_eq!(mode_here(Hide::Never, true), Hide::Never);
        assert_eq!(mode_here(Hide::Never, false), Hide::Smart);
        assert_eq!(mode_here(Hide::Always, true), Hide::Always);
    }

    #[test]
    fn windows_move_above_the_dock() {
        // Below a 26 px bar, above the dock (its top at 1000).
        let area = Rect { x: 0, y: 26, w: 1920, h: 1054 };
        let clear = |at, size| clear_of_dock(Position::Bottom, area, DOCK, at, size);
        assert_eq!(clear([100, 100], [800, 600]), None, "already clear");
        assert_eq!(clear([100, 700], [800, 600]), Some(([100, 392], [800, 600])), "moved up");
        assert_eq!(clear([0, 26], [1920, 1054]), Some(([0, 26], [1920, 966])), "too tall: smaller");
        let left = Rect { x: 0, y: 300, w: 70, h: 480 };
        assert_eq!(clear_of_dock(Position::Left, area, left, [40, 100], [400, 300]), Some(([78, 100], [400, 300])));
    }

    #[test]
    fn fullscreen_hides_it() {
        let mut d = desktop(vec![]);
        d.workspaces.push(Workspace {
            id: 1,
            name: "1".into(),
            monitor: "M".into(),
            windows: 1,
            has_fullscreen: true,
            tiled_layout: String::new(),
        });
        assert!(should_hide(Hide::Smart, &d, &d.monitors[0], DOCK));
        assert!(!should_hide(Hide::Never, &d, &d.monitors[0], DOCK));
    }
}
