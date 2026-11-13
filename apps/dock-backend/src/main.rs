//! `nyx-dock-backend`: Nyx Dock without GTK, for its Quickshell interface
//! (`qml/Dock` in this repository).
//!
//! Everything the dock decides stays here, in the same code as the GTK dock: which apps
//! show and in what order (nyx-dock-core's `items`), when the dock hides (`hide`,
//! the settle-and-reverify state machine, swipe following from the Hyprland plugin),
//! moving floating windows off a dock that stays up, launching, badges, players, the
//! cards' lists, folders, the Trash and window pictures. The interface draws and
//! reports what only it knows: the bar's rectangle, the pointer over it, and what it has
//! open (`Hover`, `Hold`, `Geometry`).
//!
//! Topic `dock`: `{global, monitors: [{name, stable_id, settings, items, hidden,
//! offset, slide_ms, reserve}]}`.

#[path = "../../dock/src/badges.rs"]
mod badges;
#[path = "../../dock/src/launch.rs"]
mod launch;
#[path = "../../dock/src/swipe.rs"]
mod swipe;

mod cards;
mod controls;
mod files;

/// For the swipe listener (it logs through `crate::dock::debug`).
mod dock {
    pub fn debug(msg: impl FnOnce() -> String) {
        crate::debug(msg)
    }
}

use gio::prelude::*;
use nyx_desktop::apps::Index;
use nyx_desktop::{Desktop, Monitor, watch};
use nyx_dock_core::config::{ClickAnimation, Config, Effective, Hide, Position, first_pins};
use nyx_dock_core::hide::{Rect, clear_of_dock, fullscreen_on, mode_here, should_hide, should_hide_on};
use nyx_dock_core::items::{self, Item, MINIMIZED};
use nyx_dock_core::swipe::{SwipeEvent, resolve_loop};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

pub fn debug(msg: impl FnOnce() -> String) {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    if *ON.get_or_init(|| std::env::var_os("NYX_DOCK_DEBUG").is_some()) {
        eprintln!("nyx-dock: {}", msg());
    }
}

const HIDE_MS: u32 = 180;
const SHOW_MS: u32 = 200;
const SWIPE_SETTLE_MS: u32 = 120;
const SETTLE_MS: u64 = 120;

pub struct Shared {
    link: nyx_ui_link::Link,
    only: Option<String>,
    config: RefCell<Config>,
    desktop: RefCell<Arc<Desktop>>,
    index: RefCell<Index>,
    aliases: RefCell<HashMap<String, String>>,
    badges: RefCell<HashMap<String, badges::Badge>>,
    docks: RefCell<Vec<Rc<Dock>>>,
    orders: RefCell<HashMap<String, Vec<String>>>,
    saving: Cell<bool>,
    publish_pending: Cell<bool>,
}

/// Floating windows moved off the dock: address to where and how big they were.
type Fitted = HashMap<String, ([i32; 2], [i32; 2])>;

pub struct Dock {
    connector: String,
    stable_id: String,
    eff: RefCell<Effective>,
    items: RefCell<Vec<Item>>,
    hidden: Cell<bool>,
    hover: Cell<bool>,
    holds: Cell<u32>,
    /// The bar's rectangle on its monitor when up, logical pixels (from the interface).
    rect: Cell<Rect>,
    /// A swipe moving the dock: how far out of sight (0–1); None otherwise.
    offset: Cell<Option<f64>>,
    slide_ms: Cell<u32>,
    reserve: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    settle_timer: RefCell<Option<glib::SourceId>>,
    settle_gen: Cell<u32>,
    settled_ws: Cell<(i64, Option<i64>)>,
    swipe: Cell<Option<(i64, bool)>>,
    swipe_watchdog: RefCell<Option<glib::SourceId>>,
    fit_timer: RefCell<Option<glib::SourceId>>,
    fitting: Cell<bool>,
    fitted: RefCell<Fitted>,
    checking_pointer: Cell<bool>,
}

impl Dock {
    fn position(&self) -> Position {
        self.eff.borrow().position
    }
}

// ---------- Settings ----------

fn load_config() -> Config {
    let store = nyx_dock_core::config::store();
    match Config::load_layered(&store) {
        Some((c, problems)) => {
            for p in problems {
                eprintln!("nyx-dock: {p}");
            }
            c
        }
        None => {
            let browser = gio::AppInfo::default_for_uri_scheme("https").and_then(|a| a.id()).map(|s| s.to_string());
            let (base, _) = nyx_config::parse::<Config>(&store.load());
            let config = Config { pinned: first_pins(omarchy_pins(), browser.as_deref(), |id| launch::info(id).is_some()), ..base };
            let _ = config.save_layered(&store);
            config
        }
    }
}

