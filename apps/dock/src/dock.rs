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
use nyx_dock_core::hide::{Rect, should_hide};
use nyx_dock_core::items::{self, Item, MINIMIZED};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

/// `NYX_DOCK_DEBUG=1` prints what the dock decides.
fn debug(msg: impl FnOnce() -> String) {
    thread_local!(static ON: bool = std::env::var_os("NYX_DOCK_DEBUG").is_some());
    if ON.with(|on| *on) {
        eprintln!("nyx-dock: {}", msg());
    }
}

struct Shared {
    app: gtk::Application,
    only: Option<String>,
    config: RefCell<Config>,
    desktop: RefCell<Arc<Desktop>>,
    index: RefCell<Index>,
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
    eff: Effective,
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
    /// The surface's size when last shown, for working out where it sits.
    size: Cell<(i32, i32)>,
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
        if u.changes.monitors || s.docks.borrow().is_empty() {
            rebuild_docks(&s);
        } else if u.changes.windows || u.changes.focus || u.changes.workspaces || u.changes.active {
            refresh(&s);
        }
    }));

    // Apps installed or removed: match windows again.
    let ws = Rc::downgrade(&s);
    let monitor = gio::AppInfoMonitor::get();
    monitor.connect_changed(move |_| {
        if let Some(s) = ws.upgrade() {
            *s.index.borrow_mut() = build_index();
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
            rebuild_docks(&s);
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
                    rebuild_docks(&s);
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

fn rebuild_docks(s: &Rc<Shared>) {
    for d in s.docks.borrow_mut().drain(..) {
        if let Some(t) = d.timer.borrow_mut().take() {
            t.remove();
        }
        d.window.destroy();
        d.edge.destroy();
    }
    let desktop = s.desktop.borrow().clone();
    let Some(display) = gdk::Display::default() else { return };
    let gmons: Vec<gdk::Monitor> = display.monitors().iter::<gdk::Monitor>().flatten().collect();
    let config = s.config.borrow().clone();
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
        debug(|| format!("dock on {} ({:?})", m.name, eff));
        let dock = new_dock(s, &m.name, gmon, eff);
        s.docks.borrow_mut().push(dock);
    }
    refresh_all(s);
}

fn edges(position: Position) -> (Edge, gtk::Orientation) {
    match position {
        Position::Bottom => (Edge::Bottom, gtk::Orientation::Horizontal),
        Position::Top => (Edge::Top, gtk::Orientation::Horizontal),
        Position::Left => (Edge::Left, gtk::Orientation::Vertical),
        Position::Right => (Edge::Right, gtk::Orientation::Vertical),
    }
}

fn new_dock(s: &Rc<Shared>, connector: &str, gmon: &gdk::Monitor, eff: Effective) -> Rc<Dock> {
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

    let dock = Rc::new(Dock {
        connector: connector.to_string(),
        gmon: gmon.clone(),
        eff,
        window,
        bar,
        edge: strip,
        items: RefCell::default(),
        tiles: RefCell::default(),
        preview: RefCell::default(),
        preview_timer: RefCell::default(),
        preview_close: RefCell::default(),
        hidden: Cell::new(false),
        hover: Cell::new(false),
        holds: Cell::new(0),
        timer: RefCell::default(),
        size: Cell::new((0, 0)),
    });

    let motion = gtk::EventControllerMotion::new();
    let (wd, ws) = (Rc::downgrade(&dock), Rc::downgrade(s));
    motion.connect_enter(move |_, _, _| {
        if let Some(d) = wd.upgrade() {
            d.hover.set(true);
            cancel_timer(&d);
        }
    });
    let reorder = gtk::DropTarget::new(glib::BoxedAnyObject::static_type(), gdk::DragAction::MOVE);
    let (ws2, wd2) = (Rc::downgrade(s), Rc::downgrade(&dock));
    reorder.connect_drop(move |_, value, x, y| {
        let (Some(s), Some(d)) = (ws2.upgrade(), wd2.upgrade()) else { return false };
        let Ok(obj) = value.get::<glib::BoxedAnyObject>() else { return false };
        let Ok(drag) = obj.try_borrow::<DockDrag>() else { return false };
        let at = pin_index(&d, if horizontal { x } else { y }, &drag.0);
        pin(&s, &d, &drag.0, true, Some(at));
        true
    });
    dock.bar.add_controller(reorder);

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

    dock.window.present();
    if dock.eff.hide != Hide::Never {
        dock.edge.present();
    }
    dock
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
    // The edge strip is as wide (or tall) as the dock.
    let horizontal = matches!(d.eff.position, Position::Bottom | Position::Top);
    if let Some(child) = d.edge.child() {
        if horizontal {
            child.set_size_request(r.width().max(48), 2);
        } else {
            child.set_size_request(2, r.height().max(48));
        }
    }
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
        let order = s.orders.borrow().get(&d.connector).cloned().unwrap_or_default();
        let (items, order) = items::compose(&d.eff.pinned, &desktop, &s.index.borrow(), d.eff.running, monitor, &order);
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
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        if on {
            close_preview(&s, &d);
            d.holds.set(d.holds.get() + 1);
            cancel_timer(&d);
        } else {
            d.holds.set(d.holds.get().saturating_sub(1));
            evaluate(&s, &d);
        }
    });
    let ws = Rc::downgrade(s);
    let remove = Box::new(move || {
        let (Some(s), Some(i)) = (ws.upgrade(), folder) else { return };
        if i < s.config.borrow().folders.len() {
            s.config.borrow_mut().folders.remove(i);
            save(&s);
            let s2 = s.clone();
            glib::idle_add_local_once(move || rebuild_docks(&s2));
        }
    });
    let ws = Rc::downgrade(s);
    let set_sort = Box::new(move |sort| {
        let (Some(s), Some(i)) = (ws.upgrade(), folder) else { return };
        if let Some(f) = s.config.borrow_mut().folders.get_mut(i) {
            f.sort = sort;
        }
        save(&s);
    });
    Rc::new(crate::stacks::Host { hold, remove, set_sort, side: popover_side(d.eff.position), icon_size: d.eff.icon_size as i32 })
}

