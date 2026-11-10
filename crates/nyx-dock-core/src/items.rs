//! What the dock shows: pinned apps in pin order, then running apps that aren't pinned,
//! in the order they started.

use crate::config::Running;
use nyx_desktop::apps::{Index, key_for};
use nyx_desktop::{Desktop, Monitor, Window};

/// Omarchy minimizes a window by moving it to this special workspace.
pub const MINIMIZED: &str = "special:minimized";

#[derive(Debug, Clone, PartialEq)]
pub struct Item {
    /// Desktop id, or `class:<class>` for a window no app claims.
    pub key: String,
    pub pinned: bool,
    /// Window addresses, most recently focused first.
    pub windows: Vec<String>,
    /// The focused window is one of this app's.
    pub focused: bool,
    pub urgent: bool,
}

impl Item {
    pub fn running(&self) -> bool {
        !self.windows.is_empty()
    }
}

/// Whether the dock counts a window: on a user workspace (1–10, or a virtual monitor's
/// own) or a special one (scratchpad, minimized). Windows on internal workspaces (test
/// outputs, Settings' loop placeholders, future agent workspaces) never show.
pub fn counts(desktop: &Desktop, w: &Window) -> bool {
    nyx_desktop::is_user_workspace(w.workspace) || desktop.workspaces.iter().any(|ws| ws.id == w.workspace && (ws.is_user() || ws.is_special()))
}

/// Build the items. `order` is the running apps' order from last time (by key), so
/// apps keep their places as windows come and go; new ones go at the end. Returns the
/// items and the new order.
pub fn compose(
    pinned: &[String],
    desktop: &Desktop,
    index: &Index,
    running: Running,
    monitor: &Monitor,
    order: &[String],
) -> (Vec<Item>, Vec<String>) {
    let mut items: Vec<Item> =
        pinned.iter().map(|p| Item { key: p.clone(), pinned: true, windows: vec![], focused: false, urgent: false }).collect();
    let mut running_keys: Vec<String> = Vec::new();
    // Windows come most recently focused first.
    for w in desktop.windows.iter().filter(|w| counts(desktop, w)) {
        if running == Running::Monitor && w.monitor != monitor.id {
            continue;
        }
        let key = key_for(index, &w.class, &w.initial_class);
        let focused = desktop.active_window.as_deref() == Some(w.address.as_str());
        match items.iter_mut().find(|i| i.key == key) {
            Some(item) => {
                item.windows.push(w.address.clone());
                item.focused |= focused;
                item.urgent |= w.urgent;
            }
            None => {
                running_keys.push(key.clone());
                items.push(Item { key, pinned: false, windows: vec![w.address.clone()], focused, urgent: w.urgent });
            }
        }
    }
    // Unpinned running apps: known ones in their old order, new ones after.
    let first_running = pinned.len();
    let mut tail: Vec<Item> = items.split_off(first_running);
    tail.sort_by_key(|i| order.iter().position(|k| *k == i.key).unwrap_or(usize::MAX));
    let new_order: Vec<String> = tail.iter().map(|i| i.key.clone()).collect();
    items.extend(tail);
    (items, new_order)
}

/// The window a click should bring forward: the most recently focused one on the
/// workspace in front of the user, else the most recently focused anywhere.
pub fn pick_window<'a>(desktop: &'a Desktop, item: &Item, monitor: &Monitor) -> Option<&'a Window> {
    let windows: Vec<&Window> = item.windows.iter().filter_map(|a| desktop.window(a)).collect();
    windows.iter().find(|w| w.workspace == monitor.active_workspace).or(windows.first()).copied()
}

