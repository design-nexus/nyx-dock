//! What an app's card (its right-click) lists, read only from what apps already
//! publish: Nyx Calendar's agenda, GTK's bookmarks, browsers' bookmarks. The dock draws
//! the card; this decides what goes in it.

use serde_json::Value;
use std::path::{Path, PathBuf};

/// The most rows a card's list shows.
pub const ROWS: usize = 5;

pub const CALENDAR: &str = "io.github.design_nexus.nyx.Calendar";
pub const MUSIC: &str = "io.github.design_nexus.nyx.Music";
pub const FILES: &str = "io.github.design_nexus.nyx.Files";

/// What a card lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// Coming events from Nyx Calendar's agenda.
    Calendar,
    /// Bookmarked and standard folders.
    Files,
    /// The browser's bookmarks bar, else its most visited sites.
    Browser,
    /// Nyx Music's pages (recently played, most played…).
    Music,
    /// Files the app opened lately.
    Recent,
}

/// What a card lists for an app: by its desktop id, then by its categories
/// (`Office;Calendar;`). `file_manager`: it's the default app for folders.
pub fn source(id: &str, categories: &str, file_manager: bool) -> Source {
    let has = |c: &str| categories.split(';').any(|x| x == c);
    match id {
        CALENDAR => Source::Calendar,
        MUSIC => Source::Music,
        FILES => Source::Files,
        _ if file_manager || has("FileManager") => Source::Files,
        _ if has("WebBrowser") => Source::Browser,
        _ => Source::Recent,
    }
}

// ---------- Calendar ----------

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Event {
    pub title: String,
    pub calendar: String,
    /// `#rrggbb`, when the calendar has a colour.
    pub color: Option<String>,
    pub location: String,
    /// Unix seconds.
    pub starts: i64,
    pub ends: i64,
    pub all_day: bool,
}

/// Nyx Calendar's agenda: written by `nyx-calendard` (yesterday to 60 days out), a
/// published read-only contract.
pub fn agenda_path() -> PathBuf {
    data_home().join("nyx-calendar/agenda.sqlite")
}

/// The events not over by `now`, soonest first, at most `max`; the same event in
/// several calendars (holidays) once. `None` when there's no agenda (Calendar hasn't run).
pub fn agenda(path: &Path, now: i64, max: usize) -> Option<Vec<Event>> {
    if !path.exists() {
        return None;
    }
    let db = rusqlite::Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).ok()?;
    let mut stmt = db
        .prepare(
            "SELECT title, calendar_name, color, location, starts, ends, all_day FROM events \
             WHERE ends > ?1 ORDER BY all_day DESC, starts, title LIMIT 40",
        )
        .ok()?;
    let rows = stmt
        .query_map([now], |r| {
            Ok(Event {
                title: r.get(0)?,
                calendar: r.get(1)?,
                color: r.get::<_, Option<String>>(2)?.filter(|c| c.starts_with('#')),
                location: r.get(3)?,
                starts: r.get(4)?,
                ends: r.get(5)?,
                all_day: r.get::<_, i64>(6)? != 0,
            })
        })
        .ok()?;
    let mut events: Vec<Event> = Vec::new();
    for e in rows.flatten() {
        if let Some(prev) = events.iter_mut().find(|p| p.title == e.title && p.starts == e.starts) {
            if prev.color.is_none() {
                prev.color = e.color;
            }
            continue;
        }
        events.push(e);
    }
    events.sort_by(|a, b| a.starts.cmp(&b.starts).then(b.all_day.cmp(&a.all_day)));
    events.truncate(max);
    Some(events)
}

/// When an event is, as Control Center's Up next says it: "in 5 min", "14:00",
/// "Tomorrow 9:00", "Thu 9:00", "All day", or a date. `days` is how many calendar days
/// ahead it starts (0 = today), and the names are its start's, in local time.
pub fn when(e: &Event, now: i64, days: i64, time: &str, weekday: &str, weekday_long: &str, date: &str) -> String {
    if e.all_day {
        return match days {
            ..=0 => "All day".into(),
            1 => "Tomorrow".into(),
            2..=6 => weekday_long.into(),
            _ => date.into(),
        };
    }
    if e.starts <= now {
        return "Now".into();
    }
    let mins = (e.starts - now) / 60;
    if mins < 60 {
        return format!("in {} min", mins.max(1));
    }
    match days {
        0 => time.into(),
        1 => format!("Tomorrow {time}"),
        2..=6 => format!("{weekday} {time}"),
        _ => date.into(),
    }
}

