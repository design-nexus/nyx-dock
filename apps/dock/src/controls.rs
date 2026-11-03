//! Hover buttons for floating windows (`window_controls = true`, Settings → Tiling &
//! Workspaces → Window controls → Hover buttons). Omarchy's titlebar is off then, so
//! the focused floating window gets three small dots at its top-right corner that
//! open into minimize, maximize and close when the pointer reaches them.
//!
//! Windows tagged `nyx-own-controls` draw their own (Nyx apps, and the apps Settings
//! lists as having their own titlebar) and get none. A floating window can be dragged
//! or resized without Hyprland saying so, so while the dots show, the focused window is
//! read again every [`FOLLOW`]; nothing runs while they don't.

use gtk::prelude::*;
use gtk::{gdk, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use nyx_desktop::hypr::Instance;
use nyx_desktop::{Desktop, actions};
use serde::Deserialize;
use std::cell::RefCell;
use std::rc::Rc;
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

/// Where the dots go: a monitor and margins from its top-right corner.
#[derive(Clone, PartialEq)]
struct Place {
    address: String,
    connector: String,
    top: i32,
    right: i32,
}

struct Pill {
    window: gtk::Window,
    root: gtk::Box,
    buttons: Vec<(gtk::Button, &'static str)>,
    place: RefCell<Option<Place>>,
    follow: RefCell<Option<glib::SourceId>>,
    enabled: std::cell::Cell<bool>,
    only: Option<String>,
    desktop: RefCell<std::sync::Arc<Desktop>>,
}

thread_local! {
    static PILL: RefCell<Option<Rc<Pill>>> = const { RefCell::new(None) };
}

/// Called with every desktop update and when the dock's settings change.
pub fn update(app: &gtk::Application, enabled: bool, only: Option<&str>, desktop: &std::sync::Arc<Desktop>) {
    let pill = PILL.with(|p| p.borrow().clone());
    let pill = match pill {
        Some(p) => p,
        None if !enabled => return,
        None => {
            let p = Pill::new(app, only.map(str::to_string));
            PILL.with(|slot| *slot.borrow_mut() = Some(p.clone()));
            p
        }
    };
    pill.enabled.set(enabled);
    *pill.desktop.borrow_mut() = desktop.clone();
    pill.refresh();
}

impl Pill {
    fn new(app: &gtk::Application, only: Option<String>) -> Rc<Pill> {
        let window = gtk::Window::new();
        window.set_application(Some(app));
        window.add_css_class("window-pill-surface");
        window.init_layer_shell();
        window.set_namespace(Some("nyx-window-controls"));
        window.set_layer(Layer::Top);
        window.set_keyboard_mode(KeyboardMode::None);
        window.set_exclusive_zone(-1);
        window.set_anchor(Edge::Top, true);
        window.set_anchor(Edge::Right, true);

        let root = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        root.add_css_class("window-pill");
        root.set_halign(gtk::Align::End);
        root.set_valign(gtk::Align::Start);
        let mut buttons = Vec::new();
        for (icon, tip) in [
            ("window-minimize-symbolic", "Minimize"),
            ("window-maximize-symbolic", "Maximize"),
            ("window-close-symbolic", "Close"),
        ] {
            let b = gtk::Button::new();
            b.add_css_class("window-pill-button");
            b.set_tooltip_text(Some(tip));
            b.set_focus_on_click(false);
            b.set_valign(gtk::Align::Center);
            root.append(&b);
            buttons.push((b, icon));
        }
        window.set_child(Some(&root));

        let pill = Rc::new(Pill {
            window,
            root,
            buttons,
            place: RefCell::default(),
            follow: RefCell::default(),
            enabled: std::cell::Cell::new(false),
            only,
            desktop: RefCell::default(),
        });

        let hover = gtk::EventControllerMotion::new();
        let wp = Rc::downgrade(&pill);
        hover.connect_enter(move |_, _, _| {
            if let Some(p) = wp.upgrade() {
                p.open(true);
                // It may have moved since the last look.
                p.refresh();
            }
        });
        let wp = Rc::downgrade(&pill);
        hover.connect_leave(move |_| {
            if let Some(p) = wp.upgrade() {
                p.open(false);
            }
        });
        pill.root.add_controller(hover);

        for (i, (b, _)) in pill.buttons.iter().enumerate() {
            let wp = Rc::downgrade(&pill);
            b.connect_clicked(move |_| {
                let Some(p) = wp.upgrade() else { return };
                let Some(address) = p.place.borrow().as_ref().map(|pl| pl.address.clone()) else { return };
                std::thread::spawn(move || match i {
                    0 => {
                        actions::minimize(&address);
                    }
                    1 => {
                        actions::toggle_maximized(&address).run();
                    }
                    _ => {
                        actions::close(&address);
                    }
                });
            });
        }
        pill.open(false);
        pill
    }

    /// Dots, or the buttons themselves.
    fn open(&self, open: bool) {
        if open {
            self.root.add_css_class("open");
        } else {
            self.root.remove_css_class("open");
        }
        for (b, icon) in &self.buttons {
            if open {
                b.set_icon_name(icon);
            } else {
                b.set_child(None::<&gtk::Widget>);
            }
        }
        // A window only grows to fit its content; asking for the smallest size lets
        // the surface shrink back to the dots.
        if !open {
            self.window.set_default_size(1, 1);
        }
    }

    /// Where the dots belong now, if anywhere.
    fn wanted(&self) -> Option<Place> {
        if !self.enabled.get() {
            return None;
        }
        let desktop = self.desktop.borrow().clone();
        if !desktop.connected {
            return None;
        }
        let hypr = Instance::find().ok()?;
        let a: Active = match self.only.as_deref() {
            // On a test output (`--only`), the window in front there: focus stays
            // with the user's own screens.
            Some(only) => {
                let m = desktop.monitors.iter().find(|m| m.name == only)?;
                let clients: Vec<Active> = hypr.json("clients").ok()?;
                clients
                    .into_iter()
                    .filter(|c| c.monitor == m.id && c.workspace.id == m.active_workspace)
                    .min_by_key(|c| c.focus_history_id)?
            }
            None => hypr.json("activewindow").ok()?,
        };
        let own = a.tags.iter().any(|t| t.trim_end_matches('*') == OWN_CONTROLS_TAG);
        if a.address.is_empty() || !a.floating || a.fullscreen & 2 != 0 || a.hidden || own || a.workspace.id < 1 {
            return None;
        }
        let m = desktop.monitors.iter().find(|m| m.id == a.monitor)?;
        if self.only.as_deref().is_some_and(|o| o != m.name) {
            return None;
        }
        let (width, _) = m.logical_size();
        Some(Place {
            address: nyx_desktop::event::address(&a.address),
            connector: m.name.clone(),
            top: (a.at[1] - m.y + INSET).max(0),
            right: (m.x + width - (a.at[0] + a.size[0]) + INSET).max(0),
        })
    }

    /// Put the dots where they belong; whether they show.
    fn reposition(&self) -> bool {
        let want = self.wanted();
        if *self.place.borrow() != want {
            self.show(want.as_ref());
            *self.place.borrow_mut() = want;
        }
        self.place.borrow().is_some()
    }

    fn refresh(self: &Rc<Self>) {
        let shown = self.reposition();
        let following = self.follow.borrow().is_some();
        if shown && !following {
            let wp = Rc::downgrade(self);
            let id = glib::timeout_add_local(FOLLOW, move || {
                let Some(p) = wp.upgrade() else { return glib::ControlFlow::Break };
                if p.reposition() {
                    glib::ControlFlow::Continue
                } else {
                    p.follow.borrow_mut().take();
                    glib::ControlFlow::Break
                }
            });
            *self.follow.borrow_mut() = Some(id);
        } else if !shown && following && let Some(id) = self.follow.borrow_mut().take() {
            id.remove();
        }
    }

    fn show(&self, place: Option<&Place>) {
        let Some(place) = place else {
            self.window.set_visible(false);
            return;
        };
        let moved_monitor = self.place.borrow().as_ref().is_none_or(|p| p.connector != place.connector);
        if moved_monitor {
            self.window.set_visible(false);
            self.window.set_monitor(gdk_monitor(&place.connector).as_ref());
        }
        self.window.set_margin(Edge::Top, place.top);
        self.window.set_margin(Edge::Right, place.right);
        self.window.set_visible(true);
    }
}

fn gdk_monitor(connector: &str) -> Option<gdk::Monitor> {
    let monitors = gdk::Display::default()?.monitors();
    (0..monitors.n_items())
        .filter_map(|i| monitors.item(i).and_downcast::<gdk::Monitor>())
        .find(|m| m.connector().is_some_and(|c| c == connector))
}
