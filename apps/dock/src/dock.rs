//! The dock windows: one layer-shell surface per monitor, plus a thin edge strip that
//! brings a hidden dock back.

use crate::badges::{self, Badge};
use crate::launch;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use nyx_desktop::{Desktop, Monitor, watch};
use nyx_desktop::apps::Index;
use nyx_dock_core::config::{Config, Effective, Hide, Position, first_pins};
use nyx_dock_core::hide::{Rect, clear_of_dock, mode_here, should_hide};
use nyx_dock_core::items::{self, Item, MINIMIZED};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// `NYX_DOCK_DEBUG=1` prints what the dock decides.
pub fn debug(msg: impl FnOnce() -> String) {
    thread_local!(static ON: bool = std::env::var_os("NYX_DOCK_DEBUG").is_some());
    if ON.with(|on| *on) {
        eprintln!("nyx-dock: {}", msg());
    }
}

/// A screenshot run (`NYX_DOCK_TEST_*`): popovers open without taking the keyboard or
/// a click to close.
pub fn testing() -> bool {
    thread_local!(static ON: bool = std::env::vars_os().any(|(k, _)| k.to_string_lossy().starts_with("NYX_DOCK_TEST_")));
    ON.with(|on| *on)
}

struct Shared {
    app: gtk::Application,
    only: Option<String>,
    config: RefCell<Config>,
    desktop: RefCell<Arc<Desktop>>,
    index: RefCell<Index>,
    /// Pins that stand for another app (Omarchy's Browser entry → the default browser):
    /// pinned id → the app shown.
    aliases: RefCell<HashMap<String, String>>,
    badges: RefCell<HashMap<String, Badge>>,
    docks: RefCell<Vec<Rc<Dock>>>,
    /// Running apps' order per output, so they keep their places.
    orders: RefCell<HashMap<String, Vec<String>>>,
    /// Ignore our own config writes when the file monitor reports them.
    saving: Cell<bool>,
}

struct Dock {
    connector: String,
    gmon: gdk::Monitor,
    /// The monitor's settings. They change in place; a new edge (`position`) makes a
    /// new dock.
    eff: RefCell<Effective>,
    position: Position,
    window: gtk::Window,
    bar: gtk::Box,
    edge: gtk::Window,
    items: RefCell<Vec<Item>>,
    tiles: RefCell<Vec<Tile>>,
    preview: RefCell<Option<Preview>>,
    preview_timer: RefCell<Option<glib::SourceId>>,
    preview_close: RefCell<Option<glib::SourceId>>,
    hidden: Cell<bool>,
    hover: Cell<bool>,
    /// Menus open from this dock; it stays up while any is.
    holds: Cell<u32>,
    timer: RefCell<Option<glib::SourceId>>,
    /// Moving floating windows off a dock that stays up, once they settle.
    fit_timer: RefCell<Option<glib::SourceId>>,
    /// The surface's size when last shown, for working out where it sits.
    size: Cell<(i32, i32)>,
    /// An app being dragged, from the dock or the Apps panel.
    drag: RefCell<Option<Drag>>,
    /// A file drag over the bar keeps the dock up.
    file_drag: Cell<bool>,
}

impl Dock {
    fn eff(&self) -> std::cell::Ref<'_, Effective> {
        self.eff.borrow()
    }

    fn horizontal(&self) -> bool {
        matches!(self.position, Position::Bottom | Position::Top)
    }
}

/// An app on its way to a new place among the pins. While the pointer is over the dock,
/// `tile` (its own tile) or `gap` (a placeholder for an app from the Apps panel) sits
/// where it would land, and the other icons make room.
struct Drag {
    key: String,
    tile: Option<gtk::Widget>,
    /// The tile's neighbour before the drag, to put it back.
    home: Option<gtk::Widget>,
    gap: Option<gtk::Widget>,
    dropped: bool,
}

pub fn start(app: &gtk::Application, only: Option<String>) {
    std::mem::forget(app.hold());
    debug(|| "start".into());
    let config = load_config();
    let s = Rc::new(Shared {
        app: app.clone(),
        only,
        config: RefCell::new(config),
        desktop: RefCell::new(Arc::new(Desktop::default())),
        index: RefCell::new(build_index()),
        aliases: RefCell::default(),
        badges: RefCell::default(),
        docks: RefCell::default(),
        orders: RefCell::default(),
        saving: Cell::new(false),
    });
    // Everything below holds weak references; the dock lives as long as the process.
    std::mem::forget(s.clone());

    let ws = Rc::downgrade(&s);
    std::mem::forget(watch::watch_local(move |u| {
        let Some(s) = ws.upgrade() else { return };
        debug(|| format!("update {:?} connected {}", u.changes, u.desktop.connected));
        *s.desktop.borrow_mut() = u.desktop.clone();
        update_controls(&s);
        if u.changes.monitors || s.docks.borrow().is_empty() {
            rebuild_docks(&s, false);
        } else if u.changes.windows || u.changes.focus || u.changes.workspaces || u.changes.active {
            refresh(&s);
            // Not on focus alone: a click on a window being dragged is one.
            if u.changes.windows || u.changes.workspaces || u.changes.active {
                fit_all(&s);
            }
        }
    }));

    // Apps installed or removed: match windows again.
    let ws = Rc::downgrade(&s);
    let monitor = gio::AppInfoMonitor::get();
    monitor.connect_changed(move |_| {
        if let Some(s) = ws.upgrade() {
            *s.index.borrow_mut() = build_index();
            s.aliases.borrow_mut().clear();
            refresh_all(&s);
        }
    });
    std::mem::forget(monitor);

    // The settings file changed (Nyx Settings, or by hand).
    let path = Config::path();
    if let Ok(fm) = gio::File::for_path(&path).monitor_file(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
        let ws = Rc::downgrade(&s);
        fm.connect_changed(move |_, _, _, event| {
            let Some(s) = ws.upgrade() else { return };
            if s.saving.get() || !matches!(event, gio::FileMonitorEvent::ChangesDoneHint | gio::FileMonitorEvent::Created) {
                return;
            }
            *s.config.borrow_mut() = load_config();
            update_controls(&s);
            rebuild_docks(&s, true);
        });
        std::mem::forget(fm);
    }

    // Screens plugged in or out.
    if let Some(display) = gdk::Display::default() {
        let ws = Rc::downgrade(&s);
        display.monitors().connect_items_changed(move |_, _, _, _| {
            let ws = ws.clone();
            glib::timeout_add_local_once(Duration::from_millis(300), move || {
                if let Some(s) = ws.upgrade() {
                    rebuild_docks(&s, false);
                }
            });
        });
    }

    let ws = Rc::downgrade(&s);
    if let Some(sub) = badges::watch(move |id, badge| {
        let Some(s) = ws.upgrade() else { return };
        s.badges.borrow_mut().insert(id.to_string(), badge.clone());
        refresh_all(&s);
    }) {
        std::mem::forget(sub);
    }
}

fn update_controls(s: &Shared) {
    let enabled = s.config.borrow().window_controls;
    crate::controls::update(&s.app, enabled, s.only.as_deref(), &s.desktop.borrow());
}

// ---------- Settings and apps ----------

fn load_config() -> Config {
    let path = Config::path();
    match Config::load(&path) {
        Some(Ok(c)) => c,
        Some(Err(e)) => {
            eprintln!("nyx-dock: {e}; using the defaults");
            Config::default()
        }
        None => {
            // First start: pins from the user's Omarchy dock, else the Nyx apps.
            let browser = gio::AppInfo::default_for_uri_scheme("https").and_then(|a| a.id()).map(|s| s.to_string());
            let config = Config {
                pinned: first_pins(omarchy_pins(), browser.as_deref(), |id| launch::info(id).is_some()),
                ..Default::default()
            };
            let _ = write_config(&config);
            config
        }
    }
}

/// The pins in `~/.config/omarchy/shell.json`, if the user set their own.
fn omarchy_pins() -> Option<Vec<String>> {
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| glib::home_dir().join(".config"));
    let text = std::fs::read_to_string(config.join("omarchy/shell.json")).ok()?;
    let v: serde_json::Value = serde_json::from_str(&text).ok()?;
    let pins = v.get("dock")?.get("pinned")?.as_array()?;
    Some(pins.iter().filter_map(|p| p.as_str()).map(|p| p.trim_end_matches(".desktop").to_string()).collect())
}

fn write_config(config: &Config) -> std::io::Result<()> {
    let path = Config::path();
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, config.to_toml())?;
    std::fs::rename(tmp, path)
}

fn save(s: &Rc<Shared>) {
    s.saving.set(true);
    if let Err(e) = write_config(&s.config.borrow()) {
        eprintln!("nyx-dock: couldn't save the settings: {e}");
    }
    let ws = Rc::downgrade(s);
    glib::timeout_add_local_once(Duration::from_millis(500), move || {
        if let Some(s) = ws.upgrade() {
            s.saving.set(false);
        }
    });
}

/// A thumbnail as a GDK texture.
fn shot_texture(shot: nyx_desktop::capture::Shot) -> gdk::Texture {
    let format = if shot.opaque { gdk::MemoryFormat::B8g8r8x8 } else { gdk::MemoryFormat::B8g8r8a8Premultiplied };
    let stride = shot.stride();
    gdk::MemoryTexture::new(shot.width as i32, shot.height as i32, format, &glib::Bytes::from_owned(shot.data), stride)
        .upcast()
}