// ---------- Files ----------

/// A folder to open.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
    pub uri: String,
    pub name: String,
}

pub fn gtk_bookmarks_path() -> PathBuf {
    config_home().join("gtk-3.0/bookmarks")
}

/// GTK's bookmarks file: `URI [label]` a line.
pub fn gtk_bookmarks(text: &str) -> Vec<Place> {
    text.lines()
        .filter_map(|l| {
            let l = l.trim();
            if l.is_empty() {
                return None;
            }
            let (uri, label) = match l.split_once(' ') {
                Some((u, n)) => (u, n.trim()),
                None => (l, ""),
            };
            let name = if label.is_empty() { uri_name(uri) } else { label.to_string() };
            Some(Place { uri: uri.to_string(), name })
        })
        .collect()
}

/// The bookmarks, then the standard folders not among them, at most `max`.
pub fn places(bookmarks: Vec<Place>, standard: Vec<Place>, max: usize) -> Vec<Place> {
    let mut out: Vec<Place> = Vec::new();
    for p in bookmarks.into_iter().chain(standard) {
        let same = |a: &str, b: &str| a.trim_end_matches('/') == b.trim_end_matches('/');
        if !out.iter().any(|o| same(&o.uri, &p.uri)) {
            out.push(p);
        }
    }
    out.truncate(max);
    out
}

