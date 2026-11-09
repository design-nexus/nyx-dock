//! Hover buttons for floating windows (`window_controls = true`), as the GTK dock's
//! `controls.rs`: where the dots go is published as topic `controls`
//! (`{address, monitor, top, right}`, or null), and the Quickshell interface draws them.
//!
//! Windows tagged `nyx-own-controls` draw their own and get none. A floating window can
//! be dragged or resized without Hyprland saying so, so while the dots show, the focused
//! window is read again every [`FOLLOW`]; nothing runs while they don't.

use gio::glib;
use nyx_desktop::Desktop;
use nyx_desktop::hypr::Instance;
use serde::Deserialize;
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

const OWN_CONTROLS_TAG: &str = "nyx-own-controls";
const FOLLOW: Duration = Duration::from_millis(150);
/// From the window's top and right edges to the dots.
const INSET: i32 = 8;

#[derive(Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct Active {
    address: String,
    at: [i32; 2],
    size: [i32; 2],
    floating: bool,
    fullscreen: u8,
    hidden: bool,
    monitor: i64,
    workspace: WorkspaceRef,
    tags: Vec<String>,
    #[serde(rename = "focusHistoryID")]
    focus_history_id: i32,
}

#[derive(Deserialize, Default, Clone, PartialEq)]
#[serde(default)]
struct WorkspaceRef {
    id: i64,
}

/// A monitor as Hyprland has it now: outputs can move without the desktop model
/// hearing of it.
#[derive(Deserialize, Default, Clone)]
#[serde(default)]
struct Output {
    id: i64,
    name: String,
    x: i32,
    y: i32,
    width: i32,
    height: i32,
    scale: f64,
    transform: i32,
}

impl Output {
    fn logical_width(&self) -> i32 {
        let w = if self.transform % 2 == 1 { self.height } else { self.width };
        (w as f64 / if self.scale > 0.0 { self.scale } else { 1.0 }).round() as i32
    }
}

#[derive(Clone, PartialEq)]
struct Place {
    address: String,
    connector: String,
    top: i32,
    right: i32,
}

struct Pill {
    link: nyx_ui_link::Link,
    place: RefCell<Option<Place>>,
    follow: RefCell<Option<glib::SourceId>>,
    enabled: Cell<bool>,
    querying: Cell<bool>,
    again: Cell<bool>,
    only: Option<String>,
    desktop: RefCell<Arc<Desktop>>,
}

thread_local! {
    static PILL: RefCell<Option<Rc<Pill>>> = const { RefCell::new(None) };
}

/// Called with every desktop update and when the dock's settings change.
pub fn update(link: &nyx_ui_link::Link, enabled: bool, only: Option<&str>, desktop: &Arc<Desktop>) {
    let pill = PILL.with(|p| p.borrow().clone());
    let pill = match pill {
        Some(p) => p,
        None if !enabled => return,
        None => {
            let p = Rc::new(Pill {
                link: link.clone(),
                place: RefCell::default(),
                follow: RefCell::default(),
                enabled: Cell::new(false),
                querying: Cell::new(false),
                again: Cell::new(false),
                only: only.map(str::to_string),
                desktop: RefCell::default(),
            });
            p.link.publish("controls", Value::Null);
            PILL.with(|slot| *slot.borrow_mut() = Some(p.clone()));
            p
        }
    };
    pill.enabled.set(enabled);
    *pill.desktop.borrow_mut() = desktop.clone();
    pill.refresh();
}

/// Look again now (the pointer reached the dots; the window may have moved).
pub fn refresh() {
    if let Some(p) = PILL.with(|p| p.borrow().clone()) {
        p.refresh();
    }
}