fn omarchy_pins() -> Option<Vec<String>> {
    let text = std::fs::read_to_string(glib::user_config_dir().join("omarchy/shell.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let pins = v.get("dock")?.get("pinned")?.as_array()?;
    Some(pins.iter().filter_map(|p| p.as_str()).map(|p| p.trim_end_matches(".desktop").to_string()).collect())
}

fn save(s: &Rc<Shared>) {
    s.saving.set(true);
    if let Err(e) = s.config.borrow().save_layered(&nyx_dock_core::config::store()) {
        eprintln!("nyx-dock: couldn't save the settings: {e}");
    }
    let ws = Rc::downgrade(s);
    glib::timeout_add_local_once(Duration::from_millis(500), move || {
        if let Some(s) = ws.upgrade() {
            s.saving.set(false);
        }
    });
}

// ---------- Docks and items ----------

fn shown_pins(s: &Shared, pins: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(pins.len());
    for p in pins {
        let shown = s.aliases.borrow_mut().entry(p.clone()).or_insert_with(|| launch::stands_for(p).unwrap_or_else(|| p.clone())).clone();
        if !out.contains(&shown) {
            out.push(shown);
        }
    }
    out
}

fn pin_source(s: &Shared, d: &Dock, key: &str) -> String {
    let source = s.aliases.borrow().iter().find(|(from, to)| *to == key && *from != key && d.eff.borrow().pinned.contains(from)).map(|(f, _)| f.clone());
    source.unwrap_or_else(|| key.to_string())
}

/// Match the docks to the monitors and settings (as the GTK dock's `rebuild_docks`).
fn update_controls(s: &Shared) {
    let enabled = s.config.borrow().window_controls;
    controls::update(&s.link, enabled, s.only.as_deref(), &s.desktop.borrow());
}

fn rebuild(s: &Rc<Shared>) {
    update_controls(s);
    let desktop = s.desktop.borrow().clone();
    let config = s.config.borrow().clone();
    let old: Vec<Rc<Dock>> = s.docks.borrow_mut().drain(..).collect();
    let mut docks = Vec::new();
    for m in desktop.monitors.iter().filter(|m| !m.disabled) {
        if s.only.as_ref().is_some_and(|o| *o != m.name) {
            continue;
        }
        let mut eff = config.for_monitor(&m.stable_id(), &m.name, m.is_virtual());
        if s.only.is_some() {
            eff.enabled = true;
        }
        if !eff.enabled {
            continue;
        }
        if let Some(d) = old.iter().find(|d| d.connector == m.name) {
            *d.eff.borrow_mut() = eff;
            docks.push(d.clone());
            continue;
        }
        let start_hidden = eff.hide == Hide::Always;
        docks.push(Rc::new(Dock {
            connector: m.name.clone(),
            stable_id: m.stable_id(),
            eff: RefCell::new(eff),
            items: RefCell::default(),
            hidden: Cell::new(start_hidden),
            hover: Cell::new(false),
            holds: Cell::new(0),
            rect: Cell::new(Rect { x: 0, y: 0, w: 0, h: 0 }),
            offset: Cell::new(None),
            slide_ms: Cell::new(0),
            reserve: Cell::new(false),
            timer: RefCell::default(),
            settle_timer: RefCell::default(),
            settle_gen: Cell::new(0),
            settled_ws: Cell::new((m.active_workspace, m.special_workspace)),
            swipe: Cell::new(None),
            swipe_watchdog: RefCell::default(),
            fit_timer: RefCell::default(),
            fitting: Cell::new(false),
            fitted: RefCell::default(),
            checking_pointer: Cell::new(false),
        }));
    }
    for d in old.iter().filter(|d| !docks.iter().any(|k| Rc::ptr_eq(k, d))) {
        cancel_timer(d);
        cancel_settle(d);
    }
    *s.docks.borrow_mut() = docks;
    refresh(s);
}

/// Recompose every dock's items and decide its hiding.
fn refresh(s: &Rc<Shared>) {
    let desktop = s.desktop.borrow().clone();
    for d in s.docks.borrow().clone() {
        let Some(m) = desktop.monitor(&d.connector) else { continue };
        let pins = shown_pins(s, &d.eff.borrow().pinned);
        let order = s.orders.borrow().get(&d.connector).cloned().unwrap_or_default();
        let (items, order) = items::compose(&pins, &desktop, &s.index.borrow(), d.eff.borrow().running, m, &order);
        s.orders.borrow_mut().insert(d.connector.clone(), order);
        // A pinned app that's no longer installed has no tile.
        let items = items.into_iter().filter(|i| !i.pinned || i.running() || launch::info(&i.key).is_some()).collect();
        *d.items.borrow_mut() = items;
        evaluate(s, &d);
    }
    publish(s);
}

fn icon_of(key: &str, class: &str) -> String {
    if let Some(info) = launch::info(key)
        && let Some(i) = info.icon()
    {
        return files::icon_name(&i);
    }
    if class.is_empty() { "application-x-executable".into() } else { class.to_lowercase() }
}

fn item_json(s: &Shared, desktop: &Desktop, m: &Monitor, item: &Item) -> Value {
    let info = launch::info(&item.key);
    let first = item.windows.first().and_then(|a| desktop.window(a));
    let class = first.map(|w| w.class.clone()).unwrap_or_else(|| item.key.trim_start_matches("class:").to_string());
    let name = info.as_ref().map(|i| i.display_name().to_string()).unwrap_or_else(|| class.clone());
    let badge = s.badges.borrow().get(&item.key).cloned().unwrap_or_default();
    let windows: Vec<Value> = item
        .windows
        .iter()
        .filter_map(|a| desktop.window(a))
        .map(|w| {
            let ws = desktop.workspaces.iter().find(|x| x.id == w.workspace);
            json!({
                "address": w.address, "stable_id": w.stable_id, "title": if w.title.is_empty() { &w.class } else { &w.title },
                "class": w.class, "workspace": w.workspace, "workspace_name": ws.map(|x| x.name.clone()).unwrap_or_default(),
                "minimized": ws.is_some_and(|x| x.name == MINIMIZED), "current": w.workspace == m.active_workspace,
                "special": ws.is_some_and(|x| x.is_special()), "focused": desktop.active_window.as_deref() == Some(w.address.as_str()),
            })
        })
        .collect();
    json!({
        "key": item.key, "name": name, "icon": icon_of(&item.key, &class), "pinned": item.pinned,
        "app": !item.key.starts_with("class:"), "windows": windows, "focused": item.focused,
        "urgent": item.urgent || badge.urgent, "badge": badge.count, "progress": badge.progress,
    })
}

fn publish(s: &Rc<Shared>) {
    if s.publish_pending.replace(true) {
        return;
    }
    let ws = Rc::downgrade(s);
    glib::idle_add_local_once(move || {
        let Some(s) = ws.upgrade() else { return };
        s.publish_pending.set(false);
        let desktop = s.desktop.borrow().clone();
        let c = s.config.borrow().clone();
        let monitors: Vec<Value> = s
            .docks
            .borrow()
            .iter()
            .filter_map(|d| {
                let m = desktop.monitor(&d.connector)?;
                let e = d.eff.borrow();
                let items: Vec<Value> = d.items.borrow().iter().map(|i| item_json(&s, &desktop, m, i)).collect();
                Some(json!({
                    "name": d.connector, "stable_id": d.stable_id,
                    "settings": {"icon_size": e.icon_size, "position": format!("{:?}", e.position).to_lowercase(),
                                 "hide": format!("{:?}", e.hide).to_lowercase(), "running": format!("{:?}", e.running).to_lowercase()},
                    "items": items, "hidden": d.hidden.get(), "offset": d.offset.get(), "slide_ms": d.slide_ms.get(),
                    "reserve": d.reserve.get(),
                }))
            })
            .collect();
        let (folders, trash) = files::tiles(&c.folders, c.folder_counts);
        s.link.publish(
            "dock",
            json!({
                "global": {
                    "previews": c.previews, "preview_delay_ms": c.preview_delay_ms, "hide_delay_ms": c.hide_delay_ms,
                    "apps": c.apps, "app_names": c.app_names, "apps_search_bar": c.apps_search_bar, "trash": c.trash, "folder_counts": c.folder_counts,
                    "window_controls": c.window_controls, "click_animation": c.click_animation, "switcher": launch::switcher_installed(),
                    "settings_app": launch::settings_installed(), "store": launch::store_installed(),
                },
                "folders": folders,
                "trash": trash,
                "monitors": monitors,
            }),
        );
    });
}

// ---------- Hiding (the GTK dock's state machine) ----------

fn dock_rect(d: &Dock) -> Rect {
    d.rect.get()
}

fn floating_on(desktop: &Desktop, workspace: i64) -> bool {
    let layout = desktop.workspaces.iter().find(|w| w.id == workspace).map_or("", |w| w.tiled_layout.as_str());
    nyx_desktop::omarchy::workspace_mode(workspace, layout) == nyx_desktop::omarchy::Mode::Floating
}

fn hide_on(d: &Dock, desktop: &Desktop, workspace: i64) -> Hide {
    let mode = d.eff.borrow().hide;
    if mode != Hide::Never {
        return mode;
    }
    mode_here(mode, floating_on(desktop, workspace))
}

fn conceal(s: &Rc<Shared>, d: &Rc<Dock>, ms: u32) {
    d.hidden.set(true);
    d.offset.set(None);
    d.slide_ms.set(ms);
    d.reserve.set(false);
    publish(s);
}

fn reveal(s: &Rc<Shared>, d: &Rc<Dock>, ms: u32) {
    cancel_settle(d);
    d.hidden.set(false);
    d.offset.set(None);
    d.slide_ms.set(ms);
    publish(s);
}

fn cancel_timer(d: &Dock) {
    if let Some(t) = d.timer.borrow_mut().take() {
        t.remove();
    }
}

fn cancel_settle(d: &Dock) {
    d.settle_gen.set(d.settle_gen.get().wrapping_add(1));
    if let Some(t) = d.settle_timer.borrow_mut().take() {
        t.remove();
    }
}

fn evaluate(s: &Rc<Shared>, d: &Rc<Dock>) {
    let desktop = s.desktop.borrow().clone();
    let Some(monitor) = desktop.monitor(&d.connector) else { return };
    let mode = hide_on(d, &desktop, monitor.active_workspace);
    if d.swipe.get().is_some() || d.rect.get().w == 0 {
        return;
    }
    let windows_want = should_hide(mode, &desktop, monitor, dock_rect(d), d.position());
    if windows_want && (d.hover.get() || d.holds.get() > 0) {
        check_stale(s, d);
    }
    let here = (monitor.active_workspace, monitor.special_workspace);
    let switched = d.settled_ws.replace(here) != here;
    debug(|| format!("{}: {:?}, windows want it hidden: {windows_want}, hidden {}", d.connector, mode, d.hidden.get()));
    if windows_want && !d.hover.get() && d.holds.get() == 0 {
        cancel_settle(d);
        if !d.hidden.get() {
            if switched {
                cancel_timer(d);
                conceal(s, d, HIDE_MS);
            } else if d.timer.borrow().is_none() {
                schedule_hide(s, d, s.config.borrow().hide_delay_ms as u64);
            }
        }
    } else {
        cancel_timer(d);
        if !windows_want && d.hidden.get() {
            schedule_settled_show(s, d);
        }
    }
    let keep = mode == Hide::Never && !d.hidden.get();
    if d.reserve.replace(keep) != keep {
        publish(s);
    }
    if keep {
        schedule_fit(d);
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
        let mode = hide_on(&d, &desktop, monitor.active_workspace);
        if d.swipe.get().is_none() && should_hide(mode, &desktop, monitor, dock_rect(&d), d.position()) && !d.hover.get() && d.holds.get() == 0 {
            conceal(&s, &d, HIDE_MS);
        }
    });
    *d.timer.borrow_mut() = Some(id);
}

fn schedule_settled_show(s: &Rc<Shared>, d: &Rc<Dock>) {
    if d.settle_timer.borrow().is_some() {
        return;
    }
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    let generation = d.settle_gen.get();
    let id = glib::timeout_add_local_once(Duration::from_millis(SETTLE_MS), move || {
        let Some(d) = wd.upgrade() else { return };
        d.settle_timer.borrow_mut().take();
        if d.settle_gen.get() != generation {
            return;
        }
        let (connector, rect, position) = (d.connector.clone(), dock_rect(&d), d.position());
        glib::MainContext::default().spawn_local(async move {
            let fresh = gio::spawn_blocking(|| Desktop::snapshot(&nyx_desktop::hypr::Instance::find().ok()?).ok()).await.ok().flatten();
            let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
            if d.settle_gen.get() != generation || d.swipe.get().is_some() || !d.hidden.get() {
                return;
            }
            let desktop = fresh.map(Arc::new).unwrap_or_else(|| s.desktop.borrow().clone());
            let Some(monitor) = desktop.monitor(&connector) else { return };
            let mode = hide_on(&d, &desktop, monitor.active_workspace);
            if should_hide(mode, &desktop, monitor, rect, position) {
                return;
            }
            reveal(&s, &d, SHOW_MS);
            evaluate(&s, &d);
        });
    });
    *d.settle_timer.borrow_mut() = Some(id);
}

fn follow_swipe(s: &Rc<Shared>, event: &SwipeEvent) {
    let desktop = s.desktop.borrow().clone();
    for d in s.docks.borrow().clone() {
        match event {
            SwipeEvent::Moving { monitor, from, to, progress } if *monitor == d.connector => {
                if d.swipe.get().is_none() && (d.hover.get() || d.holds.get() > 0) {
                    continue;
                }
                let Some(m) = desktop.monitor(&d.connector) else { continue };
                let (start, was_hidden) = d.swipe.get().unwrap_or((*from, d.hidden.get()));
                if d.swipe.get().is_none() {
                    d.swipe.set(Some((start, was_hidden)));
                    cancel_timer(&d);
                    cancel_settle(&d);
                }
                let to = resolve_loop(&desktop, monitor, start, *to);
                let to_hidden = should_hide_on(hide_on(&d, &desktop, to), &desktop, m, to, dock_rect(&d), d.position());
                // The plugin reports every frame of every slide; a dock that isn't moving
                // (shown on both workspaces, or hidden on both) has nothing to publish.
                let offset = Some(nyx_dock_core::swipe::offset(was_hidden, to_hidden, *progress));
                if d.offset.replace(offset) != offset {
                    publish(s);
                }
                watch_swipe(s, &d);
            }
            SwipeEvent::Done { monitor, active } if *monitor == d.connector => end_swipe(s, &d, Some(*active)),
            _ => {}
        }
    }
}

fn watch_swipe(s: &Rc<Shared>, d: &Rc<Dock>) {
    if let Some(t) = d.swipe_watchdog.borrow_mut().take() {
        t.remove();
    }
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    let id = glib::timeout_add_local_once(Duration::from_millis(1500), move || {
        if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
            d.swipe_watchdog.borrow_mut().take();
            end_swipe(&s, &d, None);
        }
    });
    *d.swipe_watchdog.borrow_mut() = Some(id);
}