fn build_index() -> Index {
    nyx_desktop::apps::from_gio()
}

// ---------- Docks ----------

/// Match the docks to the monitors and settings. A dock whose monitor and edge are
/// unchanged is kept (outputs come and go, e.g. headless ones for tests, and a new
/// window would show up before it could hide); its settings change in place. `refill`
/// redraws every dock's icons (the settings changed).
fn rebuild_docks(s: &Rc<Shared>, refill: bool) {
    let desktop = s.desktop.borrow().clone();
    let Some(display) = gdk::Display::default() else { return };
    let gmons: Vec<gdk::Monitor> = display.monitors().iter::<gdk::Monitor>().flatten().collect();
    let config = s.config.borrow().clone();
    let old: Vec<Rc<Dock>> = s.docks.borrow_mut().drain(..).collect();
    let mut docks: Vec<Rc<Dock>> = Vec::new();
    debug(|| {
        let names: Vec<String> = gmons.iter().map(|g| g.connector().map(|c| c.to_string()).unwrap_or_default()).collect();
        format!(
            "rebuild: hyprland monitors {:?}, gdk monitors {names:?}",
            desktop.monitors.iter().map(|m| &m.name).collect::<Vec<_>>()
        )
    });
    for m in desktop.monitors.iter().filter(|m| !m.disabled) {
        if s.only.as_ref().is_some_and(|o| *o != m.name) {
            continue;
        }
        let mut eff = config.for_monitor(&m.stable_id(), &m.name, m.is_virtual());
        // `--only` is for trying the dock on a test output, which is virtual.
        if s.only.is_some() {
            eff.enabled = true;
        }
        if !eff.enabled {
            continue;
        }
        let Some(gmon) = gmons.iter().find(|g| g.connector().as_deref() == Some(m.name.as_str())) else { continue };
        let prev = old.iter().find(|d| d.connector == m.name && d.position == eff.position);
        if let Some(d) = prev.filter(|d| d.gmon == *gmon) {
            if *d.eff() != eff {
                debug(|| format!("settings on {} ({:?})", m.name, eff));
                *d.eff.borrow_mut() = eff;
            }
            docks.push(d.clone());
            continue;
        }
        debug(|| format!("dock on {} ({:?})", m.name, eff));
        // A dock made again stays out of sight if the one before it was.
        let was = prev.map(|d| (d.hidden.get(), d.size.get()));
        docks.push(new_dock(s, &m.name, gmon, eff, was));
    }
    for d in old.iter().filter(|d| !docks.iter().any(|k| Rc::ptr_eq(k, d))) {
        for timer in [&d.timer, &d.fit_timer] {
            if let Some(t) = timer.borrow_mut().take() {
                t.remove();
            }
        }
        d.window.destroy();
        d.edge.destroy();
    }
    *s.docks.borrow_mut() = docks;
    if refill { refresh_all(s) } else { refresh(s) }
    fit_all(s);
}

fn fit_all(s: &Rc<Shared>) {
    for d in s.docks.borrow().clone() {
        schedule_fit(s, &d);
    }
}

fn edges(position: Position) -> (Edge, gtk::Orientation) {
    match position {
        Position::Bottom => (Edge::Bottom, gtk::Orientation::Horizontal),
        Position::Top => (Edge::Top, gtk::Orientation::Horizontal),
        Position::Left => (Edge::Left, gtk::Orientation::Vertical),
        Position::Right => (Edge::Right, gtk::Orientation::Vertical),
    }
}

/// `was`: whether the dock this one replaces was hidden, and its size.
fn new_dock(s: &Rc<Shared>, connector: &str, gmon: &gdk::Monitor, eff: Effective, was: Option<(bool, (i32, i32))>) -> Rc<Dock> {
    let (edge, orientation) = edges(eff.position);
    let window = gtk::Window::new();
    window.set_application(Some(&s.app));
    window.add_css_class("dock-surface");
    window.add_css_class(&format!("dock-{}", format!("{:?}", eff.position).to_lowercase()));
    window.init_layer_shell();
    window.set_namespace(Some("nyx-dock"));
    window.set_layer(Layer::Top);
    window.set_monitor(Some(gmon));
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_exclusive_zone(0);
    window.set_anchor(edge, true);

    let bar = gtk::Box::new(orientation, 6);
    bar.add_css_class("dock-bar");
    window.set_child(Some(&bar));
    let horizontal = orientation == gtk::Orientation::Horizontal;

    // The strip at the screen edge that brings a hidden dock back.
    let strip = gtk::Window::new();
    strip.set_application(Some(&s.app));
    strip.add_css_class("dock-edge");
    strip.init_layer_shell();
    strip.set_namespace(Some("nyx-dock-edge"));
    strip.set_layer(Layer::Top);
    strip.set_monitor(Some(gmon));
    strip.set_keyboard_mode(KeyboardMode::None);
    strip.set_exclusive_zone(0);
    strip.set_anchor(edge, true);
    let strip_child = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    strip.set_child(Some(&strip_child));
    // Sized before it's shown: an empty window gets GTK's 200 × 200 fallback, which
    // later size requests don't shrink. Until the dock has a size, half the edge.
    let half = if horizontal { gmon.geometry().width() / 2 } else { gmon.geometry().height() / 2 };
    let (was_hidden, was_size) = was.unwrap_or((false, (0, 0)));
    let was_along = if horizontal { was_size.0 } else { was_size.1 };
    size_edge(&strip, horizontal, if was_along >= 96 { was_along } else { half.max(48) });

    let start_hidden = eff.hide == Hide::Always || (was_hidden && eff.hide != Hide::Never);
    let dock = Rc::new(Dock {
        connector: connector.to_string(),
        gmon: gmon.clone(),
        position: eff.position,
        eff: RefCell::new(eff),
        window,
        bar,
        edge: strip,
        items: RefCell::default(),
        tiles: RefCell::default(),
        preview: RefCell::default(),
        preview_timer: RefCell::default(),
        preview_close: RefCell::default(),
        hidden: Cell::new(start_hidden),
        hover: Cell::new(false),
        holds: Cell::new(0),
        timer: RefCell::default(),
        fit_timer: RefCell::default(),
        size: Cell::new(was_size),
        drag: RefCell::default(),
        file_drag: Cell::new(false),
    });

    let motion = gtk::EventControllerMotion::new();
    let (wd, ws) = (Rc::downgrade(&dock), Rc::downgrade(s));
    motion.connect_enter(move |_, _, _| {
        if let Some(d) = wd.upgrade() {
            d.hover.set(true);
            cancel_timer(&d);
        }
    });
    bar_drops(s, &dock);
    bar_menu(s, &dock);

    let wd = Rc::downgrade(&dock);
    motion.connect_leave(move |_| {
        if let (Some(d), Some(s)) = (wd.upgrade(), ws.upgrade()) {
            d.hover.set(false);
            evaluate(&s, &d);
        }
    });
    dock.window.add_controller(motion);

    let reveal = gtk::EventControllerMotion::new();
    let (wd, ws) = (Rc::downgrade(&dock), Rc::downgrade(s));
    reveal.connect_enter(move |_, _, _| {
        if let (Some(d), Some(s)) = (wd.upgrade(), ws.upgrade()) {
            show(&d);
            // Give the pointer time to reach the dock before it may hide again.
            schedule_hide(&s, &d, (s.config.borrow().hide_delay_ms as u64).max(1200));
        }
    });
    dock.edge.add_controller(reveal);

    let (wd, ws) = (Rc::downgrade(&dock), Rc::downgrade(s));
    dock.window.connect_map(move |_| {
        let (wd, ws) = (wd.clone(), ws.clone());
        glib::timeout_add_local_once(Duration::from_millis(50), move || {
            if let (Some(d), Some(s)) = (wd.upgrade(), ws.upgrade()) {
                fit_input(&d);
                evaluate(&s, &d);
            }
        });
    });

    if !start_hidden {
        dock.window.present();
    }
    // Even with Never: on tiling workspaces the dock hides as Smart does.
    dock.edge.present();
    dock
}

/// What the bar takes when something is dropped on it: an app moving to a new place
/// among the pins (the others make room as it moves), or folders from the file
/// manager, added among the folders.
fn bar_drops(s: &Rc<Shared>, dock: &Rc<Dock>) {
    let target = gtk::DropTarget::new(glib::Type::INVALID, gdk::DragAction::MOVE | gdk::DragAction::COPY);
    target.set_types(&[glib::BoxedAnyObject::static_type(), gdk::FileList::static_type()]);
    let along = |d: &Dock, x: f64, y: f64| if d.horizontal() { x } else { y };

    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(dock));
    let over = move |x: f64, y: f64| -> gdk::DragAction {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return gdk::DragAction::empty() };
        if d.drag.borrow().is_some() {
            drag_over(&d, along(&d, x, y));
            return gdk::DragAction::MOVE;
        }
        if !d.file_drag.replace(true) {
            d.bar.add_css_class("folder-drop");
            hold(&s, &d, true);
        }
        gdk::DragAction::COPY
    };
    let over = Rc::new(over);
    let o = over.clone();
    target.connect_enter(move |_, x, y| o(x, y));
    target.connect_motion(move |_, x, y| over(x, y));

    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(dock));
    target.connect_leave(move |_| {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        drag_left(&d);
        if d.file_drag.replace(false) {
            d.bar.remove_css_class("folder-drop");
            hold(&s, &d, false);
        }
    });

    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(dock));
    target.connect_drop(move |_, value, x, y| {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return false };
        if let Ok(obj) = value.get::<glib::BoxedAnyObject>() {
            let Ok(drag) = obj.try_borrow::<DockDrag>() else { return false };
            return drop_pin(&s, &d, &drag.0);
        }
        let Ok(list) = value.get::<gdk::FileList>() else { return false };
        let dirs: Vec<std::path::PathBuf> = list
            .files()
            .iter()
            .filter(|f| f.query_file_type(gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) == gio::FileType::Directory)
            .filter_map(|f| f.path())
            .collect();
        if dirs.is_empty() {
            return false;
        }
        let at = folder_index(&d, along(&d, x, y));
        add_folders(&s, &dirs, Some(at));
        true
    });
    dock.bar.add_controller(target);
}

