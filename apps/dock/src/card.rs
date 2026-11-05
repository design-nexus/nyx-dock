//! An app's card, its right-click: what the app has for you (Calendar's coming events,
//! the file manager's folders, the browser's favorites, Music's pages, or the files the
//! app opened lately), its windows and actions, and keeping it in the dock.

use crate::launch;
use gtk::prelude::*;
use gtk::{gio, glib};
use nyx_dock_core::card::{self, Source};
use std::rc::Rc;

/// What a card needs from its dock.
pub struct Host {
    pub side: gtk::PositionType,
    /// The card opened: keep the dock up and take the keyboard.
    pub opened: Box<dyn Fn()>,
    /// The card closed: let them go.
    pub closed: Box<dyn Fn()>,
    /// Keep the app in the dock (`true`) or not.
    pub pin: Box<dyn Fn(bool)>,
    /// Bring a window (by address) to the front.
    pub focus: Box<dyn Fn(&str)>,
}

/// The app a card is for.
pub struct App {
    /// Desktop id, or `class:<class>` for a window no app claims.
    pub key: String,
    pub pinned: bool,
    /// Its windows, address and title, most recently focused first.
    pub windows: Vec<(String, String)>,
}

type Close = Rc<dyn Fn()>;

pub fn show(tile: &gtk::Box, app: App, host: Host) {
    let host = Rc::new(host);
    let is_app = !app.key.starts_with("class:");
    let info = if is_app { launch::info(&app.key) } else { None };
    let running = !app.windows.is_empty();

    let pop = gtk::Popover::new();
    pop.add_css_class("dock-menu");
    pop.add_css_class("dock-card");
    let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
    col.add_css_class("card");
    pop.set_child(Some(&col));
    let close: Close = {
        let pw = pop.downgrade();
        Rc::new(move || {
            if let Some(p) = pw.upgrade() {
                p.popdown();
            }
        })
    };

    col.append(&header(&app, info.as_ref()));
    if running && let Some(media) = crate::mpris::panel(&app.key) {
        media.add_css_class("card-media");
        col.append(&media);
    }
    if let Some(info) = &info {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.add_css_class("card-list");
        col.append(&list);
        fill(&list, &app.key, info, close.clone());
    }
    if app.windows.len() > 1 {
        let section = section("Windows");
        for (address, title) in &app.windows {
            let (h, a, c) = (host.clone(), address.clone(), close.clone());
            section.append(&row(None, "window-symbolic", title, "", move || {
                c();
                (h.focus)(&a);
            }));
        }
        col.append(&section);
    }
    if let Some(info) = &info {
        let actions = actions(&app.key, info, running, close.clone());
        if actions.first_child().is_some() {
            col.append(&actions);
        }
    }
    col.append(&footer(&app, is_app, host.clone(), close));

    pop.set_parent(tile);
    pop.set_position(host.side);
    pop.set_autohide(!crate::dock::testing());
    (host.opened)();
    let h = host.clone();
    pop.connect_closed(move |p| {
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
        (h.closed)();
    });
    pop.popup();
}

fn header(app: &App, info: Option<&gio_unix::DesktopAppInfo>) -> gtk::Box {
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    head.add_css_class("card-header");
    let image = gtk::Image::new();
    image.set_pixel_size(36);
    match info.and_then(|i| i.icon()) {
        Some(icon) => image.set_from_gicon(&icon),
        None => image.set_icon_name(Some("application-x-executable")),
    }
    head.append(&image);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
    text.set_valign(gtk::Align::Center);
    let name = info.map(|i| i.display_name().to_string()).unwrap_or_else(|| app.key.trim_start_matches("class:").to_string());
    let title = label(&name, "card-name");
    text.append(&title);
    let status = match app.windows.len() {
        0 => "Not running".to_string(),
        1 => "1 window".to_string(),
        n => format!("{n} windows"),
    };
    let generic = info.and_then(|i| i.generic_name()).map(|g| g.to_string()).filter(|g| *g != name);
    let line = match generic {
        Some(g) => format!("{g} · {status}"),
        None => status,
    };
    text.append(&label(&line, "card-status"));
    head.append(&text);
    head
}

