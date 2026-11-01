//! The dock windows: one layer-shell surface per monitor, plus a thin edge strip that
//! brings a hidden dock back.

use crate::badges::{self, Badge};
use crate::launch;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use nyx_desktop::{Desktop, Monitor, watch};
use nyx_dock_core::apps::{Entry, Index};
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

fn build_index() -> Index {
    let entries: Vec<Entry> = gio::AppInfo::all()
        .into_iter()
        .filter_map(|a| a.downcast::<gio_unix::DesktopAppInfo>().ok())
        .filter_map(|a| {
            let id = a.id()?.trim_end_matches(".desktop").to_string();
            Some(Entry {
                id,
                wm_class: a.startup_wm_class().map(|c| c.to_string()),
                exec: a.string("Exec").map(|c| c.to_string()).unwrap_or_default(),
                terminal: a.boolean("Terminal"),
            })
        })
        .collect();
    Index::new(&entries)
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
            *d.items.borrow_mut() = items;
            fill(s, &d);
        }
        evaluate(s, &d);
    }
}

fn fill(s: &Rc<Shared>, d: &Rc<Dock>) {
    while let Some(child) = d.bar.first_child() {
        d.bar.remove(&child);
    }
    let items = d.items.borrow().clone();
    for item in &items {
        if let Some(w) = item_widget(s, d, item) {
            d.bar.append(&w);
        }
    }
    let wd = Rc::downgrade(d);
    glib::idle_add_local_once(move || {
        if let Some(d) = wd.upgrade() {
            fit_input(&d);
        }
    });
}

fn item_widget(s: &Rc<Shared>, d: &Rc<Dock>, item: &Item) -> Option<gtk::Widget> {
    let class = item.key.strip_prefix("class:");
    let info = if class.is_some() { None } else { launch::info(&item.key) };
    // A pinned app that's no longer installed has nothing to show or start.
    if class.is_none() && info.is_none() {
        return None;
    }
    let tile = gtk::Box::new(gtk::Orientation::Vertical, 2);
    tile.add_css_class("dock-item");
    if item.focused {
        tile.add_css_class("focused");
    }

    let image = gtk::Image::new();
    image.set_pixel_size(d.eff.icon_size as i32);
    match info.as_ref().and_then(|i| i.icon()) {
        Some(icon) => image.set_from_gicon(&icon),
        None => image.set_icon_name(Some(&class.unwrap_or("application-x-executable").to_lowercase())),
    }
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&image));

    let badge = s.badges.borrow().get(&item.key).cloned().unwrap_or_default();
    if let Some(count) = badge.count {
        let label = gtk::Label::new(Some(&if count > 99 { "99+".to_string() } else { count.to_string() }));
        label.add_css_class("dock-badge");
        label.set_halign(gtk::Align::End);
        label.set_valign(gtk::Align::Start);
        overlay.add_overlay(&label);
    }
    if let Some(p) = badge.progress {
        let bar = gtk::ProgressBar::new();
        bar.set_fraction(p);
        bar.add_css_class("dock-progress");
        bar.set_valign(gtk::Align::End);
        overlay.add_overlay(&bar);
    }
    if item.urgent || badge.urgent {
        tile.add_css_class("urgent");
    }
    tile.append(&overlay);

    let dots = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    dots.add_css_class("dock-dots");
    dots.set_halign(gtk::Align::Center);
    for _ in 0..item.windows.len().min(2) {
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("dock-dot");
        dots.append(&dot);
    }
    tile.append(&dots);

    let name = info.as_ref().map(|i| i.display_name().to_string()).unwrap_or_else(|| class.unwrap_or("").to_string());
    tile.set_tooltip_text(Some(&name));

    let click = gtk::GestureClick::new();
    click.set_button(0);
    let (ws, wd, it) = (Rc::downgrade(s), Rc::downgrade(d), item.clone());
    let tile_weak = tile.downgrade();
    click.connect_released(move |g, _, _, _| {
        let (Some(s), Some(d), Some(tile)) = (ws.upgrade(), wd.upgrade(), tile_weak.upgrade()) else { return };
        match g.current_button() {
            1 => activate(&s, &d, &it),
            2 if !it.key.starts_with("class:") => launch::new_instance(&it.key),
            3 => menu(&s, &d, &tile, &it),
            _ => {}
        }
    });
    tile.add_controller(click);
    Some(tile.upcast())
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

    let first = gio::Menu::new();
    if is_app {
        first.append(Some(if item.running() { "New window" } else { "Open" }), Some("dock.open"));
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

    let pin = gio::SimpleAction::new("pin", None);
    let (ws, wd, k, pinned) = (Rc::downgrade(s), Rc::downgrade(d), key.clone(), item.pinned);
    pin.connect_activate(move |_, _| {
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        let desktop = s.desktop.borrow().clone();
        let Some(m) = desktop.monitor(&d.connector) else { return };
        s.config.borrow_mut().set_pinned(&m.stable_id(), &m.name, &k, !pinned, None);
        save(&s);
        rebuild_docks(&s);
    });
    actions.add_action(&pin);

    let quit = gio::SimpleAction::new("quit", None);
    let windows = item.windows.clone();
    quit.connect_activate(move |_, _| windows.iter().for_each(|w| launch::close(w)));
    actions.add_action(&quit);

    tile.insert_action_group("dock", Some(&actions));
    let pop = gtk::PopoverMenu::from_model(Some(&model));
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