/// Keep the dock up (a menu, a popover, a drag) or let it go.
fn hold(s: &Rc<Shared>, d: &Rc<Dock>, on: bool) {
    if on {
        close_preview(s, d);
        d.holds.set(d.holds.get() + 1);
        cancel_timer(d);
    } else {
        d.holds.set(d.holds.get().saturating_sub(1));
        evaluate(s, d);
    }
}

/// Where a folder dropped at `pos` (along the bar) goes among the folders.
fn folder_index(d: &Dock, pos: f64) -> usize {
    let mut index = 0;
    let mut child = d.bar.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if !c.has_css_class("dock-folder") {
            continue;
        }
        let Some(b) = c.compute_bounds(&d.bar) else { continue };
        let middle = if d.horizontal() { b.x() + b.width() / 2.0 } else { b.y() + b.height() / 2.0 };
        if (pos as f32) < middle {
            break;
        }
        index += 1;
    }
    index
}

fn add_folders(s: &Rc<Shared>, dirs: &[std::path::PathBuf], at: Option<usize>) {
    let home = glib::home_dir();
    let mut added = 0;
    for dir in dirs {
        if s.config.borrow_mut().add_folder(dir, &home, at.map(|a| a + added)) {
            added += 1;
        }
    }
    if added > 0 {
        save(s);
        let s2 = s.clone();
        glib::idle_add_local_once(move || rebuild_docks(&s2, true));
    }
}

/// Pick a folder to add to the dock.
fn choose_folder(s: &Rc<Shared>) {
    let dialog = gtk::FileDialog::builder().title("Add a Folder to the Dock").accept_label("Add").modal(false).build();
    dialog.set_initial_folder(Some(&gio::File::for_path(glib::home_dir())));
    let ws = Rc::downgrade(s);
    dialog.select_folder(None::<&gtk::Window>, gio::Cancellable::NONE, move |r| {
        let (Some(s), Ok(f)) = (ws.upgrade(), r) else { return };
        if let Some(path) = f.path() {
            add_folders(&s, &[path], None);
        }
    });
}

// ---------- Moving apps along the dock ----------

/// An app starts moving: from its tile, or from the Apps panel (no tile yet).
fn begin_drag(s: &Rc<Shared>, d: &Rc<Dock>, key: &str) {
    hold(s, d, true);
    let tile = d.tiles.borrow().iter().find(|t| t.key == key).map(|t| t.root.clone().upcast::<gtk::Widget>());
    let home = tile.as_ref().and_then(|t| t.prev_sibling());
    if let Some(t) = &tile {
        t.add_css_class("dragging");
    }
    *d.drag.borrow_mut() = Some(Drag { key: key.to_string(), tile, home, gap: None, dropped: false });
}

/// The pinned tiles in the bar's order, except the one moving.
fn pinned_tiles(d: &Dock, moving: &gtk::Widget) -> Vec<gtk::Widget> {
    let pinned: Vec<gtk::Widget> = d
        .tiles
        .borrow()
        .iter()
        .filter(|t| t.root.upcast_ref::<gtk::Widget>() != moving && item_for(d, &t.key).is_some_and(|i| i.pinned))
        .map(|t| t.root.clone().upcast())
        .collect();
    let mut out = Vec::with_capacity(pinned.len());
    let mut child = d.bar.first_child();
    while let Some(c) = child {
        if pinned.contains(&c) {
            out.push(c.clone());
        }
        child = c.next_sibling();
    }
    out
}

/// The Apps button, which stays first.
fn apps_button(d: &Dock) -> Option<gtk::Widget> {
    d.bar.first_child().filter(|c| c.has_css_class("dock-apps"))
}

/// The pointer moved over the dock with an app: open a gap where it would land.
fn drag_over(d: &Dock, pos: f64) {
    let mut drag = d.drag.borrow_mut();
    let Some(drag) = drag.as_mut() else { return };
    let moving = match drag.tile.clone().or_else(|| drag.gap.clone()) {
        Some(w) => w,
        None => {
            let gap = gtk::Box::new(gtk::Orientation::Vertical, 0);
            gap.add_css_class("dock-item");
            gap.add_css_class("dock-gap");
            let size = d.eff().icon_size as i32;
            gap.set_size_request(size, size + 6);
            d.bar.insert_child_after(&gap, apps_button(d).as_ref());
            drag.gap = Some(gap.clone().upcast());
            gap.upcast()
        }
    };
    moving.add_css_class("dragging");
    let others = pinned_tiles(d, &moving);
    // Compared with where the others are now: a pointer over the gap keeps it there.
    let slot = others
        .iter()
        .take_while(|w| {
            w.compute_bounds(&d.bar).is_none_or(|b| {
                let middle = if d.horizontal() { b.x() + b.width() / 2.0 } else { b.y() + b.height() / 2.0 };
                pos as f32 >= middle
            })
        })
        .count();
    let after = if slot == 0 { apps_button(d) } else { Some(others[slot - 1].clone()) };
    if moving.prev_sibling() != after {
        d.bar.reorder_child_after(&moving, after.as_ref());
    }
}

/// The pointer left the dock: the app goes back to where it was.
fn drag_left(d: &Dock) {
    let mut drag = d.drag.borrow_mut();
    let Some(drag) = drag.as_mut() else { return };
    if let Some(tile) = &drag.tile {
        if tile.prev_sibling() != drag.home {
            d.bar.reorder_child_after(tile, drag.home.as_ref());
        }
        tile.remove_css_class("dragging");
    }
    if let Some(gap) = drag.gap.take() {
        d.bar.remove(&gap);
    }
}

/// Dropped on the dock: pin the app where its gap is.
fn drop_pin(s: &Rc<Shared>, d: &Rc<Dock>, key: &str) -> bool {
    let next = {
        let mut drag = d.drag.borrow_mut();
        let Some(drag) = drag.as_mut().filter(|g| g.key == key) else { return false };
        drag.dropped = true;
        let Some(moving) = drag.tile.clone().or_else(|| drag.gap.clone()) else { return false };
        let others = pinned_tiles(d, &moving);
        let mut c = moving.next_sibling();
        let mut next = None;
        while let Some(w) = c {
            if others.contains(&w) {
                next = d.tiles.borrow().iter().find(|t| t.root.upcast_ref::<gtk::Widget>() == &w).map(|t| t.key.clone());
                break;
            }
            c = w.next_sibling();
        }
        next
    };
    let moving = pin_source(s, d, key);
    let list: Vec<String> = d.eff().pinned.iter().filter(|p| **p != moving).cloned().collect();
    let at = next.map(|n| pin_source(s, d, &n)).and_then(|n| list.iter().position(|p| *p == n)).unwrap_or(list.len());
    pin(s, d, key, true, Some(at));
    true
}

/// The drag is over, dropped or not.
fn end_drag(s: &Rc<Shared>, d: &Rc<Dock>) {
    let drag = d.drag.borrow_mut().take();
    if let Some(drag) = drag {
        if let Some(tile) = &drag.tile {
            tile.remove_css_class("dragging");
            if !drag.dropped && tile.prev_sibling() != drag.home && tile.parent().is_some() {
                d.bar.reorder_child_after(tile, drag.home.as_ref());
            }
        }
        if let Some(gap) = drag.gap.filter(|g| g.parent().is_some()) {
            d.bar.remove(&gap);
        }
    }
    hold(s, d, false);
    // Changes put off while it moved.
    let s2 = s.clone();
    glib::idle_add_local_once(move || refresh(&s2));
}

/// Only the bar takes clicks; the transparent margin around it (room for its shadow)
/// lets them through to the windows below.
fn fit_input(d: &Dock) {
    let (w, h) = (d.window.width(), d.window.height());
    if w > 0 && h > 0 {
        d.size.set((w, h));
    }
    let (Some(surface), Some(b)) = (d.window.surface(), d.bar.compute_bounds(&d.window)) else { return };
    let r = gtk::cairo::RectangleInt::new(b.x() as i32, b.y() as i32, b.width().ceil() as i32, b.height().ceil() as i32);
    surface.set_input_region(Some(&gtk::cairo::Region::create_rectangle(&r)));
    // The edge strip is as wide (or tall) as the dock, once the dock has been laid out
    // (a dock hidden before its first frame reports a few pixels).
    let horizontal = matches!(d.position, Position::Bottom | Position::Top);
    let along = if horizontal { r.width() } else { r.height() };
    if along >= 96 {
        size_edge(&d.edge, horizontal, along);
    }
}