fn end_swipe(s: &Rc<Shared>, d: &Rc<Dock>, active: Option<i64>) {
    let Some((start, _)) = d.swipe.take() else { return };
    if let Some(t) = d.swipe_watchdog.borrow_mut().take() {
        t.remove();
    }
    let desktop = s.desktop.borrow().clone();
    let Some(m) = desktop.monitor(&d.connector) else { return };
    let landed = resolve_loop(&desktop, &d.connector, start, active.unwrap_or(m.active_workspace));
    let mode = hide_on(d, &desktop, landed);
    let hide = should_hide_on(mode, &desktop, m, landed, dock_rect(d), d.position()) && !d.hover.get() && d.holds.get() == 0;
    d.settled_ws.set((landed, m.special_workspace));
    if hide {
        conceal(s, d, SWIPE_SETTLE_MS);
    } else {
        reveal(s, d, SWIPE_SETTLE_MS);
    }
}

/// Hover or holds kept the dock up, but the pointer may have left unheard (a popup
/// closed under it, a drag ended elsewhere): ask Hyprland where it is.
fn check_stale(s: &Rc<Shared>, d: &Rc<Dock>) {
    if d.checking_pointer.replace(true) {
        return;
    }
    let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(d));
    glib::MainContext::default().spawn_local(async move {
        let pos = gio::spawn_blocking(|| {
            let pos = nyx_desktop::hypr::Instance::find().ok()?.json::<Value>("cursorpos").ok()?;
            Some((pos["x"].as_f64()? as i32, pos["y"].as_f64()? as i32))
        })
        .await
        .ok()
        .flatten();
        let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) else { return };
        d.checking_pointer.set(false);
        let desktop = s.desktop.borrow().clone();
        let (Some(m), Some((x, y))) = (desktop.monitor(&d.connector), pos) else { return };
        let r = dock_rect(&d);
        let (x, y) = (x - m.x, y - m.y);
        if x >= r.x - 4 && x < r.x + r.w + 4 && y >= r.y - 4 && y < r.y + r.h + 4 {
            return;
        }
        // Only hover goes on its own; holds are the interface's (its popups say when
        // they close).
        if d.hover.replace(false) {
            debug(|| format!("{}: the pointer left without the dock hearing", d.connector));
            s.link.emit("PointerLeft", json!({"monitor": d.connector}));
            evaluate(&s, &d);
        }
    });
}

