//! Intelligent hiding: the dock steps aside while a window overlaps it.

use crate::config::Hide;
use nyx_desktop::{Desktop, Monitor};

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
