//! An app's card (right-click on its icon): what it is, its actions, the list that
//! fits it (Calendar's next events, a file manager's folders, a browser's bookmarks,
//! Music's library, the files an app opened lately), its player, and the Apps panel's
//! list. The data the GTK dock's `card.rs` gathered, through nyx-dock-core's `card`.

use crate::launch;
use gio::prelude::*;
use nyx_dock_core::card::{self, Source};
use serde_json::{Value, json};

/// The card's header, actions and what the list shows.
pub fn card(key: &str) -> Option<Value> {
    let info = launch::info(key);
    if info.is_none() && !key.starts_with("class:") {
        return None;
    }
    let name = info.as_ref().map(|i| i.display_name().to_string()).unwrap_or_else(|| key.trim_start_matches("class:").to_string());
    let generic = info.as_ref().and_then(|i| i.generic_name()).map(|g| g.to_string()).filter(|g| *g != name);
    let actions: Vec<Value> = info
        .as_ref()
        .map(|i| i.list_actions().iter().map(|a| json!({"id": a.as_str(), "name": i.action_name(a).to_string()})).collect())
        .unwrap_or_default();
    let own_new = info.as_ref().is_some_and(|i| i.list_actions().iter().any(|a| matches!(a.as_str(), "new-window" | "new-empty-window" | "NewWindow")));
    let source = info.as_ref().map(|i| {
        let categories = i.categories().map(|c| c.to_string()).unwrap_or_default();
        let file_manager = gio::AppInfo::default_for_type("inode/directory", false).and_then(|a| a.id()).is_some_and(|id| id.trim_end_matches(".desktop") == key);
        card::source(key, &categories, file_manager)
    });
    let kind = match source {
        Some(Source::Calendar) => "calendar",
        Some(Source::Files) => "files",
        Some(Source::Browser) => "browser",
        Some(Source::Music) => "music",
        Some(Source::Recent) => "recent",
        None => "",
    };
    Some(json!({
        "key": key, "name": name, "generic": generic, "icon": info.as_ref().and_then(|i| i.icon()).map(|i| crate::files::icon_name(&i)).unwrap_or_else(|| "application-x-executable".into()),
        "actions": actions, "own_new_window": own_new, "list": kind, "is_app": info.is_some(),
        "calendar_app": launch::installed("nyx-calendar"), "music_app": launch::installed("nyx-music"),
    }))
}