/// The reveal strip: `along` pixels along the edge, 2 deep.
fn size_edge(strip: &gtk::Window, horizontal: bool, along: i32) {
    let (w, h) = if horizontal { (along, 2) } else { (2, along) };
    if let Some(child) = strip.child() {
        child.set_size_request(w, h);
    }
    strip.set_default_size(w, h);
    strip.set_size_request(w, h);
}

fn refresh_all(s: &Rc<Shared>) {
    for d in s.docks.borrow().iter() {
        d.items.borrow_mut().clear();
    }
    refresh(s);
}

fn refresh(s: &Rc<Shared>) {
    let desktop = s.desktop.borrow().clone();
    let docks: Vec<Rc<Dock>> = s.docks.borrow().clone();
    for d in docks {
        let Some(monitor) = desktop.monitor(&d.connector) else { continue };
        // Not under an app being moved; end_drag catches up.
        if d.drag.borrow().is_some() {
            continue;
        }
        let order = s.orders.borrow().get(&d.connector).cloned().unwrap_or_default();
        let pinned = shown_pins(s, &d.eff().pinned);
        let (items, order) = items::compose(&pinned, &desktop, &s.index.borrow(), d.eff().running, monitor, &order);
        s.orders.borrow_mut().insert(d.connector.clone(), order);
        debug(|| format!("{}: {} items", d.connector, items.len()));
        if *d.items.borrow() != items {
            let same_apps = d.items.borrow().iter().map(|i| &i.key).eq(items.iter().map(|i| &i.key));
            *d.items.borrow_mut() = items;
            if same_apps && !d.tiles.borrow().is_empty() {
                // Only state changed (focus, windows, badges): keep the widgets, so a
                // hover or an open preview isn't torn down.
                for t in d.tiles.borrow().iter() {
                    update_tile(s, &d, t);
                }
            } else {
                fill(s, &d);
            }
        }
        evaluate(s, &d);
    }
}

/// What folder and Trash tiles (and drop targets) need from a dock. `folder` is the
/// folder's place in the settings.
fn host(s: &Rc<Shared>, d: &Rc<Dock>, folder: Option<usize>) -> Rc<crate::stacks::Host> {
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    let hold = Box::new(move |on: bool| {
        if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
            self::hold(&s, &d, on);
        }
    });
    let ws = Rc::downgrade(s);
    let remove = Box::new(move || {
        let (Some(s), Some(i)) = (ws.upgrade(), folder) else { return };
        if i < s.config.borrow().folders.len() {
            s.config.borrow_mut().folders.remove(i);
            save(&s);
            let s2 = s.clone();
            glib::idle_add_local_once(move || rebuild_docks(&s2, true));
        }
    });
    // The tile redraws itself; other screens' docks catch up when they next refill.
    let ws = Rc::downgrade(s);
    let save_folder = Box::new(move |new: &nyx_dock_core::config::Folder| {
        let (Some(s), Some(i)) = (ws.upgrade(), folder) else { return };
        if let Some(f) = s.config.borrow_mut().folders.get_mut(i) {
            *f = new.clone();
        }
        save(&s);
    });
    Rc::new(crate::stacks::Host {
        hold,
        remove,
        save: save_folder,
        side: popover_side(d.position),
        icon_size: d.eff().icon_size as i32,
        max_height: max_height(d, 460),
    })
}

/// How tall a popover from this dock may grow: `cap`, or less on a short screen.
fn max_height(d: &Dock, cap: i32) -> i32 {
    (d.gmon.geometry().height() * 6 / 10).clamp(200, cap)
}

/// An app being dragged within the dock, or from the Apps panel onto it.
pub struct DockDrag(pub String);

/// The pins as shown: each alias pin as the app it stands for (once).
fn shown_pins(s: &Shared, pins: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(pins.len());
    for p in pins {
        let shown = {
            let mut aliases = s.aliases.borrow_mut();
            aliases.entry(p.clone()).or_insert_with(|| launch::stands_for(p).unwrap_or_else(|| p.clone())).clone()
        };
        if !out.contains(&shown) {
            out.push(shown);
        }
    }
    out
}

/// The pin an app is shown for: a shown app that stands in for a pin (the browser)
/// is that pin.
fn pin_source(s: &Shared, d: &Dock, key: &str) -> String {
    let source = s.aliases.borrow().iter().find(|(from, to)| *to == key && *from != key && d.eff().pinned.contains(from)).map(|(f, _)| f.clone());
    source.unwrap_or_else(|| key.to_string())
}

fn pin(s: &Rc<Shared>, d: &Rc<Dock>, key: &str, pinned: bool, at: Option<usize>) {
    let key = pin_source(s, d, key);
    let desktop = s.desktop.borrow().clone();
    let Some(m) = desktop.monitor(&d.connector) else { return };
    s.config.borrow_mut().set_pinned(&m.stable_id(), &m.name, &key, pinned, at);
    save(s);
    let s2 = s.clone();
    glib::idle_add_local_once(move || rebuild_docks(&s2, true));
}

/// Change a setting for this dock's monitor, and show it.
fn change(s: &Rc<Shared>, d: &Dock, f: impl FnOnce(&mut Config, &str, &str)) {
    let desktop = s.desktop.borrow().clone();
    let Some(m) = desktop.monitor(&d.connector) else { return };
    f(&mut s.config.borrow_mut(), &m.stable_id(), &m.name);
    save(s);
    let s2 = s.clone();
    glib::idle_add_local_once(move || rebuild_docks(&s2, true));
}

/// One app's widgets.
struct Tile {
    key: String,
    root: gtk::Box,
    overlay: gtk::Overlay,
    image: gtk::Image,
    dots: gtk::Box,
}

fn item_for(d: &Dock, key: &str) -> Option<Item> {
    d.items.borrow().iter().find(|i| i.key == key).cloned()
}

fn fill(s: &Rc<Shared>, d: &Rc<Dock>) {
    close_preview(s, d);
    // The dock's own menu hangs off the bar; it stays.
    let mut child = d.bar.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if !c.is::<gtk::Popover>() {
            d.bar.remove(&c);
        }
    }
    if s.config.borrow().apps {
        let apps = crate::apps::tile(apps_host(s, d));
        d.bar.prepend(&apps);
    }
    let items = d.items.borrow().clone();
    let mut tiles = Vec::new();
    for item in &items {
        if let Some(t) = new_tile(s, d, item) {
            d.bar.append(&t.root);
            tiles.push(t);
        }
    }
    *d.tiles.borrow_mut() = tiles;
    for t in d.tiles.borrow().iter() {
        update_tile(s, d, t);
    }
    let config = s.config.borrow().clone();
    if !config.folders.is_empty() || config.trash {
        let sep = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        sep.add_css_class("dock-sep");
        d.bar.append(&sep);
    }
    for (i, folder) in config.folders.iter().enumerate() {
        d.bar.append(&crate::stacks::folder_tile(folder, host(s, d, Some(i))));
    }
    if config.trash {
        d.bar.append(&crate::stacks::trash_tile(host(s, d, None)));
    }
    // For screenshots on a test output: open the first folder without a pointer.
    if std::env::var_os("NYX_DOCK_TEST_FOLDER").is_some() {
        let found = d.bar.last_child().and_then(|c| {
            let mut w = Some(c);
            while let Some(x) = w {
                if x.has_css_class("dock-folder") {
                    return Some(x);
                }
                w = x.prev_sibling();
            }
            None
        });
        if let Some(tile) = found {
            glib::timeout_add_local_once(Duration::from_millis(800), move || {
                // Simulate a click by activating the tile's gesture path.
                tile.activate_action("folder-test.open", None).ok();
            });
        }
    }
    // For screenshots on a test output: the Apps panel, or the dock's own menu.
    if std::env::var_os("NYX_DOCK_TEST_APPS").is_some()
        && let Some(apps) = apps_button(d)
    {
        glib::timeout_add_local_once(Duration::from_millis(800), move || {
            apps.activate_action("apps-test.open", None).ok();
        });
    }
    if std::env::var_os("NYX_DOCK_TEST_DOCK_MENU").is_some() {
        let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
        glib::timeout_add_local_once(Duration::from_millis(800), move || {
            if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
                let (x, y) = (d.bar.width() as f64 / 2.0, d.bar.height() as f64 / 2.0);
                dock_menu(&s, &d, x, y);
            }
        });
    }
    // For screenshots on a test output: open one app's menu without a pointer.
    if let Ok(key) = std::env::var("NYX_DOCK_TEST_MENU") {
        let found = d.tiles.borrow().iter().find(|t| t.key == key).map(|t| t.root.clone());
        if let (Some(tile), Some(it)) = (found, item_for(d, &key)) {
            let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
            glib::timeout_add_local_once(Duration::from_millis(800), move || {
                if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
                    menu(&s, &d, &tile, &it);
                }
            });
        }
    }
    // For screenshots on a test output: open one app's preview without a pointer.
    if let Ok(key) = std::env::var("NYX_DOCK_TEST_PREVIEW") {
        let tile = d.tiles.borrow().iter().find(|t| t.key == key).map(|t| t.root.clone());
        if let Some(tile) = tile {
            let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
            glib::timeout_add_local_once(Duration::from_millis(800), move || {
                if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
                    open_preview(&s, &d, &tile, &key);
                }
            });
        }
    }
    let wd = Rc::downgrade(d);
    glib::idle_add_local_once(move || {
        if let Some(d) = wd.upgrade() {
            fit_input(&d);
        }
    });
}