/// The app's list, by what it is.
fn fill(list: &gtk::Box, key: &str, info: &gio_unix::DesktopAppInfo, close: Close) {
    let categories = info.categories().map(|c| c.to_string()).unwrap_or_default();
    let file_manager = gio::AppInfo::default_for_type("inode/directory", false)
        .and_then(|a| a.id())
        .is_some_and(|id| id.trim_end_matches(".desktop") == key);
    match card::source(key, &categories, file_manager) {
        Source::Calendar => calendar(list, key, close),
        Source::Files => folders(list, key, close),
        Source::Browser => browser(list, key, close),
        Source::Music => music(list, close),
        Source::Recent => recent(list, key, info, close),
    }
}

// ---------- Lists ----------

fn calendar(list: &gtk::Box, key: &str, close: Close) {
    let section = section("Up next");
    list.append(&section);
    let shortcuts = [("Today", "day"), ("Agenda", "agenda")].map(|(name, page)| {
        let (k, c) = (key.to_string(), close.clone());
        chip(name, move || {
            c();
            calendar_open(&k, &["--section", page]);
        })
    });
    list.append(&chip_rows(shortcuts.to_vec()));
    let (key, section) = (key.to_string(), section.clone());
    glib::spawn_future_local(async move {
        let now = glib::real_time() / 1_000_000;
        let events = gio::spawn_blocking(move || card::agenda(&card::agenda_path(), now, card::ROWS)).await.ok().flatten();
        match events {
            None => section.append(&note("Open Calendar once to see events here")),
            Some(events) if events.is_empty() => section.append(&note("Nothing coming up")),
            Some(events) => {
                for e in events {
                    let start = glib::DateTime::from_unix_local(e.starts).ok();
                    let day = start.as_ref().and_then(|d| d.format("%Y-%m-%d").ok()).map(|s| s.to_string());
                    let when = when(&e, now);
                    let sub = if e.location.is_empty() { when } else { format!("{when} · {}", e.location) };
                    let (k, c) = (key.clone(), close.clone());
                    section.append(&row(e.color.as_deref(), "x-office-calendar-symbolic", &e.title, &sub, move || {
                        c();
                        match &day {
                            Some(day) => calendar_open(&k, &["--section", "day", "--date", day]),
                            None => launch::launch(&k, None),
                        }
                    }));
                }
            }
        }
    });
}

/// Open Calendar on a page, or just open it when its command isn't on the path.
fn calendar_open(key: &str, args: &[&str]) {
    if launch::installed("nyx-calendar") {
        let mut all = vec!["nyx-calendar"];
        all.extend_from_slice(args);
        launch::command(&all);
    } else {
        launch::launch(key, None);
    }
}

fn when(e: &card::Event, now: i64) -> String {
    let local = |t: i64| glib::DateTime::from_unix_local(t).ok();
    let (Some(start), Some(today)) = (local(e.starts), local(now)) else { return String::new() };
    let days = i64::from((start.ymd().0 - today.ymd().0) * 366 + start.day_of_year() - today.day_of_year());
    let f = |fmt: &str| start.format(fmt).map(|s| s.to_string()).unwrap_or_default();
    card::when(e, now, days, &f("%H:%M"), &f("%a"), &f("%A"), &f("%b %-d"))
}

fn folders(list: &gtk::Box, key: &str, close: Close) {
    let marks = std::fs::read_to_string(card::gtk_bookmarks_path()).map(|t| card::gtk_bookmarks(&t)).unwrap_or_default();
    let home = glib::home_dir();
    let mut standard = vec![card::Place { uri: gio::File::for_path(&home).uri().to_string(), name: "Home".into() }];
    for dir in [
        glib::UserDirectory::Documents,
        glib::UserDirectory::Downloads,
        glib::UserDirectory::Pictures,
        glib::UserDirectory::Music,
        glib::UserDirectory::Videos,
    ] {
        if let Some(path) = glib::user_special_dir(dir).filter(|p| *p != home) {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            standard.push(card::Place { uri: gio::File::for_path(&path).uri().to_string(), name });
        }
    }
    let places = card::places(marks, standard, card::ROWS);
    if places.is_empty() {
        return;
    }
    let section = section("Folders");
    for p in places {
        let file = gio::File::for_uri(&p.uri);
        let sub = file.path().map(|p| tilde(&p)).unwrap_or_else(|| p.uri.clone());
        let (k, c, uri) = (key.to_string(), close.clone(), p.uri.clone());
        section.append(&row(None, "folder-symbolic", &p.name, &sub, move || {
            c();
            launch::open_with(&k, std::slice::from_ref(&uri));
        }));
    }
    list.append(&section);
}

