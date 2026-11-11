//! Folders on the dock, the Trash, window pictures, and the theme snapshot.

use crate::Shared;
use gio::prelude::*;
use nyx_dock_core::config::{Folder, FolderLook, FolderOpen, Sort};
use nyx_dock_core::files::{FileEntry, sorted};
use serde_json::{Value, json};
use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

const ATTRS: &str = "standard::name,standard::display-name,standard::content-type,standard::is-hidden,standard::icon,\
time::modified,thumbnail::path,thumbnail::failed";

/// An icon as the interface asks for it: a theme name, or a file.
pub fn icon_name(icon: &gio::Icon) -> String {
    if let Some(t) = icon.downcast_ref::<gio::ThemedIcon>() {
        return t.names().first().map(|n| n.to_string()).unwrap_or_default();
    }
    if let Some(f) = icon.downcast_ref::<gio::FileIcon>() {
        return f.file().path().map(|p| p.display().to_string()).unwrap_or_default();
    }
    gio::prelude::IconExt::to_string(icon).map(|s| s.to_string()).unwrap_or_default()
}

struct Listed {
    entry: FileEntry,
    path: PathBuf,
    icon: String,
    thumb: Option<PathBuf>,
}

fn list(dir: &Path) -> Vec<Listed> {
    let Ok(children) = gio::File::for_path(dir).enumerate_children(ATTRS, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) else {
        return Vec::new();
    };
    children
        .filter_map(|i| i.ok())
        .map(|info| Listed {
            entry: FileEntry {
                name: info.display_name().to_string(),
                modified: info.modification_date_time().map_or(0, |t| t.to_unix()),
                kind: info.content_type().map(|t| t.to_string()).unwrap_or_default(),
                hidden: info.is_hidden(),
            },
            path: dir.join(info.name()),
            icon: info.icon().map(|i| icon_name(&i)).unwrap_or_else(|| "text-x-generic".into()),
            thumb: info.attribute_byte_string("thumbnail::path").map(|p| PathBuf::from(p.as_str())).filter(|p| p.exists()),
        })
        .collect()
}

fn contents(dir: &Path, sort: Sort) -> Vec<Listed> {
    let mut by_name: std::collections::HashMap<String, Listed> = list(dir).into_iter().map(|l| (l.entry.name.clone(), l)).collect();
    let entries: Vec<FileEntry> = by_name.values().map(|l| l.entry.clone()).collect();
    sorted(entries, sort).into_iter().filter_map(|e| by_name.remove(&e.name)).collect()
}

/// "Just now", "5 min ago", "3 h ago", "Yesterday", "Tue", or a date.
pub fn age(modified: i64) -> String {
    let now = glib::real_time() / 1_000_000;
    let (Ok(then), Ok(today)) = (glib::DateTime::from_unix_local(modified), glib::DateTime::now_local()) else { return String::new() };
    let days = i64::from((today.ymd().0 - then.ymd().0) * 366 + today.day_of_year() - then.day_of_year());
    let f = |fmt: &str| then.format(fmt).map(|s| s.to_string()).unwrap_or_default();
    nyx_dock_core::files::ago(now, modified, days, &f("%a"), &f("%b %-d"))
}

fn entry_json(l: &Listed) -> Value {
    let file = gio::File::for_path(&l.path);
    json!({
        "name": l.entry.name, "uri": file.uri().to_string(), "path": l.path.display().to_string(), "icon": l.icon,
        "thumb": l.thumb.as_ref().map(|p| p.display().to_string()), "kind": l.entry.kind,
        "image": l.entry.kind.starts_with("image/"), "dir": l.entry.kind == "inode/directory", "age": age(l.entry.modified),
    })
}

fn name(v: impl serde::Serialize) -> String {
    serde_json::to_value(v).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_default()
}