fn new_tile(s: &Rc<Shared>, d: &Rc<Dock>, item: &Item) -> Option<Tile> {
    let class = item.key.strip_prefix("class:");
    let info = if class.is_some() { None } else { launch::info(&item.key) };
    // A pinned app that's no longer installed has nothing to show or start.
    if class.is_none() && info.is_none() {
        return None;
    }
    let root = gtk::Box::new(gtk::Orientation::Vertical, 2);
    root.add_css_class("dock-item");
    let image = gtk::Image::new();
    image.set_pixel_size(d.eff().icon_size as i32);
    match info.as_ref().and_then(|i| i.icon()) {
        Some(icon) => image.set_from_gicon(&icon),
        None => image.set_icon_name(Some(&fallback_icon(class.unwrap_or("")))),
    }
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&image));
    root.append(&overlay);
    let dots = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    dots.add_css_class("dock-dots");
    dots.set_halign(gtk::Align::Center);
    root.append(&dots);

    let key = item.key.clone();
    let click = gtk::GestureClick::new();
    click.set_button(0);
    let (ws, wd, k) = (Rc::downgrade(s), Rc::downgrade(d), key.clone());
    let tile_weak = root.downgrade();
    click.connect_released(move |g, _, _, _| {
        let (Some(s), Some(d), Some(tile)) = (ws.upgrade(), wd.upgrade(), tile_weak.upgrade()) else { return };
        let Some(it) = item_for(&d, &k) else { return };
        close_preview(&s, &d);
        match g.current_button() {
            1 => activate(&s, &d, &it),
            2 if !it.key.starts_with("class:") => launch::new_instance(&it.key),
            3 => menu(&s, &d, &tile, &it),
            _ => {}
        }
    });
    root.add_controller(click);

    // Rest the pointer on a running app to see its windows.
    let motion = gtk::EventControllerMotion::new();
    let (ws, wd, k) = (Rc::downgrade(s), Rc::downgrade(d), key.clone());
    let tile_weak = root.downgrade();
    motion.connect_enter(move |_, _, _| {
        let (Some(s), Some(d), Some(tile)) = (ws.upgrade(), wd.upgrade(), tile_weak.upgrade()) else { return };
        hover_tile(&s, &d, &tile, &k);
    });
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    motion.connect_leave(move |_| {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        if let Some(t) = d.preview_timer.borrow_mut().take() {
            t.remove();
        }
        schedule_close_preview(&s, &d);
    });
    root.add_controller(motion);

    if !key.starts_with("class:") {
        // Drag an app along the dock to pin it there; drop it outside to unpin it.
        let drag = gtk::DragSource::new();
        drag.set_actions(gdk::DragAction::MOVE);
        let k = key.clone();
        drag.connect_prepare(move |_, _, _| {
            Some(gdk::ContentProvider::for_value(&glib::BoxedAnyObject::new(DockDrag(k.clone())).to_value()))
        });
        let (ws, wd, iw, k) = (Rc::downgrade(s), Rc::downgrade(d), image.downgrade(), key.clone());
        drag.connect_drag_begin(move |src, _| {
            // A still picture: the tile itself turns into the gap.
            if let Some(image) = iw.upgrade() {
                let paintable = gtk::WidgetPaintable::new(Some(&image)).current_image();
                src.set_icon(Some(&paintable), image.width() / 2, image.height() / 2);
            }
            if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
                begin_drag(&s, &d, &k);
            }
        });
        let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
        drag.connect_drag_end(move |_, _, _| {
            if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
                end_drag(&s, &d);
            }
        });
        let (ws, wd, k) = (Rc::downgrade(s), Rc::downgrade(d), key.clone());
        drag.connect_drag_cancel(move |_, _, reason| {
            let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return false };
            // Let go away from any drop target: off the dock.
            if reason == gdk::DragCancelReason::NoTarget && item_for(&d, &k).is_some_and(|i| i.pinned) {
                if let Some(drag) = d.drag.borrow_mut().as_mut() {
                    drag.dropped = true;
                }
                pin(&s, &d, &k, false, None);
                return true;
            }
            false
        });
        root.add_controller(drag);

        let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
        let k = key.clone();
        drop.connect_drop(move |_, value, _, _| {
            let Ok(list) = value.get::<gdk::FileList>() else { return false };
            let uris: Vec<String> = list.files().iter().map(|f| f.uri().to_string()).collect();
            launch::open_with(&k, &uris);
            true
        });
        crate::stacks::hold_while_dragging(&drop, &root, host(s, d, None));
        root.add_controller(drop);
    }
    Some(Tile { key, root, overlay, image, dots })
}

/// Bring a tile up to date with its item: focus, windows, badge, tooltip.
fn update_tile(s: &Rc<Shared>, d: &Rc<Dock>, t: &Tile) {
    let Some(item) = item_for(d, &t.key) else { return };
    let badge = s.badges.borrow().get(&item.key).cloned().unwrap_or_default();
    set_class(&t.root, "focused", item.focused);
    set_class(&t.root, "urgent", item.urgent || badge.urgent);

    let want = item.windows.len().min(2);
    let mut have = 0;
    let mut child = t.dots.first_child();
    while let Some(c) = child {
        have += 1;
        child = c.next_sibling();
    }
    while have > want {
        if let Some(c) = t.dots.last_child() {
            t.dots.remove(&c);
        }
        have -= 1;
    }
    while have < want {
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("dock-dot");
        t.dots.append(&dot);
        have += 1;
    }

    // Badge and progress: overlays on the icon, made again each time (rare).
    let mut child = t.overlay.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if c != t.image.clone().upcast::<gtk::Widget>() {
            t.overlay.remove_overlay(&c);
        }
    }
    if let Some(count) = badge.count {
        let label = gtk::Label::new(Some(&if count > 99 { "99+".to_string() } else { count.to_string() }));
        label.add_css_class("dock-badge");
        label.set_halign(gtk::Align::End);
        label.set_valign(gtk::Align::Start);
        t.overlay.add_overlay(&label);
    }
    if let Some(p) = badge.progress {
        let bar = gtk::ProgressBar::new();
        bar.set_fraction(p);
        bar.add_css_class("dock-progress");
        bar.set_valign(gtk::Align::End);
        t.overlay.add_overlay(&bar);
    }

    // A running app shows its windows on hover instead of a tooltip.
    let previews = s.config.borrow().previews && item.running();
    let name = app_name(&item.key);
    t.root.set_tooltip_text(if previews { None } else { Some(&name) });
}

/// An icon named after a window's class, if the icon theme has one; else a generic app.
fn fallback_icon(class: &str) -> String {
    let name = class.to_lowercase();
    let has = gdk::Display::default().is_some_and(|d| !name.is_empty() && gtk::IconTheme::for_display(&d).has_icon(&name));
    if has { name } else { "application-x-executable".into() }
}

fn set_class(w: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    if on {
        w.add_css_class(class);
    } else {
        w.remove_css_class(class);
    }
}

fn app_name(key: &str) -> String {
    match key.strip_prefix("class:") {
        Some(class) => class.to_string(),
        None => launch::info(key).map(|i| i.display_name().to_string()).unwrap_or_else(|| key.to_string()),
    }
}

// ---------- Window previews ----------

struct Preview {
    key: String,
    popover: gtk::Popover,
}

/// Preview thumbnails: up to this size in logical pixels (captured at twice that).
const THUMB_W: i32 = 212;
const THUMB_H: i32 = 128;

fn hover_tile(s: &Rc<Shared>, d: &Rc<Dock>, tile: &gtk::Box, key: &str) {
    cancel_close_preview(d);
    if let Some(t) = d.preview_timer.borrow_mut().take() {
        t.remove();
    }
    let running = item_for(d, key).is_some_and(|i| i.running());
    if !s.config.borrow().previews || !running || d.holds.get() > d.preview.borrow().is_some() as u32 {
        return;
    }
    if d.preview.borrow().as_ref().is_some_and(|p| p.key == key) {
        return;
    }
    // Moving along the dock with a preview open switches at once.
    let delay = if d.preview.borrow().is_some() { 0 } else { s.config.borrow().preview_delay_ms as u64 };
    let (ws, wd, tw, k) = (Rc::downgrade(s), Rc::downgrade(d), tile.downgrade(), key.to_string());
    let id = glib::timeout_add_local_once(Duration::from_millis(delay), move || {
        let (Some(s), Some(d), Some(tile)) = (ws.upgrade(), wd.upgrade(), tw.upgrade()) else { return };
        d.preview_timer.borrow_mut().take();
        open_preview(&s, &d, &tile, &k);
    });
    *d.preview_timer.borrow_mut() = Some(id);
}