fn schedule_fit(d: &Rc<Dock>) {
    if d.fit_timer.borrow().is_some() {
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

/// Floating windows that reach behind a dock that stays up move off it (and get smaller
/// if they must), as the GTK dock's `fit_windows`.
fn fit_windows(d: &Rc<Dock>) {
    if d.hidden.get() || d.fitting.replace(true) {
        return;
    }
    let (connector, position, hide) = (d.connector.clone(), d.position(), d.eff.borrow().hide);
    let r = dock_rect(d);
    let mut fitted = d.fitted.borrow().clone();
    let wd = Rc::downgrade(d);
    glib::MainContext::default().spawn_local(async move {
        let fitted = gio::spawn_blocking(move || {
            use nyx_desktop::actions::Dispatch;
            let Ok(hypr) = nyx_desktop::hypr::Instance::find() else { return fitted };
            let Ok(desktop) = Desktop::snapshot(&hypr) else { return fitted };
            let Some(m) = desktop.monitor(&connector) else { return fitted };
            if mode_here(hide, floating_on(&desktop, m.active_workspace)) != Hide::Never {
                return HashMap::new();
            }
            let (gw, gh) = m.logical_size();
            let dock = Rect { x: m.x + r.x, y: m.y + r.y, w: r.w, h: r.h };
            let [left, top, right, bottom] = m.reserved;
            let area = Rect { x: m.x + left, y: m.y + top, w: gw - left - right, h: gh - top - bottom };
            let windows: Vec<_> = desktop.visible_windows(m).filter(|w| w.floating && w.fullscreen == 0).collect();
            fitted.retain(|a, _| windows.iter().any(|w| w.address == *a));
            for w in windows {
                let Some((at, size)) = clear_of_dock(position, area, dock, w.at, w.size) else {
                    fitted.remove(&w.address);
                    continue;
                };
                if fitted.get(&w.address) == Some(&(w.at, w.size)) {
                    continue;
                }
                fitted.insert(w.address.clone(), (w.at, w.size));
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
            fitted
        })
        .await
        .unwrap_or_default();
        if let Some(d) = wd.upgrade() {
            *d.fitted.borrow_mut() = fitted;
            d.fitting.set(false);
        }
    });
}

// ---------- Operations ----------

fn dock_of(s: &Shared, b: &Value) -> Option<Rc<Dock>> {
    let name = b.get("monitor").and_then(Value::as_str)?;
    s.docks.borrow().iter().find(|d| d.connector == name).cloned()
}

/// A fullscreen window is on this dock's monitor: a dock mapped now would cover it.
fn under_fullscreen(s: &Shared, d: &Dock) -> bool {
    let desktop = s.desktop.borrow().clone();
    desktop.monitor(&d.connector).is_some_and(|m| fullscreen_on(&desktop, m))
}

fn str_of(b: &Value, k: &str) -> String {
    b.get(k).and_then(Value::as_str).unwrap_or_default().to_string()
}

fn activate(s: &Rc<Shared>, d: &Rc<Dock>, key: &str) {
    let desktop = s.desktop.borrow().clone();
    let Some(m) = desktop.monitor(&d.connector) else { return };
    let Some(item) = d.items.borrow().iter().find(|i| i.key == key).cloned() else { return };
    if !item.running() {
        if !item.key.starts_with("class:") {
            launch::launch(&item.key, None);
        }
        return;
    }
    let target = if item.focused { items::next_window(&desktop, &item) } else { items::pick_window(&desktop, &item, m) };
    if let Some(w) = target {
        let minimized = desktop.workspaces.iter().any(|x| x.id == w.workspace && x.name == MINIMIZED);
        launch::focus(&w.address, minimized, m.active_workspace);
    }
}

fn handle(s: &Rc<Shared>, req: nyx_ui_link::Request) {
    let b = req.body.clone();
    let ok = |req: nyx_ui_link::Request| req.ok(json!({}));
    match req.op.as_str() {
        // `nyx-dock --focus`: the dock on the focused monitor takes the keyboard.
        "Focus" => {
            let desktop = s.desktop.borrow().clone();
            let Some(m) = desktop.focused_monitor() else { return req.err("not_found", "no monitor has the focus") };
            s.link.emit("Focus", json!({"monitor": m.name}));
            ok(req)
        }
        "Hover" | "Hold" => {
            let Some(d) = dock_of(s, &b) else { return req.err("not_found", "no dock there") };
            let on = b.get("on").and_then(Value::as_bool).unwrap_or(false);
            if req.op == "Hover" {
                d.hover.set(on);
            } else {
                d.holds.set(if on { d.holds.get() + 1 } else { d.holds.get().saturating_sub(1) });
                debug(|| format!("{}: hold {on}, now {}", d.connector, d.holds.get()));
            }
            if (d.hover.get() || d.holds.get() > 0) && d.hidden.get() && !under_fullscreen(s, &d) {
                reveal(s, &d, SHOW_MS);
            }
            evaluate(s, &d);
            ok(req)
        }
        "Geometry" => {
            let Some(d) = dock_of(s, &b) else { return req.err("not_found", "no dock there") };
            let n = |k: &str| b.get(k).and_then(Value::as_f64).unwrap_or(0.0).round() as i32;
            d.rect.set(Rect { x: n("x"), y: n("y"), w: n("w"), h: n("h") });
            evaluate(s, &d);
            ok(req)
        }
        "Edge" => {
            let Some(d) = dock_of(s, &b) else { return req.err("not_found", "no dock there") };
            if d.hidden.get() && !under_fullscreen(s, &d) {
                reveal(s, &d, SHOW_MS);
                let ms = s.config.borrow().hide_delay_ms.max(1200) as u64;
                let (ws, wd) = (Rc::downgrade(s), Rc::downgrade(&d));
                glib::timeout_add_local_once(Duration::from_millis(ms), move || {
                    if let (Some(s), Some(d)) = (ws.upgrade(), wd.upgrade()) {
                        d.settled_ws.set((0, None));
                        evaluate(&s, &d);
                    }
                });
            }
            ok(req)
        }
        "Activate" => {
            if let Some(d) = dock_of(s, &b) {
                activate(s, &d, &str_of(&b, "key"));
            }
            ok(req)
        }
        "NewInstance" => {
            launch::new_instance(&str_of(&b, "key"));
            ok(req)
        }
        "Launch" => {
            let action = b.get("action").and_then(Value::as_str).filter(|a| !a.is_empty());
            launch::launch(&str_of(&b, "key"), action);
            ok(req)
        }
        "FocusWindow" => {
            let desktop = s.desktop.borrow().clone();
            let a = str_of(&b, "address");
            if let Some(w) = desktop.window(&a) {
                let current = desktop.monitor(&str_of(&b, "monitor")).or(desktop.focused_monitor()).map(|m| m.active_workspace).unwrap_or(1);
                let minimized = desktop.workspaces.iter().any(|x| x.id == w.workspace && x.name == MINIMIZED);
                launch::focus(&w.address, minimized, current);
            }
            ok(req)
        }
        "CloseWindow" => {
            launch::close(&str_of(&b, "address"));
            ok(req)
        }
        "CloseApp" => {
            let desktop = s.desktop.borrow().clone();
            for d in s.docks.borrow().iter() {
                if let Some(i) = d.items.borrow().iter().find(|i| i.key == str_of(&b, "key")) {
                    for a in &i.windows {
                        if desktop.window(a).is_some() {
                            launch::close(a);
                        }
                    }
                    break;
                }
            }
            ok(req)
        }
        "Pin" => {
            let Some(d) = dock_of(s, &b) else { return req.err("not_found", "no dock there") };
            let key = pin_source(s, &d, &str_of(&b, "key"));
            let pinned = b.get("pinned").and_then(Value::as_bool).unwrap_or(true);
            let at = b.get("at").and_then(Value::as_u64).map(|a| a as usize);
            let (sid, name) = (d.stable_id.clone(), d.connector.clone());
            s.config.borrow_mut().set_pinned(&sid, &name, &key, pinned, at);
            save(s);
            rebuild(s);
            ok(req)
        }
        "OpenWith" => {
            let uris: Vec<String> = b.get("uris").and_then(Value::as_array).map(|a| a.iter().filter_map(|u| u.as_str().map(String::from)).collect()).unwrap_or_default();
            launch::open_with(&str_of(&b, "key"), &uris);
            ok(req)
        }
        "OpenDefault" => {
            launch::open_default(&str_of(&b, "uri"));
            ok(req)
        }
        "OpenTrash" => {
            launch::open_trash();
            ok(req)
        }
        "EmptyTrash" => {
            launch::empty_trash();
            ok(req)
        }
        "ShowWindows" => {
            launch::show_windows(&str_of(&b, "key"));
            ok(req)
        }
        "DockSettings" => {
            launch::dock_settings();
            ok(req)
        }
        "Uninstall" => {
            launch::uninstall(&str_of(&b, "id"));
            ok(req)
        }
        "Command" => {
            // Only the apps' own pages the cards open.
            let args: Vec<String> = b.get("args").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            if !matches!(args.first().map(String::as_str), Some("nyx-calendar" | "nyx-music")) || args.iter().skip(1).any(|a| !a.chars().all(|c| c.is_ascii_alphanumeric() || "-_".contains(c))) {
                return req.err("invalid", "not a card command");
            }
            let refs: Vec<&str> = args.iter().map(String::as_str).collect();
            launch::command(&refs);
            ok(req)
        }
        "Set" => {
            let what = str_of(&b, "key");
            let v = b.get("value").cloned().unwrap_or(Value::Null);
            let d = dock_of(s, &b);
            {
                let mut c = s.config.borrow_mut();
                let target = d.as_ref().map(|d| (d.stable_id.clone(), d.connector.clone()));
                match (what.as_str(), target) {
                    ("icon_size", Some((sid, n))) => c.set_icon_size(&sid, &n, v.as_u64().unwrap_or(48).clamp(24, 96) as u32),
                    ("position", Some((sid, n))) => {
                        if let Ok(p) = serde_json::from_value::<Position>(v.clone()) {
                            c.set_position(&sid, &n, p)
                        }
                    }
                    ("hide", Some((sid, n))) => {
                        if let Ok(h) = serde_json::from_value::<Hide>(v.clone()) {
                            c.set_hide(&sid, &n, h)
                        }
                    }
                    ("previews", _) => c.previews = v.as_bool().unwrap_or(c.previews),
                    ("apps", _) => c.apps = v.as_bool().unwrap_or(c.apps),
                    ("app_names", _) => c.app_names = v.as_bool().unwrap_or(c.app_names),
                    ("apps_search_bar", _) => c.apps_search_bar = v.as_bool().unwrap_or(c.apps_search_bar),
                    ("trash", _) => c.trash = v.as_bool().unwrap_or(c.trash),
                    ("click_animation", _) => {
                        if let Ok(a) = serde_json::from_value::<ClickAnimation>(v.clone()) {
                            c.click_animation = a
                        }
                    }
                    _ => return req.err("invalid", format!("can't set {what}")),
                }
            }
            save(s);
            rebuild(s);
            ok(req)
        }
        "Folder" | "AddFolders" | "RemoveFolder" | "SetFolder" | "CopyInto" | "TrashFiles" => {
            let r = files::handle(s, &req.op, &b);
            match r {
                Ok(v) => req.ok(v),
                Err(e) => req.err("invalid", e),
            }
        }
        "Card" => {
            let key = str_of(&b, "key");
            match cards::card(&key) {
                Some(v) => req.ok(v),
                None => req.err("not_found", "no such app"),
            }
        }
        "CardList" => {
            let key = str_of(&b, "key");
            glib::MainContext::default().spawn_local(async move {
                let v = gio::spawn_blocking(move || cards::list(&key)).await.unwrap_or(Value::Null);
                req.ok(v)
            });
        }
        "Player" => req.ok(cards::player(&str_of(&b, "key"))),
        "PlayerCommand" => {
            let m = str_of(&b, "method");
            if !matches!(m.as_str(), "Previous" | "PlayPause" | "Next") {
                return req.err("invalid", "unknown player command");
            }
            cards::player_command(&str_of(&b, "key"), &m);
            ok(req)
        }
        "Apps" => req.ok(cards::apps()),
        "ControlsHover" => {
            controls::refresh();
            ok(req)
        }
        "WindowControl" => match controls::act(&str_of(&b, "action")) {
            Ok(()) => ok(req),
            Err(e) => req.err("invalid", e),
        },
        "Shots" => {
            let ids: Vec<String> = b.get("ids").and_then(Value::as_array).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default();
            let (w, h) = (b.get("width").and_then(Value::as_u64).unwrap_or(340) as u32, b.get("height").and_then(Value::as_u64).unwrap_or(205) as u32);
            let (link, peer) = (s.link.clone(), req.peer.id);
            let each = move |id: &str, at: &str| link.emit_to(peer, "Shot", json!({"id": id, "path": at}));
            files::shots(ids, w.min(1600), h.min(1600), each, move |v| req.ok(v));
        }
        _ => {
            let op = req.op.clone();
            req.err("unknown_op", op)
        }
    }
}

pub fn changed(s: &Rc<Shared>) {
    rebuild(s);
}

fn main() -> std::process::ExitCode {
    if std::env::args().any(|a| a == "-h" || a == "--help") {
        println!("Usage: nyx-dock-backend\n\nThe dock's services for its Quickshell interface (nyx-dock --ui qml). NYX_DOCK_ONLY=OUTPUT keeps it to one output.");
        return std::process::ExitCode::SUCCESS;
    }
    let only = std::env::var("NYX_DOCK_ONLY").ok().filter(|s| !s.is_empty());
    let ctx = glib::MainContext::default();
    let _guard = ctx.acquire().expect("main context");
    let (req_tx, req_rx) = async_channel::unbounded::<nyx_ui_link::Request>();
    let link = match nyx_ui_link::serve(nyx_ui_link::Options::new("nyx-dock"), move |r| {
        let _ = req_tx.send_blocking(r);
    }) {
        Ok(l) => l,
        Err(nyx_ui_link::Error::AlreadyRunning(p)) => {
            eprintln!("nyx-dock-backend: already running at {}", p.display());
            return std::process::ExitCode::SUCCESS;
        }
        Err(e) => {
            eprintln!("nyx-dock-backend: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    let s = Rc::new(Shared {
        link,
        only: only.clone(),
        config: RefCell::new(load_config()),
        desktop: RefCell::new(Arc::new(Desktop::default())),
        index: RefCell::new(nyx_desktop::apps::from_gio()),
        aliases: RefCell::default(),
        badges: RefCell::default(),
        docks: RefCell::default(),
        orders: RefCell::default(),
        saving: Cell::new(false),
        publish_pending: Cell::new(false),
    });
    // The theme snapshot for Quickshell parts, where no theme service runs. A test copy
    // (NYX_DOCK_ONLY) shows its own settings and mustn't publish them for the desktop.
    if only.is_none() {
        nyx_theme_core::notify_changed("system", "shell");
        files::follow_theme();
    }

    let ws = Rc::downgrade(&s);
    std::mem::forget(watch::watch_local(move |u| {
        let Some(s) = ws.upgrade() else { return };
        *s.desktop.borrow_mut() = u.desktop.clone();
        if u.changes == nyx_desktop::model::Changes::ALL && u.desktop.connected && s.only.is_none() {
            swipe::ensure_loaded();
        }
        if u.changes.monitors || s.docks.borrow().is_empty() {
            rebuild(&s);
        } else if u.changes.windows || u.changes.focus || u.changes.workspaces || u.changes.active {
            update_controls(&s);
            refresh(&s);
        }
    }));

    let ws = Rc::downgrade(&s);
    swipe::listen(move |event| {
        if let Some(s) = ws.upgrade() {
            follow_swipe(&s, &event);
        }
    });

    let ws = Rc::downgrade(&s);
    let monitor = gio::AppInfoMonitor::get();
    monitor.connect_changed(move |_| {
        if let Some(s) = ws.upgrade() {
            *s.index.borrow_mut() = nyx_desktop::apps::from_gio();
            s.aliases.borrow_mut().clear();
            refresh(&s);
        }
    });
    std::mem::forget(monitor);

    let mimeapps = glib::user_config_dir().join("mimeapps.list");
    if let Ok(fm) = gio::File::for_path(&mimeapps).monitor_file(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE) {
        let ws = Rc::downgrade(&s);
        let pending: Rc<Cell<bool>> = Rc::default();
        fm.connect_changed(move |_, _, _, _| {
            if pending.replace(true) {
                return;
            }
            let (ws, pending) = (ws.clone(), pending.clone());
            glib::timeout_add_local_once(Duration::from_millis(300), move || {
                pending.set(false);
                let Some(s) = ws.upgrade() else { return };
                s.aliases.borrow_mut().clear();
                refresh(&s);
            });
        });
        std::mem::forget(fm);
    }

    let (tx, rx) = async_channel::unbounded::<()>();
    match nyx_dock_core::config::store().watch(move || {
        let _ = tx.send_blocking(());
    }) {
        Ok(watch) => std::mem::forget(watch),
        Err(e) => eprintln!("nyx-dock: can't watch the settings: {e:#}"),
    }
    let ws = Rc::downgrade(&s);
    ctx.spawn_local(async move {
        while rx.recv().await.is_ok() {
            glib::timeout_future(Duration::from_millis(150)).await;
            while rx.try_recv().is_ok() {}
            let Some(s) = ws.upgrade() else { return };
            if s.saving.get() {
                continue;
            }
            let fresh = load_config();
            if *s.config.borrow() == fresh {
                continue;
            }
            *s.config.borrow_mut() = fresh;
            rebuild(&s);
        }
    });

    let ws = Rc::downgrade(&s);
    if let Some(sub) = badges::watch(move |id, badge| {
        let Some(s) = ws.upgrade() else { return };
        s.badges.borrow_mut().insert(id.to_string(), badge.clone());
        publish(&s);
    }) {
        std::mem::forget(sub);
    }
    files::watch(&s);

    let s2 = s.clone();
    ctx.spawn_local(async move {
        while let Ok(req) = req_rx.recv().await {
            handle(&s2, req);
        }
    });
    glib::MainLoop::new(Some(&ctx), false).run();
    std::process::ExitCode::SUCCESS
}
