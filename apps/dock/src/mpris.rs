//! The media panel at the top of a player's menu: what's playing and the controls,
//! through MPRIS (`org.mpris.MediaPlayer2.*` on the session bus), which Nyx Music,
//! Media Player, browsers and most players publish. The panel lives only while its menu
//! is open: it listens for the player's changes and reads the position once a second.

use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Playing {
    pub title: String,
    pub artist: String,
    pub art: Option<String>,
    pub length_us: i64,
    pub position_us: i64,
    pub playing: bool,
    pub can_previous: bool,
    pub can_next: bool,
}

fn bus() -> Option<gio::DBusConnection> {
    gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()
}

fn call(
    conn: &gio::DBusConnection,
    dest: &str,
    iface: &str,
    method: &str,
    args: Option<&glib::Variant>,
) -> Option<glib::Variant> {
    conn.call_sync(Some(dest), PATH, iface, method, args, None, gio::DBusCallFlags::NONE, 300, gio::Cancellable::NONE).ok()
}

fn get(conn: &gio::DBusConnection, dest: &str, iface: &str, prop: &str) -> Option<glib::Variant> {
    let reply = conn
        .call_sync(
            Some(dest),
            PATH,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&(iface, prop).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            300,
            gio::Cancellable::NONE,
        )
        .ok()?;
    reply.child_value(0).as_variant()
}

/// Whether a player's `DesktopEntry` (or bus name) belongs to the dock item `key`.
pub fn matches(key: &str, desktop_entry: &str, bus_name: &str) -> bool {
    let key = key.strip_prefix("class:").unwrap_or(key).to_lowercase();
    let entry = desktop_entry.trim_end_matches(".desktop").to_lowercase();
    let short = key.rsplit('.').next().unwrap_or(&key);
    let name = bus_name.strip_prefix("org.mpris.MediaPlayer2.").unwrap_or(bus_name).to_lowercase();
    let name = name.split(".instance").next().unwrap_or(&name);
    (!entry.is_empty() && (entry == key || entry == short || entry.rsplit('.').next() == Some(short)))
        || name == key
        || name == short
}

/// The bus name of the app's player, if it has one.
pub fn find(key: &str) -> Option<String> {
    let conn = bus()?;
    let names = conn
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "ListNames",
            None,
            None,
            gio::DBusCallFlags::NONE,
            300,
            gio::Cancellable::NONE,
        )
        .ok()?
        .child_value(0)
        .get::<Vec<String>>()?;
    names.into_iter().filter(|n| n.starts_with("org.mpris.MediaPlayer2.")).find(|n| {
        let entry = get(&conn, n, "org.mpris.MediaPlayer2", "DesktopEntry").and_then(|v| v.get::<String>()).unwrap_or_default();
        matches(key, &entry, n)
    })
}

fn read(conn: &gio::DBusConnection, dest: &str) -> Option<Playing> {
    let all = call(conn, dest, "org.freedesktop.DBus.Properties", "GetAll", Some(&(PLAYER,).to_variant()))?;
    let props = glib::VariantDict::new(Some(&all.child_value(0)));
    let mut p = Playing {
        playing: props.lookup::<String>("PlaybackStatus").ok().flatten().as_deref() == Some("Playing"),
        can_previous: props.lookup::<bool>("CanGoPrevious").ok().flatten().unwrap_or(true),
        can_next: props.lookup::<bool>("CanGoNext").ok().flatten().unwrap_or(true),
        position_us: props.lookup::<i64>("Position").ok().flatten().unwrap_or(0),
        ..Default::default()
    };
    if let Some(meta) = props.lookup_value("Metadata", None) {
        let meta = glib::VariantDict::new(Some(&meta));
        p.title = meta.lookup::<String>("xesam:title").ok().flatten().unwrap_or_default();
        p.artist = meta.lookup::<Vec<String>>("xesam:artist").ok().flatten().unwrap_or_default().join(", ");
        p.art = meta.lookup::<String>("mpris:artUrl").ok().flatten().filter(|u| u.starts_with("file://"));
        p.length_us = meta
            .lookup_value("mpris:length", None)
            .and_then(|v| v.get::<i64>().or_else(|| v.get::<u64>().map(|u| u as i64)))
            .unwrap_or(0);
    }
    Some(p)
}

fn clock(us: i64) -> String {
    let s = (us / 1_000_000).max(0);
    format!("{}:{:02}", s / 60, s % 60)
}