/// A URI's last part, decoded: `file:///home/k/My%20Stuff` is "My Stuff".
fn uri_name(uri: &str) -> String {
    let last = uri.trim_end_matches('/').rsplit('/').next().unwrap_or(uri);
    percent_decode(last)
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && let Some(b) = s.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

// ---------- Browsers ----------

/// A web page to open.
#[derive(Debug, Clone, PartialEq)]
pub struct Link {
    pub title: String,
    pub url: String,
}

/// Where a browser keeps its profiles.
#[derive(Debug, Clone, PartialEq)]
pub enum Browser {
    /// The Chromium family: its config folder (with `Local State`).
    Chromium(PathBuf),
    /// The Firefox family: the folders that may hold `profiles.ini`, likeliest first.
    Firefox(Vec<PathBuf>),
}

/// Chromium-family browsers by a word in their desktop id, and their folder in
/// `~/.config`.
const CHROMIUM: &[(&str, &str)] = &[
    ("brave", "BraveSoftware/Brave-Browser"),
    ("google-chrome", "google-chrome"),
    ("vivaldi", "vivaldi"),
    ("microsoft-edge", "microsoft-edge"),
    ("chromium", "chromium"),
];

/// Firefox-family browsers, and their folder in the home folder.
const FIREFOX: &[(&str, &str)] = &[("librewolf", ".librewolf"), ("zen", ".zen"), ("firefox", ".mozilla/firefox")];

/// Where the browser with this desktop id keeps its profiles. A Flatpak (a reverse-DNS
/// id) keeps them under `~/.var/app/ID`.
pub fn browser(id: &str, home: &Path, config: &Path) -> Option<Browser> {
    let lower = id.to_lowercase();
    let flatpak = id.split('.').count() >= 3;
    let app = home.join(".var/app").join(id);
    if let Some((_, dir)) = CHROMIUM.iter().find(|(word, _)| lower.contains(word)) {
        let base = if flatpak { app.join("config") } else { config.to_path_buf() };
        return Some(Browser::Chromium(base.join(dir)));
    }
    let (_, dir) = FIREFOX.iter().find(|(word, _)| lower.contains(word))?;
    let mut roots = Vec::new();
    if flatpak {
        roots.push(app.join(dir));
    }
    if *dir == ".mozilla/firefox" {
        // Firefox 147 moved to the XDG config folder.
        roots.push(config.join("mozilla/firefox"));
    }
    roots.push(home.join(dir));
    Some(Browser::Firefox(roots))
}

/// The bookmarks bar, else the most visited sites; at most `max`.
pub fn bookmarks(browser: &Browser, max: usize) -> Vec<Link> {
    match browser {
        Browser::Chromium(dir) => {
            let profile = std::fs::read_to_string(dir.join("Local State"))
                .ok()
                .and_then(|t| chromium_profile(&t))
                .unwrap_or_else(|| "Default".into());
            std::fs::read_to_string(dir.join(profile).join("Bookmarks"))
                .ok()
                .map(|t| chromium_bookmarks(&t, max))
                .unwrap_or_default()
        }
        Browser::Firefox(roots) => roots
            .iter()
            .find_map(|root| {
                let ini = std::fs::read_to_string(root.join("profiles.ini")).ok()?;
                let profile = firefox_profile(&ini, root)?;
                Some(firefox_bookmarks(&profile.join("places.sqlite"), max))
            })
            .unwrap_or_default(),
    }
}

/// The profile a Chromium browser used last, from its `Local State`.
pub fn chromium_profile(local_state: &str) -> Option<String> {
    let v: Value = serde_json::from_str(local_state).ok()?;
    v.pointer("/profile/last_used")?.as_str().filter(|s| !s.is_empty()).map(str::to_string)
}

/// The bookmarks bar from a Chromium `Bookmarks` file: its links, then those in its
/// folders, at most `max`.
pub fn chromium_bookmarks(text: &str, max: usize) -> Vec<Link> {
    let Ok(v) = serde_json::from_str::<Value>(text) else { return Vec::new() };
    let Some(bar) = v.pointer("/roots/bookmark_bar/children").and_then(Value::as_array) else { return Vec::new() };
    let link = |n: &Value| {
        let url = n.get("url")?.as_str()?;
        let title = n.get("name").and_then(Value::as_str).unwrap_or("");
        Some(Link { title: title_or_host(title, url), url: url.into() })
    };
    let mut out: Vec<Link> = bar.iter().filter_map(link).collect();
    for folder in bar.iter().filter(|n| n.get("type").and_then(Value::as_str) == Some("folder")) {
        out.extend(folder.get("children").and_then(Value::as_array).into_iter().flatten().filter_map(link));
    }
    out.truncate(max);
    out
}

/// The profile Firefox opens: this install's default (`[Install…] Default=`), else the
/// profile marked `Default=1`, else the first.
pub fn firefox_profile(ini: &str, root: &Path) -> Option<PathBuf> {
    let mut install: Option<String> = None;
    let mut profiles: Vec<(String, bool, bool)> = Vec::new();
    let mut section = String::new();
    for line in ini.lines().map(str::trim) {
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = name.to_string();
            if section.starts_with("Profile") {
                profiles.push((String::new(), true, false));
            }
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        if section.starts_with("Install") {
            if key == "Default" && install.is_none() {
                install = Some(value.to_string());
            }
        } else if section.starts_with("Profile")
            && let Some(p) = profiles.last_mut()
        {
            match key {
                "Path" => p.0 = value.to_string(),
                "IsRelative" => p.1 = value != "0",
                "Default" => p.2 = value == "1",
                _ => {}
            }
        }
    }
    let at = |path: &str, relative: bool| if relative { root.join(path) } else { PathBuf::from(path) };
    if let Some(path) = install {
        return Some(at(&path, !Path::new(&path).is_absolute()));
    }
    let (path, relative, _) = profiles.iter().find(|p| p.2).or(profiles.first())?;
    (!path.is_empty()).then(|| at(path, *relative))
}

/// The bookmarks toolbar from a Firefox `places.sqlite`, else the most visited sites.
/// Firefox locks the file while it runs, so this reads a copy (with its journal).
pub fn firefox_bookmarks(places: &Path, max: usize) -> Vec<Link> {
    static READS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let n = READS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = std::env::temp_dir().join(format!("nyx-dock-places-{}-{n}", std::process::id()));
    let _ = std::fs::create_dir_all(&dir);
    let copy = dir.join("places.sqlite");
    if std::fs::copy(places, &copy).is_err() {
        return Vec::new();
    }
    let wal = places.with_file_name("places.sqlite-wal");
    let copy_wal = dir.join("places.sqlite-wal");
    if wal.exists() {
        let _ = std::fs::copy(&wal, &copy_wal);
    } else {
        let _ = std::fs::remove_file(&copy_wal);
    }
    let links = read_places(&copy, max).unwrap_or_default();
    let _ = std::fs::remove_dir_all(&dir);
    links
}

fn read_places(path: &Path, max: usize) -> rusqlite::Result<Vec<Link>> {
    let db = rusqlite::Connection::open(path)?;
    let query = |sql: &str| -> rusqlite::Result<Vec<Link>> {
        let mut stmt = db.prepare(sql)?;
        let rows = stmt.query_map([max as i64], |r| {
            let title: Option<String> = r.get(0)?;
            let url: String = r.get(1)?;
            Ok(Link { title: title_or_host(title.as_deref().unwrap_or(""), &url), url })
        })?;
        Ok(rows.flatten().collect())
    };
    let toolbar = query(
        "SELECT COALESCE(NULLIF(b.title, ''), p.title), p.url FROM moz_bookmarks b JOIN moz_places p ON p.id = b.fk \
         WHERE b.type = 1 AND b.parent = (SELECT id FROM moz_bookmarks WHERE guid = 'toolbar_____') \
         ORDER BY b.position LIMIT ?1",
    )?;
    if !toolbar.is_empty() {
        return Ok(toolbar);
    }
    query(
        "SELECT title, url FROM moz_places WHERE hidden = 0 AND url LIKE 'http%' \
         ORDER BY frecency DESC LIMIT ?1",
    )
}

/// A link's title, or its site when it has none (`www.reddit.com` is "reddit.com").
fn title_or_host(title: &str, url: &str) -> String {
    if !title.trim().is_empty() {
        return title.trim().to_string();
    }
    let rest = url.split_once("://").map(|(_, r)| r).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    host.strip_prefix("www.").unwrap_or(host).to_string()
}

// ---------- Music ----------

/// Nyx Music's pages a card opens (`nyx-music --section ID`): id, name, icon.
pub const MUSIC_SECTIONS: &[(&str, &str, &str)] = &[
    ("recently-played", "Recently played", "document-open-recent-symbolic"),
    ("most-played", "Most played", "starred-symbolic"),
    ("recently-added", "Recently added", "list-add-symbolic"),
    ("radio", "Radio", "network-wireless-symbolic"),
    ("podcasts", "Podcasts", "microphone-sensitivity-high-symbolic"),
];

// ---------- Recent files ----------

/// Whether a recent file was opened by this app: one of the names GTK recorded for it
/// (`applications`) is the app's name, its program, or its id (whole or last part).
pub fn opened_by(applications: &[String], name: &str, id: &str, program: &str) -> bool {
    let last = id.rsplit('.').next().unwrap_or(id);
    let ours = [name, id, last, program];
    applications.iter().any(|a| ours.iter().any(|o| !o.is_empty() && a.eq_ignore_ascii_case(o)))
}

fn home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME").filter(|s| !s.is_empty()).map(PathBuf::from).unwrap_or_else(|| home().join(".config"))
}