/// After the app's newest window, the next one, for clicking an app that already has
/// focus (cycling through its windows).
pub fn next_window<'a>(desktop: &'a Desktop, item: &Item) -> Option<&'a Window> {
    let active = desktop.active_window.as_deref()?;
    let pos = item.windows.iter().position(|a| a == active)?;
    // Least recently focused: cycling then visits every window in turn.
    let next = item.windows.iter().rev().find(|a| **a != item.windows[pos])?;
    desktop.window(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nyx_desktop::apps::Entry;
    use nyx_desktop::model::Workspace;

    fn window(address: &str, class: &str, workspace: i64, monitor: i64, focus: i32) -> Window {
        Window {
            address: address.into(),
            stable_id: String::new(),
            class: class.into(),
            initial_class: class.into(),
            title: String::new(),
            workspace,
            monitor,
            floating: false,
            fullscreen: 0,
            at: [0, 0],
            size: [100, 100],
            pid: 1,
            focus_order: focus,
            pinned: false,
            hidden: false,
            urgent: false,
            grouped: Vec::new(),
        }
    }

    fn monitor(id: i64, active: i64) -> Monitor {
        Monitor {
            id,
            name: format!("M{id}"),
            description: String::new(),
            make: String::new(),
            model: String::new(),
            serial: String::new(),
            width: 1920,
            height: 1080,
            x: 0,
            y: 0,
            scale: 1.0,
            transform: 0,
            focused: true,
            disabled: false,
            active_workspace: active,
            special_workspace: None,
            reserved: [0; 4],
        }
    }

    fn desktop() -> Desktop {
        let mut d = Desktop { connected: true, ..Default::default() };
        d.monitors = vec![monitor(0, 1), monitor(1, 3)];
        d.workspaces = vec![Workspace {
            id: -99,
            name: MINIMIZED.into(),
            monitor: "M0".into(),
            windows: 1,
            has_fullscreen: false,
            tiled_layout: String::new(),
        }];
        d.windows = vec![
            window("a", "firefox", 1, 0, 0),
            window("b", "kitty", 3, 1, 1),
            window("c", "firefox", 2, 0, 2),
            window("d", "io.github.design_nexus.nyx.Files", -99, 0, 3),
            window("t", "firefox", 201, 1, 4),
        ];
        d.active_window = Some("a".into());
        d
    }

    fn index() -> Index {
        Index::new(&[
            Entry { id: "firefox".into(), exec: "firefox %u".into(), ..Default::default() },
            Entry { id: "io.github.design_nexus.nyx.Files".into(), exec: "nyx-files".into(), ..Default::default() },
        ])
    }

    #[test]
    fn pinned_first_then_running_in_order() {
        let d = desktop();
        let pins = vec!["io.github.design_nexus.nyx.Files".to_string(), "io.github.design_nexus.nyx.Mail".to_string()];
        let (items, order) = compose(&pins, &d, &index(), Running::All, &d.monitors[0], &[]);
        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, vec!["io.github.design_nexus.nyx.Files", "io.github.design_nexus.nyx.Mail", "firefox", "class:kitty"]);
        assert_eq!(items[0].windows, vec!["d"], "a minimized window still runs");
        assert!(!items[1].running());
        assert_eq!(items[2].windows, vec!["a", "c"], "not the test output's window");
        assert!(items[2].focused);
        assert_eq!(order, vec!["firefox", "class:kitty"]);

        // Kitty started first last time: it keeps its place.
        let (items, _) = compose(&pins, &d, &index(), Running::All, &d.monitors[0], &["class:kitty".into(), "firefox".into()]);
        assert_eq!(items[2].key, "class:kitty");
    }

    #[test]
    fn this_monitor_only() {
        let d = desktop();
        let (items, _) = compose(&[], &d, &index(), Running::Monitor, &d.monitors[1], &[]);
        let keys: Vec<&str> = items.iter().map(|i| i.key.as_str()).collect();
        assert_eq!(keys, vec!["class:kitty"]);
    }

    #[test]
    fn clicks_pick_the_window_in_front() {
        let mut d = desktop();
        let (items, _) = compose(&[], &d, &index(), Running::All, &d.monitors[0], &[]);
        let firefox = items.iter().find(|i| i.key == "firefox").unwrap();
        assert_eq!(pick_window(&d, firefox, &d.monitors[0]).unwrap().address, "a");
        d.monitors[0].active_workspace = 2;
        assert_eq!(pick_window(&d, firefox, &d.monitors[0]).unwrap().address, "c", "the one on this workspace");
        assert_eq!(next_window(&d, firefox).unwrap().address, "c", "cycling from the focused one");
    }
}