/// The panel for the app's player, or `None` when it has no player.
pub fn panel(key: &str) -> Option<gtk::Widget> {
    let dest = find(key)?;
    let conn = bus()?;
    let now = read(&conn, &dest)?;

    let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
    root.add_css_class("quick-media");
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let art = gtk::Image::new();
    art.set_pixel_size(48);
    art.add_css_class("quick-art");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_valign(gtk::Align::Center);
    text.set_hexpand(true);
    let title = gtk::Label::new(None);
    title.add_css_class("quick-title");
    let artist = gtk::Label::new(None);
    artist.add_css_class("quick-artist");
    for l in [&title, &artist] {
        l.set_xalign(0.0);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        l.set_max_width_chars(24);
        text.append(l);
    }
    head.append(&art);
    head.append(&text);
    root.append(&head);

    let progress = gtk::ProgressBar::new();
    progress.add_css_class("quick-progress");
    root.append(&progress);
    let times = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    times.add_css_class("quick-times");
    let (pos, len) = (gtk::Label::new(None), gtk::Label::new(None));
    pos.set_hexpand(true);
    pos.set_xalign(0.0);
    times.append(&pos);
    times.append(&len);
    root.append(&times);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::Center);
    let button = |icon: &str, tip: &str, method: &'static str| {
        let b = gtk::Button::from_icon_name(icon);
        b.set_tooltip_text(Some(tip));
        b.add_css_class("flat");
        let dest = dest.clone();
        b.connect_clicked(move |_| {
            if let Some(conn) = bus() {
                conn.call(
                    Some(&dest),
                    PATH,
                    PLAYER,
                    method,
                    None,
                    None,
                    gio::DBusCallFlags::NONE,
                    1000,
                    gio::Cancellable::NONE,
                    |_| {},
                );
            }
        });
        buttons.append(&b);
        b
    };
    let prev = button("media-skip-backward-symbolic", "Previous", "Previous");
    let play = button("media-playback-start-symbolic", "Play", "PlayPause");
    play.add_css_class("quick-play");
    let next = button("media-skip-forward-symbolic", "Next", "Next");
    root.append(&buttons);

    let shown: Rc<RefCell<Playing>> = Rc::new(RefCell::new(Playing::default()));
    let show = {
        let (art, title, artist, progress, pos, len, prev, play, next, shown) = (
            art.clone(),
            title.clone(),
            artist.clone(),
            progress.clone(),
            pos.clone(),
            len.clone(),
            prev.clone(),
            play.clone(),
            next.clone(),
            shown.clone(),
        );
        move |p: Playing| {
            let old = shown.replace(p.clone());
            if old.art != p.art || old.title.is_empty() {
                match p.art.as_deref().map(gio::File::for_uri) {
                    Some(file) => art.set_from_gicon(&gio::FileIcon::new(&file)),
                    None => art.set_icon_name(Some("audio-x-generic")),
                }
            }
            title.set_text(if p.title.is_empty() { "Nothing playing" } else { &p.title });
            artist.set_text(&p.artist);
            artist.set_visible(!p.artist.is_empty());
            progress.set_fraction(if p.length_us > 0 {
                (p.position_us as f64 / p.length_us as f64).clamp(0.0, 1.0)
            } else {
                0.0
            });
            pos.set_text(&clock(p.position_us));
            len.set_text(&if p.length_us > 0 { clock(p.length_us) } else { String::new() });
            prev.set_sensitive(p.can_previous);
            next.set_sensitive(p.can_next);
            play.set_icon_name(if p.playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
            play.set_tooltip_text(Some(if p.playing { "Pause" } else { "Play" }));
        }
    };
    show(now);

    // Track changes while the panel exists; the position isn't signalled, so read it
    // once a second (only while the menu is open).
    let show = Rc::new(show);
    let (s2, d2) = (show.clone(), dest.clone());
    let sub = conn.subscribe_to_signal(
        Some(&dest),
        Some("org.freedesktop.DBus.Properties"),
        Some("PropertiesChanged"),
        Some(PATH),
        None,
        gio::DBusSignalFlags::NONE,
        move |_| {
            if let Some(p) = bus().and_then(|c| read(&c, &d2)) {
                s2(p);
            }
        },
    );
    let (s3, d3, weak) = (show.clone(), dest.clone(), root.downgrade());
    let tick = glib::timeout_add_local(Duration::from_secs(1), move || {
        if weak.upgrade().is_none() {
            return glib::ControlFlow::Break;
        }
        if let Some(p) = bus().and_then(|c| read(&c, &d3)) {
            s3(p);
        }
        glib::ControlFlow::Continue
    });
    let keep = RefCell::new(Some((sub, tick)));
    root.connect_destroy(move |_| {
        if let Some((sub, tick)) = keep.borrow_mut().take() {
            drop(sub);
            tick.remove();
        }
    });
    Some(root.upcast())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn players_match_their_apps() {
        assert!(matches(
            "io.github.design_nexus.nyx.Music",
            "io.github.design_nexus.nyx.Music",
            "org.mpris.MediaPlayer2.nyx-music"
        ));
        assert!(matches("firefox", "firefox", "org.mpris.MediaPlayer2.firefox.instance_1_42"));
        assert!(matches("org.mozilla.firefox", "firefox", "org.mpris.MediaPlayer2.firefox.instance_1_42"));
        assert!(matches("spotify", "", "org.mpris.MediaPlayer2.spotify"), "by bus name when there's no DesktopEntry");
        assert!(!matches(
            "io.github.design_nexus.nyx.Files",
            "io.github.design_nexus.nyx.Music",
            "org.mpris.MediaPlayer2.nyx-music"
        ));
    }

    #[test]
    fn clock_format() {
        assert_eq!(clock(84_000_000), "1:24");
        assert_eq!(clock(3_605_000_000), "60:05");
    }
}