fn data_home() -> PathBuf {
    std::env::var_os("XDG_DATA_HOME").filter(|s| !s.is_empty()).map(PathBuf::from).unwrap_or_else(|| home().join(".local/share"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nyx-dock-card-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sources() {
        assert_eq!(source(CALENDAR, "", false), Source::Calendar);
        assert_eq!(source(MUSIC, "", false), Source::Music);
        assert_eq!(source("org.gnome.Nautilus", "GTK;Utility;Core;FileManager;", false), Source::Files);
        assert_eq!(source("thunar", "", true), Source::Files, "the default for folders");
        assert_eq!(source("firefox", "Network;WebBrowser;", false), Source::Browser);
        assert_eq!(source("org.gnome.TextEditor", "GNOME;GTK;Utility;TextEditor;", false), Source::Recent);
    }

    #[test]
    fn reads_the_agenda() {
        let dir = scratch("agenda");
        let path = dir.join("agenda.sqlite");
        assert_eq!(agenda(&path, 0, 5), None, "no agenda yet");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE events (calendar TEXT, calendar_name TEXT, color TEXT, uid TEXT, rid TEXT, title TEXT, location TEXT,
                                  description TEXT, url TEXT, starts INTEGER, ends INTEGER, all_day INTEGER, first_day TEXT, last_day TEXT);
             INSERT INTO events VALUES ('a','Holidays',NULL,'1',NULL,'Election Day','','',NULL,1000,87400,1,'','');
             INSERT INTO events VALUES ('b','US Holidays','#8e44ad','2',NULL,'Election Day','','',NULL,1000,87400,1,'','');
             INSERT INTO events VALUES ('c','Work','#3584e4','3',NULL,'Review','Studio B','',NULL,5000,8600,0,'','');
             INSERT INTO events VALUES ('c','Work','#3584e4','4',NULL,'Over','','',NULL,100,200,0,'','');",
        )
        .unwrap();
        drop(db);
        let events = agenda(&path, 1500, 5).unwrap();
        let titles: Vec<&str> = events.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Election Day", "Review"], "one of each holiday, nothing over");
        assert_eq!(events[0].color.as_deref(), Some("#8e44ad"));
        assert_eq!(agenda(&path, 1500, 1).unwrap().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn says_when() {
        let e = |starts: i64, all_day: bool| Event { starts, ends: starts + 3600, all_day, ..Default::default() };
        let now = 100_000;
        assert_eq!(when(&e(now + 300, false), now, 0, "14:00", "Thu", "Thursday", "Nov 5"), "in 5 min");
        assert_eq!(when(&e(now - 60, false), now, 0, "", "", "", ""), "Now");
        assert_eq!(when(&e(now + 7200, false), now, 0, "14:00", "", "", ""), "14:00");
        assert_eq!(when(&e(now + 86_400, false), now, 1, "9:00", "", "", ""), "Tomorrow 9:00");
        assert_eq!(when(&e(now + 3 * 86_400, false), now, 3, "9:00", "Sat", "", ""), "Sat 9:00");
        assert_eq!(when(&e(now, true), now, 0, "", "", "", ""), "All day");
        assert_eq!(when(&e(now + 3 * 86_400, true), now, 3, "", "Sat", "Saturday", ""), "Saturday");
        assert_eq!(when(&e(now + 30 * 86_400, true), now, 30, "", "", "", "Dec 4"), "Dec 4");
    }

    #[test]
    fn folders() {
        let marks = gtk_bookmarks("file:///home/k/Downloads Downloads\nfile:///home/k/My%20Stuff\n\nsftp://box/srv srv\n");
        assert_eq!(marks[1], Place { uri: "file:///home/k/My%20Stuff".into(), name: "My Stuff".into() });
        assert_eq!(marks[2].name, "srv");
        let standard = vec![
            Place { uri: "file:///home/k/Downloads/".into(), name: "Downloads".into() },
            Place { uri: "file:///home/k/Music".into(), name: "Music".into() },
        ];
        let names: Vec<String> = places(marks, standard, 5).into_iter().map(|p| p.name).collect();
        assert_eq!(names, ["Downloads", "My Stuff", "srv", "Music"], "Downloads once");
    }

    #[test]
    fn browsers_by_id() {
        let (home, config) = (Path::new("/h"), Path::new("/h/.config"));
        assert_eq!(browser("chromium", home, config), Some(Browser::Chromium("/h/.config/chromium".into())));
        assert_eq!(
            browser("brave-browser", home, config),
            Some(Browser::Chromium("/h/.config/BraveSoftware/Brave-Browser".into()))
        );
        assert_eq!(
            browser("com.brave.Browser", home, config),
            Some(Browser::Chromium("/h/.var/app/com.brave.Browser/config/BraveSoftware/Brave-Browser".into()))
        );
        assert_eq!(
            browser("firefox", home, config),
            Some(Browser::Firefox(vec!["/h/.config/mozilla/firefox".into(), "/h/.mozilla/firefox".into()]))
        );
        assert_eq!(browser("org.gnome.Epiphany", home, config), None);
    }

    #[test]
    fn chromium() {
        assert_eq!(chromium_profile(r#"{"profile":{"last_used":"Profile 1"}}"#).as_deref(), Some("Profile 1"));
        assert_eq!(chromium_profile("{}"), None);
        let text = r#"{"roots":{"bookmark_bar":{"children":[
            {"type":"url","name":"News","url":"https://news.example/"},
            {"type":"folder","name":"Work","children":[{"type":"url","name":"","url":"https://www.git.example/x"}]},
            {"type":"url","name":"Mail","url":"https://mail.example/"}]}}}"#;
        let links = chromium_bookmarks(text, 5);
        let titles: Vec<&str> = links.iter().map(|l| l.title.as_str()).collect();
        assert_eq!(titles, ["News", "Mail", "git.example"], "the bar's links, then its folders'");
        assert_eq!(chromium_bookmarks(text, 1).len(), 1);
        assert!(chromium_bookmarks("not json", 5).is_empty());
    }

    #[test]
    fn firefox_profiles() {
        let root = Path::new("/r");
        let ini = "[Profile0]\nName=a\nIsRelative=1\nPath=a.default-release\n\n[Install4F96]\nDefault=a.default-release\n\n\
                   [Profile1]\nName=b\nIsRelative=1\nPath=b.default\nDefault=1\n";
        assert_eq!(firefox_profile(ini, root), Some("/r/a.default-release".into()), "the install's default");
        let ini = "[Profile0]\nPath=a\n[Profile1]\nPath=/abs/b\nIsRelative=0\nDefault=1\n";
        assert_eq!(firefox_profile(ini, root), Some("/abs/b".into()));
        assert_eq!(firefox_profile("[General]\nVersion=2\n", root), None);
    }

    #[test]
    fn firefox_places() {
        let dir = scratch("places");
        let path = dir.join("places.sqlite");
        let db = rusqlite::Connection::open(&path).unwrap();
        db.execute_batch(
            "CREATE TABLE moz_places (id INTEGER PRIMARY KEY, url TEXT, title TEXT, hidden INTEGER, frecency INTEGER);
             CREATE TABLE moz_bookmarks (id INTEGER PRIMARY KEY, type INTEGER, fk INTEGER, parent INTEGER, position INTEGER, title TEXT, guid TEXT);
             INSERT INTO moz_places VALUES (1,'https://www.reddit.com/','Reddit',0,50),(2,'https://x.com/home','Home / X',0,90);
             INSERT INTO moz_bookmarks VALUES (10,2,NULL,1,0,'toolbar','toolbar_____');",
        )
        .unwrap();
        let titles = |links: Vec<Link>| links.into_iter().map(|l| l.title).collect::<Vec<_>>();
        assert_eq!(titles(firefox_bookmarks(&path, 5)), ["Home / X", "Reddit"], "most visited without a toolbar");
        db.execute_batch(
            "INSERT INTO moz_places VALUES (3,'https://www.sammobile.com/',NULL,0,1);
             INSERT INTO moz_bookmarks VALUES (11,1,3,10,1,'',NULL),(12,1,1,10,0,'',NULL);",
        )
        .unwrap();
        drop(db);
        assert_eq!(titles(firefox_bookmarks(&path, 5)), ["Reddit", "sammobile.com"], "the toolbar, in order");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn recent_files_by_app() {
        let apps = vec!["gnome-text-editor".to_string(), "Org.Gnome.TextEditor".to_string()];
        assert!(opened_by(&apps, "Text Editor", "org.gnome.TextEditor", "gnome-text-editor"));
        assert!(opened_by(&["Notes".into()], "Notes", "io.github.design_nexus.nyx.Notes", "nyx-notes"));
        assert!(!opened_by(&apps, "Firefox", "firefox", "firefox"));
    }
}