/// A folder's tile: where it is, how it shows, its newest files and its count.
pub fn folder_json(index: usize, f: &Folder, counts: bool) -> Value {
    let dir = f.expanded(&glib::home_dir());
    let items = contents(&dir, f.sort);
    let newest: Vec<Value> = items.iter().take(3).map(entry_json).collect();
    // The folder's own icon (Umbra draws Downloads, Games, …), else the generic one.
    let face = f.icon.clone().unwrap_or_else(|| {
        gio::File::for_path(&dir)
            .query_info("standard::icon,metadata::custom-icon-name", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
            .ok()
            .and_then(|i| i.attribute_string("metadata::custom-icon-name").map(|s| s.to_string()).or_else(|| i.icon().map(|i| icon_name(&i))))
            .unwrap_or_else(|| "folder".into())
    });
    json!({
        "index": index, "path": dir.display().to_string(), "uri": gio::File::for_path(&dir).uri().to_string(),
        "name": dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default(),
        "look": name(f.look), "open": name(f.open), "sort": name(f.sort), "icon": face,
        "count": items.len(), "show_count": f.shows_count(counts), "newest": newest,
    })
}

pub fn trash_count() -> u32 {
    gio::File::for_uri("trash:///")
        .query_info("trash::item-count", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
        .map(|i| i.attribute_uint32("trash::item-count"))
        .unwrap_or(0)
}

/// Folder settings and folder counts, when they were read, the tiles and the Trash count.
type Tiles = (Vec<Folder>, bool, std::time::Instant, Vec<Value>, u32);

thread_local! {
    static MONITORS: RefCell<Vec<gio::FileMonitor>> = const { RefCell::new(Vec::new()) };
    /// The folder tiles and the Trash count as last read, for the folder settings they
    /// were read with; a change in a folder or the Trash ([`watch`]) clears them, and
    /// they're read again after a minute for the files' ages ("5 min ago").
    static TILES: RefCell<Option<Tiles>> = const { RefCell::new(None) };
}

/// The folder tiles and the Trash count. Every change on the desktop publishes the
/// dock again; listing the folders and asking for the Trash each time made a focus or
/// workspace change wait on the disk and D-Bus.
pub fn tiles(folders: &[Folder], counts: bool) -> (Vec<Value>, u32) {
    TILES.with(|t| {
        let mut t = t.borrow_mut();
        if let Some((f, c, at, tiles, trash)) = t.as_ref()
            && f == folders
            && *c == counts
            && at.elapsed() < Duration::from_secs(60)
        {
            return (tiles.clone(), *trash);
        }
        let tiles: Vec<Value> = folders.iter().enumerate().map(|(i, f)| folder_json(i, f, counts)).collect();
        let trash = trash_count();
        *t = Some((folders.to_vec(), counts, std::time::Instant::now(), tiles.clone(), trash));
        (tiles, trash)
    })
}

/// Redraw folder tiles and the Trash when they change (debounced).
pub fn watch(s: &Rc<Shared>) {
    let pending = Rc::new(Cell::new(false));
    let mut monitors = Vec::new();
    let mut dirs: Vec<gio::File> = s.config.borrow().folders.iter().map(|f| gio::File::for_path(f.expanded(&glib::home_dir()))).collect();
    dirs.push(gio::File::for_uri("trash:///"));
    for dir in dirs {
        let Ok(m) = dir.monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE) else { continue };
        let (ws, pending) = (Rc::downgrade(s), pending.clone());
        m.connect_changed(move |_, _, _, _| {
            if pending.replace(true) {
                return;
            }
            let (ws, pending) = (ws.clone(), pending.clone());
            glib::timeout_add_local_once(Duration::from_millis(500), move || {
                pending.set(false);
                TILES.with(|t| *t.borrow_mut() = None);
                if let Some(s) = ws.upgrade() {
                    crate::changed(&s);
                }
            });
        });
        monitors.push(m);
    }
    MONITORS.with(|m| *m.borrow_mut() = monitors);
}

fn uris(b: &Value) -> Vec<String> {
    b.get("uris").and_then(Value::as_array).map(|a| a.iter().filter_map(|u| u.as_str().map(String::from)).collect()).unwrap_or_default()
}

pub fn handle(s: &Rc<Shared>, op: &str, b: &Value) -> Result<Value, String> {
    let index = b.get("index").and_then(Value::as_u64).map(|i| i as usize);
    match op {
        "Folder" => {
            let f = s.config.borrow().folders.get(index.ok_or("no folder")?).cloned().ok_or("no such folder")?;
            let sort = b.get("sort").and_then(|v| serde_json::from_value::<Sort>(v.clone()).ok()).unwrap_or(f.sort);
            let dir = f.expanded(&glib::home_dir());
            Ok(Value::Array(contents(&dir, sort).iter().map(entry_json).collect()))
        }
        "AddFolders" => {
            let home = glib::home_dir();
            let at = b.get("at").and_then(Value::as_u64).map(|a| a as usize);
            let mut added = 0;
            for u in uris(b) {
                let file = gio::File::for_uri(&u);
                let Some(path) = file.path().filter(|p| p.is_dir()) else { continue };
                if s.config.borrow_mut().add_folder(&path, &home, at.map(|a| a + added)) {
                    added += 1;
                }
            }
            if added > 0 {
                crate::save(s);
                watch(s);
                crate::changed(s);
            }
            Ok(json!({"added": added}))
        }
        "RemoveFolder" => {
            let i = index.ok_or("no folder")?;
            if i < s.config.borrow().folders.len() {
                s.config.borrow_mut().folders.remove(i);
                crate::save(s);
                watch(s);
                crate::changed(s);
            }
            Ok(json!({}))
        }
        "SetFolder" => {
            let i = index.ok_or("no folder")?;
            {
                let mut c = s.config.borrow_mut();
                let f = c.folders.get_mut(i).ok_or("no such folder")?;
                let v = b.get("value").cloned().unwrap_or(Value::Null);
                match b.get("key").and_then(Value::as_str).unwrap_or("") {
                    "look" => f.look = serde_json::from_value::<FolderLook>(v).map_err(|e| e.to_string())?,
                    "open" => f.open = serde_json::from_value::<FolderOpen>(v).map_err(|e| e.to_string())?,
                    "sort" => f.sort = serde_json::from_value::<Sort>(v).map_err(|e| e.to_string())?,
                    "count" => f.count = v.as_bool(),
                    k => return Err(format!("can't set {k}")),
                }
            }
            crate::save(s);
            crate::changed(s);
            Ok(json!({}))
        }
        "CopyInto" => {
            let dest = gio::File::for_path(b.get("path").and_then(Value::as_str).ok_or("no folder")?);
            for u in uris(b) {
                let src = gio::File::for_uri(&u);
                let Some(name) = src.basename() else { continue };
                let target = dest.child(name);
                src.copy_async(&target, gio::FileCopyFlags::NONE, glib::Priority::DEFAULT, gio::Cancellable::NONE, None, |r| {
                    if let Err(e) = r {
                        eprintln!("nyx-dock: couldn't copy: {e}");
                    }
                });
            }
            Ok(json!({}))
        }
        "TrashFiles" => {
            for u in uris(b) {
                gio::File::for_uri(&u).trash_async(glib::Priority::DEFAULT, gio::Cancellable::NONE, |r| {
                    if let Err(e) = r {
                        eprintln!("nyx-dock: couldn't trash: {e}");
                    }
                });
            }
            Ok(json!({}))
        }
        _ => Err(format!("unknown {op}")),
    }
}

/// Pictures of windows (their last frame, hidden workspaces too), written as PPM files
/// in the runtime folder for the interface's previews; `done` gets `{stable_id: path}`.
/// Each window's picture is also sent on its own as it lands (`each(stable_id, path)`),
/// so the cards fill in one by one as in the GTK dock.
pub fn shots(ids: Vec<String>, max_w: u32, max_h: u32, each: impl Fn(&str, &str) + Send + 'static, done: impl FnOnce(Value) + 'static) {
    let dir = glib::user_runtime_dir().join("nyx-dock/shots");
    glib::MainContext::default().spawn_local(async move {
        let v = gio::spawn_blocking(move || {
            let _ = std::fs::create_dir_all(&dir);
            let out = std::sync::Mutex::new(serde_json::Map::new());
            let _ = nyx_desktop::capture::capture_blocking(&ids, max_w, max_h, |shot| {
                let path = dir.join(format!("{}.ppm", shot.stable_id.replace('/', "_")));
                let mut data = format!("P6\n{} {}\n255\n", shot.width, shot.height).into_bytes();
                data.reserve(shot.width as usize * shot.height as usize * 3);
                for px in shot.data.as_chunks::<4>().0 {
                    data.extend_from_slice(&[px[2], px[1], px[0]]);
                }
                let tmp = path.with_extension("ppm.tmp");
                if std::fs::write(&tmp, &data).and_then(|_| std::fs::rename(&tmp, &path)).is_ok() {
                    let stamp = glib::real_time();
                    let at = format!("{}?{stamp}", path.display());
                    each(&shot.stable_id, &at);
                    out.lock().unwrap().insert(shot.stable_id.clone(), json!(at));
                }
            });
            Value::Object(out.into_inner().unwrap())
        })
        .await
        .unwrap_or(Value::Null);
        done(v);
    });
}

/// Keep the theme snapshot current for Quickshell parts where no theme service runs, as
/// the GTK dock did through `nyx_theme::use_shell_scope()`.
pub fn follow_theme() {
    let pending = Rc::new(Cell::new(false));
    let mut monitors = Vec::new();
    for dir in nyx_theme_core::paths::watched_dirs() {
        let Ok(m) = gio::File::for_path(&dir).monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE) else { continue };
        let pending = pending.clone();
        m.connect_changed(move |_, file, _, _| {
            if file.basename().is_some_and(|b| b.to_string_lossy().ends_with(".tmp")) || pending.replace(true) {
                return;
            }
            let pending = pending.clone();
            glib::timeout_add_local_once(Duration::from_millis(120), move || {
                pending.set(false);
                nyx_theme_core::notify_changed("system", "shell");
            });
        });
        monitors.push(m);
    }
    std::mem::forget(monitors);
}