fn browser(list: &gtk::Box, key: &str, close: Close) {
    let home = glib::home_dir();
    let Some(which) = card::browser(key, &home, &glib::user_config_dir()) else { return };
    let section = section("Favorites");
    section.set_visible(false);
    list.append(&section);
    let key = key.to_string();
    glib::spawn_future_local(async move {
        let links = gio::spawn_blocking(move || card::bookmarks(&which, card::ROWS)).await.unwrap_or_default();
        for l in links {
            let host = l.url.split_once("://").map(|(_, r)| r).unwrap_or(&l.url);
            let host = host.split(['/', '?', '#']).next().unwrap_or(host).trim_start_matches("www.");
            let sub = if host == l.title { "" } else { host };
            let (k, c, url) = (key.clone(), close.clone(), l.url.clone());
            section.append(&row(None, "starred-symbolic", &l.title, sub, move || {
                c();
                launch::open_with(&k, std::slice::from_ref(&url));
            }));
            section.set_visible(true);
        }
    });
}

fn music(list: &gtk::Box, close: Close) {
    if !launch::installed("nyx-music") {
        return;
    }
    let section = section("Library");
    for (page, name, icon) in card::MUSIC_SECTIONS {
        let c = close.clone();
        section.append(&row(None, icon, name, "", move || {
            c();
            launch::command(&["nyx-music", "--section", page]);
        }));
    }
    list.append(&section);
}

fn recent(list: &gtk::Box, key: &str, info: &gio_unix::DesktopAppInfo, close: Close) {
    let name = info.name().to_string();
    let program = info.executable().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let mut items: Vec<gtk::RecentInfo> = gtk::RecentManager::default()
        .items()
        .into_iter()
        .filter(|r| !r.is_local() || r.exists())
        .filter(|r| {
            let apps: Vec<String> = r.applications().iter().map(|a| a.to_string()).collect();
            card::opened_by(&apps, &name, key, &program)
        })
        .collect();
    if items.is_empty() {
        return;
    }
    items.sort_by_key(|r| std::cmp::Reverse(r.modified().to_unix()));
    let section = section("Recent");
    for r in items.into_iter().take(card::ROWS) {
        let uri = r.uri().to_string();
        let sub = crate::stacks::age(r.modified().to_unix());
        let title = r.display_name().to_string();
        let icon = r.gicon();
        let (k, c) = (key.to_string(), close.clone());
        let button = row(None, "text-x-generic-symbolic", &title, &sub, move || {
            c();
            launch::open_with(&k, std::slice::from_ref(&uri));
        });
        if let (Some(icon), Some(image)) = (icon, find_image(&button)) {
            image.set_from_gicon(&icon);
        }
        section.append(&button);
    }
    list.append(&section);
}

// ---------- Actions and footer ----------

/// The app's own actions (New Window, Compose…), and ours to open it or a new window.
fn actions(key: &str, info: &gio_unix::DesktopAppInfo, running: bool, close: Close) -> gtk::Box {
    let mut chips = Vec::new();
    let own_new = info.list_actions().iter().any(|a| matches!(a.as_str(), "new-window" | "new-empty-window" | "NewWindow"));
    if !(running && own_new) {
        let (k, c) = (key.to_string(), close.clone());
        chips.push(chip(if running { "New window" } else { "Open" }, move || {
            c();
            if running { launch::new_instance(&k) } else { launch::launch(&k, None) }
        }));
    }
    for a in info.list_actions() {
        let name = info.action_name(&a).to_string();
        let (k, c, a) = (key.to_string(), close.clone(), a.to_string());
        chips.push(chip(&name, move || {
            c();
            launch::launch(&k, Some(&a));
        }));
    }
    chip_rows(chips)
}

/// Buttons, three to a line.
fn chip_rows(chips: Vec<gtk::Button>) -> gtk::Box {
    let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    rows.add_css_class("card-chips");
    for line in chips.chunks(3) {
        let h = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        line.iter().for_each(|c| h.append(c));
        rows.append(&h);
    }
    rows
}