fn open_preview(s: &Rc<Shared>, d: &Rc<Dock>, tile: &gtk::Box, key: &str) {
    close_preview(s, d);
    let desktop = s.desktop.borrow().clone();
    let (Some(it), Some(monitor)) = (item_for(d, key), desktop.monitor(&d.connector)) else { return };
    let windows: Vec<&nyx_desktop::Window> = it.windows.iter().filter_map(|a| desktop.window(a)).collect();
    if windows.is_empty() {
        return;
    }
    let popover = gtk::Popover::new();
    popover.set_autohide(false);
    popover.set_position(popover_side(d.position));
    popover.add_css_class("dock-preview");
    popover.set_parent(tile);
    let row = gtk::Box::new(
        if matches!(d.position, Position::Bottom | Position::Top) {
            gtk::Orientation::Horizontal
        } else {
            gtk::Orientation::Vertical
        },
        4,
    );
    popover.set_child(Some(&row));

    let icon = launch::info(key)
        .and_then(|i| i.icon())
        .unwrap_or_else(|| gio::ThemedIcon::new(&fallback_icon(key.strip_prefix("class:").unwrap_or(""))).upcast());
    let icon = Some(icon);
    let mut pictures: HashMap<String, (gtk::Picture, gtk::Image)> = HashMap::new();
    for w in windows {
        let (card, picture, placeholder) =
            preview_card(s, d, w, icon.as_ref(), monitor.active_workspace, is_minimized(&desktop, w.workspace));
        row.append(&card);
        if !w.stable_id.is_empty() {
            pictures.insert(w.stable_id.clone(), (picture, placeholder));
        }
    }

    let motion = gtk::EventControllerMotion::new();
    let wd = Rc::downgrade(d);
    motion.connect_enter(move |_, _, _| {
        if let Some(d) = wd.upgrade() {
            cancel_close_preview(&d);
        }
    });
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    motion.connect_leave(move |_| {
        if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
            schedule_close_preview(&s, &d);
        }
    });
    row.add_controller(motion);

    d.holds.set(d.holds.get() + 1);
    cancel_timer(d);
    *d.preview.borrow_mut() = Some(Preview { key: key.to_string(), popover: popover.clone() });
    popover.popup();

    let ids: Vec<String> = pictures.keys().cloned().collect();
    let scale = tile.scale_factor().max(1) as u32;
    nyx_desktop::capture::capture(ids, THUMB_W as u32 * scale, THUMB_H as u32 * scale, move |shot| {
        if let Some((picture, placeholder)) = pictures.get(&shot.stable_id) {
            picture.set_paintable(Some(&shot_texture(shot)));
            placeholder.set_visible(false);
        }
    });
}

fn preview_card(
    s: &Rc<Shared>,
    d: &Rc<Dock>,
    w: &nyx_desktop::Window,
    icon: Option<&gio::Icon>,
    current: i64,
    minimized: bool,
) -> (gtk::Box, gtk::Picture, gtk::Image) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("preview-card");

    // The overlay takes its size from this box; the picture (captured at twice the
    // size, for sharp HiDPI) fills it rather than asking for its own pixel size.
    let thumb = gtk::Overlay::new();
    thumb.add_css_class("preview-thumb");
    let frame = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    frame.set_size_request(THUMB_W, THUMB_H);
    thumb.set_child(Some(&frame));
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_can_shrink(true);
    thumb.add_overlay(&picture);
    let placeholder = gtk::Image::new();
    placeholder.set_pixel_size(48);
    if let Some(icon) = icon {
        placeholder.set_from_gicon(icon);
    }
    placeholder.set_halign(gtk::Align::Center);
    placeholder.set_valign(gtk::Align::Center);
    thumb.add_overlay(&placeholder);

    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("preview-close");
    close.set_tooltip_text(Some("Close window"));
    close.set_halign(gtk::Align::End);
    close.set_valign(gtk::Align::Start);
    let (address, cw) = (w.address.clone(), card.downgrade());
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    close.connect_clicked(move |_| {
        launch::close(&address);
        let (Some(card), Some(s), Some(d)) = (cw.upgrade(), ws.upgrade(), wd.upgrade()) else { return };
        let row = card.parent();
        card.set_visible(false);
        // Last window closed: nothing left to preview.
        let any_left = row.as_ref().and_then(|r| r.first_child()).into_iter().any(|first| {
            let mut c = Some(first);
            while let Some(w) = c {
                if w.is_visible() {
                    return true;
                }
                c = w.next_sibling();
            }
            false
        });
        if !any_left {
            close_preview(&s, &d);
        }
    });
    thumb.add_overlay(&close);
    card.append(&thumb);

    let caption = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    caption.add_css_class("preview-caption");
    let small = gtk::Image::new();
    small.set_pixel_size(16);
    if let Some(icon) = icon {
        small.set_from_gicon(icon);
    }
    caption.append(&small);
    let title = gtk::Label::new(Some(if w.title.is_empty() { &w.class } else { &w.title }));
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(1);
    title.set_hexpand(true);
    title.set_xalign(0.0);
    caption.append(&title);
    let place = if minimized {
        Some(("preview-minimized", "–".to_string(), "Minimized".to_string()))
    } else if nyx_desktop::is_user_workspace(w.workspace) {
        Some(("preview-ws", w.workspace.to_string(), format!("Workspace {}", w.workspace)))
    } else {
        None
    };
    if let Some((class, text, tip)) = place {
        let label = gtk::Label::new(Some(&text));
        label.add_css_class("preview-ws");
        label.add_css_class(class);
        if w.workspace == current {
            label.add_css_class("current");
        }
        label.set_tooltip_text(Some(&tip));
        caption.append(&label);
    }
    card.append(&caption);

    let click = gtk::GestureClick::new();
    click.set_button(1);
    let (ws, wd, address, workspace) = (Rc::downgrade(s), Rc::downgrade(d), w.address.clone(), w.workspace);
    click.connect_released(move |_, _, _, _| {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        let desktop = s.desktop.borrow().clone();
        let current = desktop.monitor(&d.connector).map_or(1, |m| m.active_workspace);
        launch::focus(&address, is_minimized(&desktop, workspace), current);
        close_preview(&s, &d);
    });
    thumb.add_controller(click);
    (card, picture, placeholder)
}

fn schedule_close_preview(s: &Rc<Shared>, d: &Rc<Dock>) {
    if d.preview.borrow().is_none() || d.preview_close.borrow().is_some() {
        return;
    }
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    let id = glib::timeout_add_local_once(Duration::from_millis(300), move || {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        d.preview_close.borrow_mut().take();
        close_preview(&s, &d);
    });
    *d.preview_close.borrow_mut() = Some(id);
}

fn cancel_close_preview(d: &Dock) {
    if let Some(t) = d.preview_close.borrow_mut().take() {
        t.remove();
    }
}

fn close_preview(s: &Rc<Shared>, d: &Rc<Dock>) {
    cancel_close_preview(d);
    let Some(p) = d.preview.borrow_mut().take() else { return };
    p.popover.popdown();
    let pop = p.popover;
    glib::idle_add_local_once(move || pop.unparent());
    d.holds.set(d.holds.get().saturating_sub(1));
    evaluate(s, d);
}

fn is_minimized(desktop: &Desktop, workspace: i64) -> bool {
    desktop.workspaces.iter().any(|w| w.id == workspace && w.name == MINIMIZED)
}

fn activate(s: &Rc<Shared>, d: &Rc<Dock>, item: &Item) {
    let desktop = s.desktop.borrow().clone();
    let Some(monitor) = desktop.monitor(&d.connector) else { return };
    if !item.running() {
        if !item.key.starts_with("class:") {
            launch::launch(&item.key, None);
        }
        return;
    }
    let target = if item.focused { items::next_window(&desktop, item) } else { items::pick_window(&desktop, item, monitor) };
    if let Some(w) = target {
        launch::focus(&w.address, is_minimized(&desktop, w.workspace), monitor.active_workspace);
    }
}