/// An app being dragged within the dock.
struct DockDrag(String);

fn pin(s: &Rc<Shared>, d: &Rc<Dock>, key: &str, pinned: bool, at: Option<usize>) {
    let desktop = s.desktop.borrow().clone();
    let Some(m) = desktop.monitor(&d.connector) else { return };
    s.config.borrow_mut().set_pinned(&m.stable_id(), &m.name, key, pinned, at);
    save(s);
    let s2 = s.clone();
    glib::idle_add_local_once(move || rebuild_docks(&s2));
}

/// Where a drop at `pos` (along the dock) lands among the pinned apps, not counting
/// the app being moved.
fn pin_index(d: &Dock, pos: f64, moving: &str) -> usize {
    let horizontal = matches!(d.eff.position, Position::Bottom | Position::Top);
    let mut index = 0;
    for t in d.tiles.borrow().iter() {
        let pinned = item_for(d, &t.key).is_some_and(|i| i.pinned);
        if !pinned || t.key == moving {
            continue;
        }
        let Some(b) = t.root.compute_bounds(&d.bar) else { continue };
        let middle = if horizontal { b.x() + b.width() / 2.0 } else { b.y() + b.height() / 2.0 };
        if (pos as f32) < middle {
            break;
        }
        index += 1;
    }
    index
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
    while let Some(child) = d.bar.first_child() {
        d.bar.remove(&child);
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
    image.set_pixel_size(d.eff.icon_size as i32);
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
        let (ws, wd, iw) = (Rc::downgrade(s), Rc::downgrade(d), image.downgrade());
        drag.connect_drag_begin(move |src, _| {
            if let Some(image) = iw.upgrade() {
                let paintable = gtk::WidgetPaintable::new(Some(&image));
                src.set_icon(Some(&paintable), image.width() / 2, image.height() / 2);
            }
            if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
                close_preview(&s, &d);
                d.holds.set(d.holds.get() + 1);
                cancel_timer(&d);
            }
        });
        let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
        drag.connect_drag_end(move |_, _, _| {
            if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
                d.holds.set(d.holds.get().saturating_sub(1));
                evaluate(&s, &d);
            }
        });
        let (ws, wd, k) = (Rc::downgrade(s), Rc::downgrade(d), key.clone());
        drag.connect_drag_cancel(move |_, _, reason| {
            let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return false };
            // Let go away from any drop target: off the dock.
            if reason == gdk::DragCancelReason::NoTarget && item_for(&d, &k).is_some_and(|i| i.pinned) {
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
    popover.set_position(popover_side(d.eff.position));
    popover.add_css_class("dock-preview");
    popover.set_parent(tile);
    let row = gtk::Box::new(
        if matches!(d.eff.position, Position::Bottom | Position::Top) {
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
    pop.set_position(popover_side(d.eff.position));
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
    let (ox, oy) = match d.eff.position {
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

fn evaluate(s: &Rc<Shared>, d: &Rc<Dock>) {
    let desktop = s.desktop.borrow().clone();
    let Some(monitor): Option<&Monitor> = desktop.monitor(&d.connector) else { return };
    let windows_want = should_hide(d.eff.hide, &desktop, monitor, dock_rect(d));
    debug(|| {
        format!("{}: rect {:?}, windows want it hidden: {windows_want}, hidden {}", d.connector, dock_rect(d), d.hidden.get())
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
}

fn schedule_hide(s: &Rc<Shared>, d: &Rc<Dock>, ms: u64) {
    cancel_timer(d);
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    let id = glib::timeout_add_local_once(Duration::from_millis(ms), move || {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        d.timer.borrow_mut().take();
        let desktop = s.desktop.borrow().clone();
        let Some(monitor) = desktop.monitor(&d.connector) else { return };
        if should_hide(d.eff.hide, &desktop, monitor, dock_rect(&d)) && !d.hover.get() && d.holds.get() == 0 {
            d.hidden.set(true);
            d.window.set_visible(false);
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