fn tilde(path: &std::path::Path) -> String {
    let home = glib::home_dir();
    match path.strip_prefix(&home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn when(e: &card::Event, now: i64) -> String {
    let local = |t: i64| glib::DateTime::from_unix_local(t).ok();
    let (Some(start), Some(today)) = (local(e.starts), local(now)) else { return String::new() };
    let days = i64::from((start.ymd().0 - today.ymd().0) * 366 + start.day_of_year() - today.day_of_year());
    let f = |fmt: &str| start.format(fmt).map(|s| s.to_string()).unwrap_or_default();
    card::when(e, now, days, &f("%H:%M"), &f("%a"), &f("%A"), &f("%b %-d"))
}

/// The list in the card, by what the app is (read from what the apps already keep).
pub fn list(key: &str) -> Value {
    let Some(c) = card(key) else { return Value::Null };
    match c["list"].as_str().unwrap_or("") {
        "calendar" => {
            let now = glib::real_time() / 1_000_000;
            match card::agenda(&card::agenda_path(), now, card::ROWS) {
                None => json!({"title": "Up next", "note": "Open Calendar once to see events here", "rows": []}),
                Some(events) if events.is_empty() => json!({"title": "Up next", "note": "Nothing coming up", "rows": []}),
                Some(events) => {
                    let rows: Vec<Value> = events
                        .iter()
                        .map(|e| {
                            let when = when(e, now);
                            let day = glib::DateTime::from_unix_local(e.starts).ok().and_then(|d| d.format("%Y-%m-%d").ok()).map(|s| s.to_string());
                            json!({"title": e.title, "sub": if e.location.is_empty() { when } else { format!("{when} · {}", e.location) },
                                   "icon": "x-office-calendar-symbolic", "color": e.color, "calendar_day": day})
                        })
                        .collect();
                    json!({"title": "Up next", "rows": rows, "chips": [["Today", "day"], ["Agenda", "agenda"]]})
                }
            }
        }
        "files" => {
            let marks = std::fs::read_to_string(card::gtk_bookmarks_path()).map(|t| card::gtk_bookmarks(&t)).unwrap_or_default();
            let home = glib::home_dir();
            let mut standard = vec![card::Place { uri: gio::File::for_path(&home).uri().to_string(), name: "Home".into() }];
            for dir in [glib::UserDirectory::Documents, glib::UserDirectory::Downloads, glib::UserDirectory::Pictures, glib::UserDirectory::Music, glib::UserDirectory::Videos] {
                if let Some(path) = glib::user_special_dir(dir).filter(|p| *p != home) {
                    let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
                    standard.push(card::Place { uri: gio::File::for_path(&path).uri().to_string(), name });
                }
            }
            let rows: Vec<Value> = card::places(marks, standard, card::ROWS)
                .into_iter()
                .map(|p| {
                    let sub = gio::File::for_uri(&p.uri).path().map(|p| tilde(&p)).unwrap_or_else(|| p.uri.clone());
                    json!({"title": p.name, "sub": sub, "icon": "folder-symbolic", "open": p.uri})
                })
                .collect();
            json!({"title": "Folders", "rows": rows})
        }
        "browser" => {
            let Some(which) = card::browser(key, &glib::home_dir(), &glib::user_config_dir()) else { return json!({"rows": []}) };
            let rows: Vec<Value> = card::bookmarks(&which, card::ROWS)
                .into_iter()
                .map(|l| {
                    let host = l.url.split_once("://").map(|(_, r)| r).unwrap_or(&l.url);
                    let host = host.split(['/', '?', '#']).next().unwrap_or(host).trim_start_matches("www.");
                    json!({"title": l.title, "sub": if host == l.title { "" } else { host }, "icon": "starred-symbolic", "open": l.url})
                })
                .collect();
            json!({"title": "Favorites", "rows": rows})
        }
        "music" if c["music_app"].as_bool() == Some(true) => {
            let rows: Vec<Value> = card::MUSIC_SECTIONS.iter().map(|(page, name, icon)| json!({"title": name, "sub": "", "icon": icon, "music_page": page})).collect();
            json!({"title": "Library", "rows": rows})
        }
        "recent" => {
            let Some(info) = launch::info(key) else { return json!({"rows": []}) };
            let name = info.name().to_string();
            let program = info.executable().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let mut items: Vec<(i64, Value)> = recent_files()
                .into_iter()
                .filter(|r| card::opened_by(&r.apps, &name, key, &program))
                .filter(|r| !r.uri.starts_with("file://") || gio::File::for_uri(&r.uri).query_exists(gio::Cancellable::NONE))
                .map(|r| {
                    let title = gio::File::for_uri(&r.uri).basename().map(|b| b.to_string_lossy().to_string()).unwrap_or_else(|| r.uri.clone());
                    (r.modified, json!({"title": title, "sub": crate::files::age(r.modified), "icon": "text-x-generic-symbolic", "open": r.uri}))
                })
                .collect();
            items.sort_by_key(|(m, _)| std::cmp::Reverse(*m));
            json!({"title": "Recent", "rows": items.into_iter().take(card::ROWS).map(|(_, v)| v).collect::<Vec<_>>()})
        }
        _ => json!({"rows": []}),
    }
}

struct Recent {
    uri: String,
    modified: i64,
    apps: Vec<String>,
}

/// The recently-used store (`recently-used.xbel`) that GTK's RecentManager keeps.
fn recent_files() -> Vec<Recent> {
    let Ok(text) = std::fs::read_to_string(glib::user_data_dir().join("recently-used.xbel")) else { return vec![] };
    let attr = |tag: &str, name: &str| -> Option<String> {
        let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        let end = tag[start..].find('"')? + start;
        Some(tag[start..end].replace("&amp;", "&").replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&apos;", "'"))
    };
    let mut out = Vec::new();
    for chunk in text.split("<bookmark ").skip(1) {
        let head = chunk.split('>').next().unwrap_or("");
        let Some(uri) = attr(head, "href") else { continue };
        let modified = attr(head, "modified").and_then(|m| glib::DateTime::from_iso8601(&m, None).ok()).map(|d| d.to_unix()).unwrap_or(0);
        let body = chunk.split("</bookmark>").next().unwrap_or("");
        let apps = body.split("<bookmark:application ").skip(1).filter_map(|a| attr(a.split('>').next().unwrap_or(""), "name")).collect();
        out.push(Recent { uri, modified, apps });
    }
    out
}

// ---------- Players (MPRIS) ----------

const PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

fn bus() -> Option<gio::DBusConnection> {
    gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).ok()
}

fn get(conn: &gio::DBusConnection, dest: &str, iface: &str, prop: &str) -> Option<glib::Variant> {
    let reply = conn
        .call_sync(Some(dest), PATH, "org.freedesktop.DBus.Properties", "Get", Some(&(iface, prop).to_variant()), None, gio::DBusCallFlags::NONE, 300, gio::Cancellable::NONE)
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
    (!entry.is_empty() && (entry == key || entry == short || entry.rsplit('.').next() == Some(short))) || name == key || name == short
}

fn find(conn: &gio::DBusConnection, key: &str) -> Option<String> {
    let names = conn
        .call_sync(Some("org.freedesktop.DBus"), "/org/freedesktop/DBus", "org.freedesktop.DBus", "ListNames", None, None, gio::DBusCallFlags::NONE, 300, gio::Cancellable::NONE)
        .ok()?
        .child_value(0)
        .get::<Vec<String>>()?;
    names.into_iter().filter(|n| n.starts_with("org.mpris.MediaPlayer2.")).find(|n| {
        let entry = get(conn, n, "org.mpris.MediaPlayer2", "DesktopEntry").and_then(|v| v.get::<String>()).unwrap_or_default();
        matches(key, &entry, n)
    })
}

/// What the app's player plays, or null.
pub fn player(key: &str) -> Value {
    let Some(conn) = bus() else { return Value::Null };
    let Some(dest) = find(&conn, key) else { return Value::Null };
    let Ok(all) = conn.call_sync(Some(&dest), PATH, "org.freedesktop.DBus.Properties", "GetAll", Some(&(PLAYER,).to_variant()), None, gio::DBusCallFlags::NONE, 300, gio::Cancellable::NONE) else {
        return Value::Null;
    };
    let props = glib::VariantDict::new(Some(&all.child_value(0)));
    let mut v = json!({
        "playing": props.lookup::<String>("PlaybackStatus").ok().flatten().as_deref() == Some("Playing"),
        "can_previous": props.lookup::<bool>("CanGoPrevious").ok().flatten().unwrap_or(true),
        "can_next": props.lookup::<bool>("CanGoNext").ok().flatten().unwrap_or(true),
        "position_us": props.lookup::<i64>("Position").ok().flatten().unwrap_or(0),
        "title": "", "artist": "", "art": null, "length_us": 0,
    });
    if let Some(meta) = props.lookup_value("Metadata", None) {
        let meta = glib::VariantDict::new(Some(&meta));
        v["title"] = json!(meta.lookup::<String>("xesam:title").ok().flatten().unwrap_or_default());
        v["artist"] = json!(meta.lookup::<Vec<String>>("xesam:artist").ok().flatten().unwrap_or_default().join(", "));
        v["art"] = json!(meta.lookup::<String>("mpris:artUrl").ok().flatten().filter(|u| u.starts_with("file://")));
        v["length_us"] = json!(meta.lookup_value("mpris:length", None).and_then(|x| x.get::<i64>().or_else(|| x.get::<u64>().map(|u| u as i64))).unwrap_or(0));
    }
    v
}

pub fn player_command(key: &str, method: &str) {
    let Some(conn) = bus() else { return };
    if let Some(dest) = find(&conn, key) {
        conn.call(Some(&dest), PATH, PLAYER, method, None, None, gio::DBusCallFlags::NONE, 1000, gio::Cancellable::NONE, |_| {});
    }
}

/// Every app from the app menu, for the Apps panel.
pub fn apps() -> Value {
    let mut all: Vec<(String, Value)> = gio::AppInfo::all()
        .into_iter()
        .filter(|a| a.should_show())
        .filter_map(|a| {
            let id = a.id()?.trim_end_matches(".desktop").to_string();
            let name = a.display_name().to_string();
            let d = launch::info(&id);
            let words = [name.clone(), id.clone(), d.as_ref().and_then(|d| d.generic_name()).map(|g| g.to_string()).unwrap_or_default(), d.as_ref().map(|d| d.keywords().iter().map(|k| k.to_string()).collect::<Vec<_>>().join(" ")).unwrap_or_default()]
                .join(" ")
                .to_lowercase();
            let icon = a.icon().map(|i| crate::files::icon_name(&i)).unwrap_or_else(|| "application-x-executable".into());
            Some((name.to_lowercase(), json!({"id": id, "name": name, "icon": icon, "words": words})))
        })
        .collect();
    all.sort_by(|a, b| a.0.cmp(&b.0));
    all.dedup_by(|a, b| a.1["id"] == b.1["id"]);
    Value::Array(all.into_iter().map(|(_, v)| v).collect())
}