fn menu(s: &Rc<Shared>, d: &Rc<Dock>, tile: &gtk::Box, item: &Item) {
    let is_app = !item.key.starts_with("class:");
    let info = if is_app { launch::info(&item.key) } else { None };
    let model = gio::Menu::new();
    let actions = gio::SimpleActionGroup::new();

    // Quick panel: a player's now-playing and controls first.
    let media = if item.running() { crate::mpris::panel(&item.key) } else { None };
    if media.is_some() {
        let section = gio::Menu::new();
        let entry = gio::MenuItem::new(None, None);
        entry.set_attribute_value("custom", Some(&"media".to_variant()));
        section.append_item(&entry);
        model.append_section(None, &section);
    }

    let first = gio::Menu::new();
    if is_app {
        // The app's own "new window" action stands in for ours.
        let own_new = info.as_ref().is_some_and(|i| {
            i.list_actions().iter().any(|a| matches!(a.as_str(), "new-window" | "new-empty-window" | "NewWindow"))
        });
        if !(item.running() && own_new) {
            first.append(Some(if item.running() { "New window" } else { "Open" }), Some("dock.open"));
        }
        if let Some(info) = &info {
            for a in info.list_actions() {
                let label = info.action_name(&a);
                let entry = gio::MenuItem::new(Some(&label), None);
                entry.set_action_and_target_value(Some("dock.action"), Some(&a.to_variant()));
                first.append_item(&entry);
            }
        }
    }
    model.append_section(None, &first);

    let second = gio::Menu::new();
    if is_app {
        second.append(Some(if item.pinned { "Remove from dock" } else { "Keep in dock" }), Some("dock.pin"));
    }
    if item.running() && launch::switcher_installed() {
        second.append(Some("Show windows"), Some("dock.windows"));
    }
    if item.running() {
        second.append(Some(if item.windows.len() > 1 { "Close all windows" } else { "Quit" }), Some("dock.quit"));
    }
    model.append_section(None, &second);

    let key = item.key.clone();
    let running = item.running();
    let open = gio::SimpleAction::new("open", None);
    let k = key.clone();
    open.connect_activate(move |_, _| if running { launch::new_instance(&k) } else { launch::launch(&k, None) });
    actions.add_action(&open);

    let action = gio::SimpleAction::new("action", Some(glib::VariantTy::STRING));
    let k = key.clone();
    action.connect_activate(move |_, p| {
        if let Some(name) = p.and_then(|p| p.get::<String>()) {
            launch::launch(&k, Some(&name));
        }
    });
    actions.add_action(&action);

    let pin_action = gio::SimpleAction::new("pin", None);
    let (ws, wd, k, pinned) = (Rc::downgrade(s), Rc::downgrade(d), key.clone(), item.pinned);
    pin_action.connect_activate(move |_, _| {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        pin(&s, &d, &k, !pinned, None);
    });
    actions.add_action(&pin_action);

    let show = gio::SimpleAction::new("windows", None);
    let k = key.clone();
    show.connect_activate(move |_, _| launch::show_windows(&k));
    actions.add_action(&show);

    let quit = gio::SimpleAction::new("quit", None);
    let windows = item.windows.clone();
    quit.connect_activate(move |_, _| windows.iter().for_each(|w| launch::close(w)));
    actions.add_action(&quit);

    tile.insert_action_group("dock", Some(&actions));
    let pop = gtk::PopoverMenu::from_model(Some(&model));
    if let Some(media) = &media {
        pop.add_child(media, "media");
    }
    pop.set_parent(tile);
    pop.set_position(popover_side(d.position));
    pop.add_css_class("dock-menu");
    d.holds.set(d.holds.get() + 1);
    cancel_timer(d);
    // Keyboard for the menu (arrows, Esc) only while it's open.
    d.window.set_keyboard_mode(KeyboardMode::OnDemand);
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    pop.connect_closed(move |pop| {
        let pop = pop.clone();
        glib::idle_add_local_once(move || pop.unparent());
        if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
            d.holds.set(d.holds.get().saturating_sub(1));
            d.window.set_keyboard_mode(KeyboardMode::None);
            evaluate(&s, &d);
        }
    });
    pop.popup();
}

fn popover_side(position: Position) -> gtk::PositionType {
    match position {
        Position::Bottom => gtk::PositionType::Top,
        Position::Top => gtk::PositionType::Bottom,
        Position::Left => gtk::PositionType::Right,
        Position::Right => gtk::PositionType::Left,
    }
}

/// What the Apps button needs from its dock.
fn apps_host(s: &Rc<Shared>, d: &Rc<Dock>) -> Rc<crate::apps::Host> {
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    let hold = Box::new(move |on: bool| {
        if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
            self::hold(&s, &d, on);
        }
    });
    let wd = Rc::downgrade(d);
    let keyboard = Box::new(move |on: bool| {
        if let Some(d) = wd.upgrade() {
            // On demand, as for the app menus: taking the keyboard outright moves the
            // focus, and Hyprland closes the panel that just opened.
            d.window.set_keyboard_mode(if on { KeyboardMode::OnDemand } else { KeyboardMode::None });
        }
    });
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    let is_pinned = Box::new(move |id: &str| {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return false };
        let pins = d.eff().pinned.clone();
        shown_pins(&s, &pins).iter().any(|p| p == id)
    });
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    let set_pinned = Box::new(move |id: &str, on: bool| {
        if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
            pin(&s, &d, id, on, None);
        }
    });
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    let drag = Box::new(move |id: Option<&str>| {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        match id {
            Some(id) => begin_drag(&s, &d, id),
            None => end_drag(&s, &d),
        }
    });
    Rc::new(crate::apps::Host {
        hold,
        keyboard,
        is_pinned,
        set_pinned,
        drag,
        side: popover_side(d.position),
        icon_size: d.eff().icon_size as i32,
        max_height: max_height(d, 520),
        names: s.config.borrow().app_names,
    })
}

// ---------- The dock's own menu ----------

/// Right-click on the dock itself (not on an app or a folder): its settings.
fn bar_menu(s: &Rc<Shared>, dock: &Rc<Dock>) {
    let click = gtk::GestureClick::new();
    click.set_button(3);
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(dock));
    click.connect_released(move |_, _, x, y| {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        let bar: gtk::Widget = d.bar.clone().upcast();
        let mut w = d.bar.pick(x, y, gtk::PickFlags::DEFAULT);
        while let Some(c) = w.filter(|c| *c != bar) {
            if c.has_css_class("dock-item") && !c.has_css_class("dock-apps") {
                return;
            }
            w = c.parent();
        }
        dock_menu(&s, &d, x, y);
    });
    dock.bar.add_controller(click);
}

/// A row in the dock menu: a title, and its control at the end.
fn setting_row(title: &str, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("dock-setting");
    let label = gtk::Label::new(Some(title));
    label.set_xalign(0.0);
    label.set_hexpand(true);
    row.append(&label);
    row.append(control);
    row
}

/// Buttons side by side, one of them chosen.
fn segmented(options: &[(&str, &str)], current: &str, on: impl Fn(&str) + 'static) -> gtk::Box {
    let seg = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    seg.add_css_class("dock-seg");
    let buttons: Vec<gtk::ToggleButton> = options.iter().map(|(_, label)| gtk::ToggleButton::with_label(label)).collect();
    for (i, b) in buttons.iter().enumerate() {
        if i > 0 {
            b.set_group(Some(&buttons[0]));
        }
        seg.append(b);
    }
    for (b, (value, _)) in buttons.iter().zip(options) {
        b.set_active(*value == current);
    }
    let on = Rc::new(on);
    for (b, (value, _)) in buttons.iter().zip(options) {
        let (on, value) = (on.clone(), value.to_string());
        b.connect_toggled(move |b| {
            if b.is_active() {
                on(&value);
            }
        });
    }
    seg
}

fn switch(active: bool, on: impl Fn(bool) + 'static) -> gtk::Switch {
    let sw = gtk::Switch::new();
    sw.set_active(active);
    sw.set_valign(gtk::Align::Center);
    sw.connect_active_notify(move |sw| on(sw.is_active()));
    sw
}

/// Change a setting from the dock menu: the config, this monitor's stable id and
/// connector.
type Apply = Rc<dyn Fn(&dyn Fn(&mut Config, &str, &str))>;

fn dock_menu(s: &Rc<Shared>, d: &Rc<Dock>, x: f64, y: f64) {
    let eff = d.eff().clone();
    let config = s.config.borrow().clone();
    let pop = gtk::Popover::new();
    pop.add_css_class("dock-menu");
    let col = gtk::Box::new(gtk::Orientation::Vertical, 2);
    col.add_css_class("dock-settings");
    pop.set_child(Some(&col));

    let apply: Apply = {
        let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
        Rc::new(move |f: &dyn Fn(&mut Config, &str, &str)| {
            if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
                change(&s, &d, f);
            }
        })
    };

    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 24.0, 96.0, 4.0);
    scale.set_value(eff.icon_size as f64);
    scale.set_draw_value(false);
    scale.set_size_request(180, -1);
    // Saved once the slider rests: each size redraws the dock.
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let a = apply.clone();
    scale.connect_value_changed(move |sc| {
        if let Some(t) = pending.borrow_mut().take() {
            t.remove();
        }
        let size = sc.value().round() as u32;
        let (a, p) = (a.clone(), pending.clone());
        let id = glib::timeout_add_local_once(Duration::from_millis(200), move || {
            p.borrow_mut().take();
            a(&|c, id, conn| c.set_icon_size(id, conn, size));
        });
        *pending.borrow_mut() = Some(id);
    });
    col.append(&setting_row("Size", &scale));

    let current = format!("{:?}", eff.position).to_lowercase();
    let a = apply.clone();
    let position = segmented(
        &[("bottom", "Bottom"), ("left", "Left"), ("right", "Right"), ("top", "Top")],
        &current,
        move |v| {
            let p = match v {
                "left" => Position::Left,
                "right" => Position::Right,
                "top" => Position::Top,
                _ => Position::Bottom,
            };
            a(&|c, id, conn| c.set_position(id, conn, p));
        },
    );
    col.append(&setting_row("Position", &position));

    let current = format!("{:?}", eff.hide).to_lowercase();
    let a = apply.clone();
    let hiding = segmented(&[("smart", "Smart"), ("always", "Always"), ("never", "Never")], &current, move |v| {
        let h = match v {
            "always" => Hide::Always,
            "never" => Hide::Never,
            _ => Hide::Smart,
        };
        a(&|c, id, conn| c.set_hide(id, conn, h));
    });
    let row = setting_row("Hide", &hiding);
    row.set_tooltip_text(Some(
        "Smart: while a window covers the dock. Always: until the pointer reaches the edge. \
         Never: stays up on floating workspaces, with windows kept above it.",
    ));
    col.append(&row);

    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    col.append(&sep);
    let a = apply.clone();
    col.append(&setting_row("Window previews", &switch(config.previews, move |on| a(&|c, _, _| c.previews = on))));
    let a = apply.clone();
    col.append(&setting_row("Apps button", &switch(config.apps, move |on| a(&|c, _, _| c.apps = on))));
    let a = apply.clone();
    let names = setting_row("App names", &switch(config.app_names, move |on| a(&|c, _, _| c.app_names = on)));
    names.set_tooltip_text(Some("Names under the icons in the Apps panel. Off: icons only."));
    col.append(&names);
    let a = apply.clone();
    col.append(&setting_row("Trash", &switch(config.trash, move |on| a(&|c, _, _| c.trash = on))));

    col.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let add = gtk::Button::with_label("Add Folder…");
    add.add_css_class("flat");
    let (ws, pw) = (Rc::downgrade(s), pop.downgrade());
    add.connect_clicked(move |_| {
        if let Some(p) = pw.upgrade() {
            p.popdown();
        }
        if let Some(s) = ws.upgrade() {
            choose_folder(&s);
        }
    });
    col.append(&add);
    if launch::settings_installed() {
        let more = gtk::Button::with_label("Dock Settings…");
        more.add_css_class("flat");
        let pw = pop.downgrade();
        more.connect_clicked(move |_| {
            if let Some(p) = pw.upgrade() {
                p.popdown();
            }
            launch::dock_settings();
        });
        col.append(&more);
    }

    pop.set_parent(&d.bar);
    pop.set_autohide(!testing());
    pop.set_pointing_to(Some(&gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    pop.set_position(popover_side(d.position));
    hold(s, d, true);
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    pop.connect_closed(move |pop| {
        let pop = pop.clone();
        glib::idle_add_local_once(move || pop.unparent());
        if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
            hold(&s, &d, false);
        }
    });
    pop.popup();
}