/// `minimize`, `maximize` or `close` the window the dots are on.
pub fn act(action: &str) -> Result<(), String> {
    let address = PILL
        .with(|p| p.borrow().as_ref().and_then(|p| p.place.borrow().as_ref().map(|pl| pl.address.clone())))
        .ok_or("no window")?;
    let action = action.to_string();
    std::thread::spawn(move || match action.as_str() {
        "minimize" => {
            nyx_desktop::actions::minimize(&address);
        }
        "maximize" => {
            nyx_desktop::actions::toggle_maximized(&address).run();
        }
        "close" => {
            nyx_desktop::actions::close(&address);
        }
        _ => {}
    });
    Ok(())
}

impl Pill {
    /// The window the dots are for: the focused one, or on a test output the window in
    /// front there, as focus stays with the user's own screens. Blocks.
    fn fetch(only: Option<(i64, i64)>) -> Option<(Active, Vec<Output>)> {
        let hypr = Instance::find().ok()?;
        let outputs: Vec<Output> = hypr.json("monitors").ok()?;
        let active = match only {
            Some((monitor, workspace)) => {
                let clients: Vec<Active> = hypr.json("clients").ok()?;
                clients
                    .into_iter()
                    .filter(|c| c.monitor == monitor && c.workspace.id == workspace)
                    .min_by_key(|c| c.focus_history_id)
            }
            None => hypr.json("activewindow").ok(),
        };
        Some((active?, outputs))
    }

    fn place_for(&self, found: Option<(Active, Vec<Output>)>) -> Option<Place> {
        let (a, outputs) = found?;
        let own = a.tags.iter().any(|t| t.trim_end_matches('*') == OWN_CONTROLS_TAG);
        if a.address.is_empty() || !a.floating || a.fullscreen & 2 != 0 || a.hidden || own || a.workspace.id < 1 {
            return None;
        }
        let m = outputs.iter().find(|m| m.id == a.monitor)?;
        if self.only.as_deref().is_some_and(|o| o != m.name) {
            return None;
        }
        let width = m.logical_width();
        Some(Place {
            address: nyx_desktop::event::address(&a.address),
            connector: m.name.clone(),
            top: (a.at[1] - m.y + INSET).max(0),
            right: (m.x + width - (a.at[0] + a.size[0]) + INSET).max(0),
        })
    }

    fn refresh(self: &Rc<Self>) {
        let desktop = self.desktop.borrow().clone();
        if !self.enabled.get() || !desktop.connected {
            self.apply(None);
            return;
        }
        let only = match self.only.as_deref() {
            Some(only) => match desktop.monitors.iter().find(|m| m.name == only) {
                Some(m) => Some((m.id, m.active_workspace)),
                None => {
                    self.apply(None);
                    return;
                }
            },
            None => None,
        };
        if self.querying.replace(true) {
            self.again.set(true);
            return;
        }
        let wp = Rc::downgrade(self);
        glib::MainContext::default().spawn_local(async move {
            let active = gio::spawn_blocking(move || Pill::fetch(only)).await.ok().flatten();
            let Some(p) = wp.upgrade() else { return };
            p.querying.set(false);
            if p.again.replace(false) {
                p.refresh();
                return;
            }
            let want = p.place_for(active);
            p.apply(want);
        });
    }

    fn apply(self: &Rc<Self>, want: Option<Place>) {
        if *self.place.borrow() != want {
            self.link.publish(
                "controls",
                want.as_ref().map_or(Value::Null, |p| {
                    json!({"address": p.address, "monitor": p.connector, "top": p.top, "right": p.right})
                }),
            );
            *self.place.borrow_mut() = want;
        }
        let shown = self.place.borrow().is_some();
        let following = self.follow.borrow().is_some();
        if shown && !following {
            let wp = Rc::downgrade(self);
            let id = glib::timeout_add_local(FOLLOW, move || {
                let Some(p) = wp.upgrade() else { return glib::ControlFlow::Break };
                p.refresh();
                glib::ControlFlow::Continue
            });
            *self.follow.borrow_mut() = Some(id);
        } else if !shown && let Some(id) = self.follow.borrow_mut().take() {
            id.remove();
        }
    }
}