fn footer(app: &App, is_app: bool, host: Rc<Host>, close: Close) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    bar.add_css_class("card-footer");
    if is_app {
        let (h, c, pinned) = (host.clone(), close.clone(), app.pinned);
        bar.append(&text_button(if pinned { "Remove from Dock" } else { "Keep in Dock" }, move || {
            c();
            (h.pin)(!pinned);
        }));
    }
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);
    if !app.windows.is_empty() && launch::switcher_installed() {
        let (k, c) = (app.key.clone(), close.clone());
        bar.append(&icon_button("view-grid-symbolic", "Show windows", move || {
            c();
            launch::show_windows(&k);
        }));
    }
    if !app.windows.is_empty() {
        let windows: Vec<String> = app.windows.iter().map(|(a, _)| a.clone()).collect();
        let label = if windows.len() > 1 { "Close all" } else { "Quit" };
        let quit = text_button(label, move || {
            close();
            windows.iter().for_each(|w| launch::close(w));
        });
        quit.add_css_class("card-quit");
        bar.append(&quit);
    }
    bar
}

// ---------- Pieces ----------

fn label(text: &str, class: &str) -> gtk::Label {
    let l = gtk::Label::new(Some(text));
    l.add_css_class(class);
    l.set_xalign(0.0);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    l.set_max_width_chars(32);
    l
}

fn section(title: &str) -> gtk::Box {
    let b = gtk::Box::new(gtk::Orientation::Vertical, 0);
    b.add_css_class("card-section");
    b.append(&label(title, "card-section-title"));
    b
}

fn note(text: &str) -> gtk::Label {
    let l = label(text, "card-note");
    l.set_wrap(true);
    l
}

/// A row that does something: an icon (or a calendar's colour), a title, and a line
/// under it.
fn row(color: Option<&str>, icon: &str, title: &str, sub: &str, act: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::new();
    b.add_css_class("flat");
    b.add_css_class("card-row");
    let h = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    match color.and_then(|c| gtk::gdk::RGBA::parse(c).ok()) {
        Some(rgba) => h.append(&dot(rgba)),
        None => {
            let image = gtk::Image::from_icon_name(icon);
            image.set_pixel_size(16);
            image.add_css_class("card-row-icon");
            h.append(&image);
        }
    }
    let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
    text.set_valign(gtk::Align::Center);
    text.append(&label(title, "card-row-title"));
    if !sub.is_empty() {
        text.append(&label(sub, "card-row-sub"));
    }
    h.append(&text);
    b.set_child(Some(&h));
    b.set_tooltip_text(Some(title));
    b.connect_clicked(move |_| act());
    b
}

fn find_image(button: &gtk::Button) -> Option<gtk::Image> {
    button.child()?.first_child()?.downcast::<gtk::Image>().ok()
}

/// A calendar's colour.
fn dot(rgba: gtk::gdk::RGBA) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(16);
    area.set_content_height(16);
    area.set_valign(gtk::Align::Center);
    area.set_draw_func(move |_, cr, w, h| {
        cr.set_source_rgba(rgba.red() as f64, rgba.green() as f64, rgba.blue() as f64, rgba.alpha() as f64);
        cr.arc(w as f64 / 2.0, h as f64 / 2.0, 4.5, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
    });
    area
}

fn chip(text: &str, act: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class("card-chip");
    b.connect_clicked(move |_| act());
    b
}

fn text_button(text: &str, act: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::with_label(text);
    b.add_css_class("flat");
    b.add_css_class("card-button");
    b.connect_clicked(move |_| act());
    b
}

fn icon_button(icon: &str, tip: &str, act: impl Fn() + 'static) -> gtk::Button {
    let b = gtk::Button::from_icon_name(icon);
    b.add_css_class("flat");
    b.add_css_class("card-button");
    b.set_tooltip_text(Some(tip));
    b.update_property(&[gtk::accessible::Property::Label(tip)]);
    b.connect_clicked(move |_| act());
    b
}

/// A path with the home folder as `~`.
fn tilde(path: &std::path::Path) -> String {
    let home = glib::home_dir();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