// ---------- Hiding ----------

/// Where the bar sits on its monitor, in logical pixels (without the transparent
/// margin around it).
fn dock_rect(d: &Dock) -> Rect {
    let geo = d.gmon.geometry();
    let (mut w, mut h) = (d.window.width(), d.window.height());
    if w > 0 && h > 0 {
        d.size.set((w, h));
    } else {
        (w, h) = d.size.get();
    }
    let (ox, oy) = match d.position {
        Position::Bottom => ((geo.width() - w) / 2, geo.height() - h),
        Position::Top => ((geo.width() - w) / 2, 0),
        Position::Left => (0, (geo.height() - h) / 2),
        Position::Right => (geo.width() - w, (geo.height() - h) / 2),
    };
    match d.bar.compute_bounds(&d.window) {
        Some(b) if b.width() > 0.0 => {
            Rect { x: ox + b.x() as i32, y: oy + b.y() as i32, w: b.width().ceil() as i32, h: b.height().ceil() as i32 }
        }
        _ => Rect { x: ox, y: oy, w, h },
    }
}

/// Whether the workspace in front on `monitor` is in Omarchy's floating mode.
fn floating_here(desktop: &Desktop, monitor: &Monitor) -> bool {
    let layout = desktop.workspaces.iter().find(|w| w.id == monitor.active_workspace).map_or("", |w| w.tiled_layout.as_str());
    nyx_desktop::omarchy::workspace_mode(monitor.active_workspace, layout) == nyx_desktop::omarchy::Mode::Floating
}

/// The hiding that applies on the workspace in front.
fn hide_here(d: &Dock, desktop: &Desktop, monitor: &Monitor) -> Hide {
    let mode = d.eff().hide;
    if mode != Hide::Never {
        return mode;
    }
    mode_here(mode, floating_here(desktop, monitor))
}

fn evaluate(s: &Rc<Shared>, d: &Rc<Dock>) {
    let desktop = s.desktop.borrow().clone();
    let Some(monitor): Option<&Monitor> = desktop.monitor(&d.connector) else { return };
    let mode = hide_here(d, &desktop, monitor);
    let windows_want = should_hide(mode, &desktop, monitor, dock_rect(d));
    debug(|| {
        format!(
            "{}: rect {:?}, {mode:?}, windows want it hidden: {windows_want}, hidden {}",
            d.connector,
            dock_rect(d),
            d.hidden.get()
        )
    });
    if windows_want && !d.hover.get() && d.holds.get() == 0 {
        if !d.hidden.get() && d.timer.borrow().is_none() {
            schedule_hide(s, d, s.config.borrow().hide_delay_ms as u64);
        }
    } else {
        cancel_timer(d);
        if !windows_want {
            show(d);
        }
    }
    reserve(d, mode == Hide::Never);
}

/// A dock that stays up keeps its space, as on macOS: maximized and new windows stay
/// above it.
fn reserve(d: &Dock, on: bool) {
    let bounds = d.bar.compute_bounds(&d.window).filter(|b| b.width() > 0.0);
    let zone = match bounds {
        Some(b) if on && !d.hidden.get() => match d.position {
            Position::Bottom => d.window.height() - b.y() as i32,
            Position::Top => (b.y() + b.height()).ceil() as i32,
            Position::Left => (b.x() + b.width()).ceil() as i32,
            Position::Right => d.window.width() - b.x() as i32,
        },
        _ => 0,
    }
    .max(0);
    if d.window.exclusive_zone() != zone {
        debug(|| format!("{}: keep {zone} px for the dock", d.connector));
        d.window.set_exclusive_zone(zone);
    }
}

/// Soon (once the windows settle), move floating windows off a dock that stays up.
fn schedule_fit(s: &Rc<Shared>, d: &Rc<Dock>) {
    if let Some(t) = d.fit_timer.borrow_mut().take() {
        t.remove();
    }
    let desktop = s.desktop.borrow().clone();
    let Some(monitor) = desktop.monitor(&d.connector) else { return };
    if hide_here(d, &desktop, monitor) != Hide::Never {
        return;
    }
    let wd = Rc::downgrade(d);
    let id = glib::timeout_add_local_once(Duration::from_millis(300), move || {
        if let Some(d) = wd.upgrade() {
            d.fit_timer.borrow_mut().take();
            fit_windows(&d);
        }
    });
    *d.fit_timer.borrow_mut() = Some(id);
}

/// Floating windows that reach behind a dock that stays up move off it, and get
/// smaller if they don't fit. Reads the windows afresh: a dragged window sends no event.
fn fit_windows(d: &Dock) {
    use nyx_desktop::actions::Dispatch;
    let Ok(hypr) = nyx_desktop::hypr::Instance::find() else { return };
    let Ok(desktop) = Desktop::snapshot(&hypr) else { return };
    let Some(m) = desktop.monitor(&d.connector) else { return };
    if d.hidden.get() || hide_here(d, &desktop, m) != Hide::Never {
        return;
    }
    let geo = d.gmon.geometry();
    let r = dock_rect(d);
    let dock = Rect { x: m.x + r.x, y: m.y + r.y, w: r.w, h: r.h };
    let [left, top, right, bottom] = m.reserved;
    let area = Rect { x: m.x + left, y: m.y + top, w: geo.width() - left - right, h: geo.height() - top - bottom };
    for w in desktop.visible_windows(m).filter(|w| w.floating && w.fullscreen == 0) {
        let Some((at, size)) = clear_of_dock(d.position, area, dock, w.at, w.size) else { continue };
        debug(|| format!("{}: {} {:?} {:?} -> {at:?} {size:?}", d.connector, w.class, w.at, w.size));
        let (a, target) = (&w.address, format!("window = \"address:0x{}\"", w.address));
        if size != w.size {
            let [sw, sh] = size;
            Dispatch {
                lua: format!("hl.dsp.window.resize({{ x = {sw}, y = {sh}, relative = false, {target} }})"),
                classic: format!("resizewindowpixel exact {sw} {sh},address:0x{a}"),
            }
            .run_on(&hypr);
        }
        let (dx, dy) = (at[0] - w.at[0], at[1] - w.at[1]);
        if dx != 0 || dy != 0 {
            Dispatch {
                lua: format!("hl.dsp.window.move({{ x = {dx}, y = {dy}, relative = true, {target} }})"),
                classic: format!("movewindowpixel {dx} {dy},address:0x{a}"),
            }
            .run_on(&hypr);
        }
    }
}

fn schedule_hide(s: &Rc<Shared>, d: &Rc<Dock>, ms: u64) {
    cancel_timer(d);
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    let id = glib::timeout_add_local_once(Duration::from_millis(ms), move || {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        d.timer.borrow_mut().take();
        let desktop = s.desktop.borrow().clone();
        let Some(monitor) = desktop.monitor(&d.connector) else { return };
        let mode = hide_here(&d, &desktop, monitor);
        if should_hide(mode, &desktop, monitor, dock_rect(&d)) && !d.hover.get() && d.holds.get() == 0 {
            d.hidden.set(true);
            d.window.set_visible(false);
            reserve(&d, false);
        }
    });
    *d.timer.borrow_mut() = Some(id);
}

fn cancel_timer(d: &Dock) {
    if let Some(t) = d.timer.borrow_mut().take() {
        t.remove();
    }
}

fn show(d: &Dock) {
    if d.hidden.replace(false) {
        d.window.set_visible(true);
    }
}
