//! Folders and the Trash at the end of the dock.
//!
//! A folder shows as its newest files stacked (or its folder icon) with a count, and
//! opens as a grid, a fan, a list, or straight in the file manager. Thumbnails come from
//! the freedesktop thumbnail cache; an image without one is decoded off the main thread
//! at thumbnail size and written to the cache for next time. Folders are shortcuts:
//! removing one from the dock never touches the folder.

use crate::launch;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use nyx_dock_core::config::{Folder, FolderLook, FolderOpen, Sort};
use nyx_dock_core::files::{FileEntry, ago, sorted};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// What a folder or Trash tile needs from its dock.
pub struct Host {
    /// Keep the dock up (`true`) or let it go (`false`): menus, popovers, drags.
    pub hold: Box<dyn Fn(bool)>,
    /// Remove this folder from the dock.
    pub remove: Box<dyn Fn()>,
    /// Remember this folder's new settings (look, view, sort).
    pub save: Box<dyn Fn(&Folder)>,
    pub side: gtk::PositionType,
    pub icon_size: i32,
    /// The tallest a folder view grows before it scrolls.
    pub max_height: i32,
}

/// Change a folder's settings: saved, and its tile redrawn.
type SetFolder = Rc<dyn Fn(&dyn Fn(&mut Folder))>;

const ATTRS: &str = "standard::name,standard::display-name,standard::content-type,standard::is-hidden,standard::icon,\
time::modified,thumbnail::path,thumbnail::failed";

struct Listed {
    entry: FileEntry,
    file: gio::File,
    icon: Option<gio::Icon>,
    thumb: Option<PathBuf>,
}

fn list(dir: &Path) -> Vec<Listed> {
    let Ok(children) = gio::File::for_path(dir).enumerate_children(ATTRS, gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
    else {
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
            file: gio::File::for_path(dir.join(info.name())),
            icon: info.icon(),
            thumb: info.attribute_byte_string("thumbnail::path").map(|p| PathBuf::from(p.as_str())),
        })
        .collect()
}

fn contents(dir: &Path, sort: Sort) -> Vec<Listed> {
    let mut all = list(dir);
    let order: Vec<String> = sorted(all.iter().map(|l| l.entry.clone()).collect(), sort).into_iter().map(|e| e.name).collect();
    all.retain(|l| order.contains(&l.entry.name));
    all.sort_by_key(|l| order.iter().position(|n| *n == l.entry.name));
    all
}

/// A small picture of a file: its cached thumbnail, a fresh one for an image, else its
/// type's icon.
fn file_picture(l: &Listed, size: i32) -> gtk::Widget {
    sized_picture(l, size, size * 7 / 8)
}

fn sized_picture(l: &Listed, w: i32, h: i32) -> gtk::Widget {
    if let Some(thumb) = l.thumb.as_ref().filter(|p| p.exists()) {
        let pic = gtk::Picture::for_filename(thumb);
        pic.set_content_fit(gtk::ContentFit::Cover);
        pic.set_can_shrink(true);
        return framed(pic.upcast(), w, h);
    }
    let image = gtk::Image::new();
    image.set_pixel_size(w.min(h) * 2 / 3);
    match &l.icon {
        Some(icon) => image.set_from_gicon(icon),
        None => image.set_icon_name(Some("text-x-generic")),
    }
    let holder = framed(image.clone().upcast(), w, h);
    if l.entry.kind.starts_with("image/") {
        let (file, holder_weak) = (l.file.clone(), holder.downgrade());
        glib::MainContext::default().spawn_local(async move {
            let Some(texture) = thumbnail(&file).await else { return };
            let Some(holder) = holder_weak.upgrade().and_downcast::<gtk::Overlay>() else { return };
            let pic = gtk::Picture::for_paintable(&texture);
            pic.set_content_fit(gtk::ContentFit::Cover);
            pic.set_can_shrink(true);
            holder.add_overlay(&pic);
        });
    }
    holder
}

/// A fixed-size frame the picture fills (pictures otherwise ask for their pixel size).
fn framed(child: gtk::Widget, w: i32, h: i32) -> gtk::Widget {
    let frame = gtk::Overlay::new();
    frame.add_css_class("stack-thumb");
    frame.set_overflow(gtk::Overflow::Hidden);
    let base = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    base.set_size_request(w, h);
    frame.set_child(Some(&base));
    child.set_halign(gtk::Align::Fill);
    child.set_valign(gtk::Align::Fill);
    if child.is::<gtk::Image>() {
        child.set_halign(gtk::Align::Center);
        child.set_valign(gtk::Align::Center);
    }
    frame.add_overlay(&child);
    frame.upcast()
}

/// Decode an image at thumbnail size off the main thread, and save it to the
/// freedesktop cache (`~/.cache/thumbnails/normal/<md5 of the URI>.png`).
async fn thumbnail(file: &gio::File) -> Option<gdk::Texture> {
    let stream = file.read_future(glib::Priority::LOW).await.ok()?;
    let pixbuf = gtk::gdk_pixbuf::Pixbuf::from_stream_at_scale_future(&stream, 128, 128, true).await.ok()?;
    let uri = file.uri();
    let mtime = file
        .query_info_future("time::modified", gio::FileQueryInfoFlags::NONE, glib::Priority::LOW)
        .await
        .ok()
        .and_then(|i| i.modification_date_time())
        .map(|t| t.to_unix());
    if let Some(mtime) = mtime {
        let md5 = glib::compute_checksum_for_data(glib::ChecksumType::Md5, uri.as_bytes())?;
        let dir = glib::user_cache_dir().join("thumbnails/normal");
        if std::fs::create_dir_all(&dir).is_ok() {
            let path = dir.join(format!("{md5}.png"));
            let tmp = dir.join(format!("{md5}.nyx-dock.tmp"));
            let opts = [("tEXt::Thumb::URI", uri.as_str()), ("tEXt::Thumb::MTime", &mtime.to_string())];
            if pixbuf.savev(&tmp, "png", &opts).is_ok() {
                let _ = std::fs::rename(&tmp, &path);
            }
        }
    }
    Some(gdk::Texture::for_pixbuf(&pixbuf))
}

fn age(modified: i64) -> String {
    let Ok(now) = glib::DateTime::now_local() else { return String::new() };
    let Ok(then) = glib::DateTime::from_unix_local(modified) else { return String::new() };
    let today = now.ymd();
    let days = glib::DateTime::from_local(today.0, today.1, today.2, 0, 0, 0.0)
        .ok()
        .map(|midnight| ((midnight.to_unix() - modified) as f64 / 86_400.0).ceil().max(0.0) as i64)
        .unwrap_or(0);
    let weekday = then.format("%a").map(|s| s.to_string()).unwrap_or_default();
    let date = then.format("%b %-d").map(|s| s.to_string()).unwrap_or_default();
    ago(now.to_unix(), modified, days, &weekday, &date)
}

// ---------- Folder tiles ----------

pub fn folder_tile(folder: &Folder, host: Rc<Host>) -> gtk::Widget {
    let dir = folder.expanded(&glib::home_dir());
    let folder = Rc::new(RefCell::new(folder.clone()));
    let tile = gtk::Box::new(gtk::Orientation::Vertical, 2);
    tile.add_css_class("dock-item");
    tile.add_css_class("dock-folder");
    let face = gtk::Overlay::new();
    tile.append(&face);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.add_css_class("dock-dots");
    tile.append(&spacer);
    let name = gio::File::for_path(&dir)
        .query_info("standard::display-name", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
        .map(|i| i.display_name().to_string())
        .unwrap_or_else(|_| dir.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());

    let draw: Rc<dyn Fn()> = {
        let (face, folder, dir, host, tile_w, name) =
            (face.clone(), folder.clone(), dir.clone(), host.clone(), tile.downgrade(), name.clone());
        Rc::new(move || {
            let Some(tile) = tile_w.upgrade() else { return };
            let folder = folder.borrow().clone();
            clear_overlay(&face);
            let items = contents(&dir, folder.sort);
            let size = host.icon_size;
            if folder.look == FolderLook::Stack && !items.is_empty() {
                let base = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                base.set_size_request(size, size);
                face.set_child(Some(&base));
                // Portrait cards nearly the tile's size, so the stack reads as big as an app icon.
                for (i, l) in items.iter().take(3).enumerate().collect::<Vec<_>>().into_iter().rev() {
                    let card = sized_picture(l, size * 3 / 4, size * 7 / 8);
                    card.add_css_class("stack-card");
                    card.add_css_class(&format!("stack-card-{i}"));
                    card.set_halign(gtk::Align::Center);
                    card.set_valign(gtk::Align::Center);
                    face.add_overlay(&card);
                }
            } else {
                face.set_child(Some(&folder_face(folder.icon.as_deref(), size)));
            }
            if !items.is_empty() {
                let count = gtk::Label::new(Some(&if items.len() > 99 { "99+".into() } else { items.len().to_string() }));
                count.add_css_class("stack-count");
                count.set_halign(gtk::Align::End);
                count.set_valign(gtk::Align::End);
                face.add_overlay(&count);
            }
            tile.set_tooltip_text(Some(&format!("{name} · {} items", items.len())));
        })
    };
    draw();
    let set: SetFolder = {
        let (folder, host, draw) = (folder.clone(), host.clone(), draw.clone());
        Rc::new(move |change: &dyn Fn(&mut Folder)| {
            change(&mut folder.borrow_mut());
            (host.save)(&folder.borrow());
            draw();
        })
    };

    // Redraw when the folder changes (debounced: a download writes many times).
    if let Ok(monitor) = gio::File::for_path(&dir).monitor_directory(gio::FileMonitorFlags::WATCH_MOVES, gio::Cancellable::NONE) {
        let pending: Rc<std::cell::Cell<bool>> = Rc::default();
        let draw = draw.clone();
        monitor.connect_changed(move |_, _, _, _| {
            if pending.replace(true) {
                return;
            }
            let (pending, draw) = (pending.clone(), draw.clone());
            glib::timeout_add_local_once(std::time::Duration::from_millis(500), move || {
                pending.set(false);
                draw();
            });
        });
        // The monitor lives as long as the tile.
        unsafe { tile.set_data("nyx-dock-monitor", monitor) };
    }

    let click = gtk::GestureClick::new();
    click.set_button(0);
    let (folder_c, dir_c, host_c, tile_w, name_c, set_c) =
        (folder.clone(), dir.clone(), host.clone(), tile.downgrade(), name.clone(), set.clone());
    click.connect_released(move |g, _, _, _| {
        let Some(tile) = tile_w.upgrade() else { return };
        let uri = gio::File::for_path(&dir_c).uri();
        let folder = folder_c.borrow().clone();
        match g.current_button() {
            1 => match folder.open {
                FolderOpen::Files => launch::open_default(&uri),
                view => open_view(&tile, &folder, &dir_c, &name_c, view, host_c.clone(), set_c.clone()),
            },
            3 => folder_menu(&tile, &uri, &folder, set_c.clone(), host_c.clone()),
            _ => {}
        }
    });
    tile.add_controller(click);

    // A test hook (NYX_DOCK_TEST_FOLDER): opens the view as a click would, or the
    // folder's menu with NYX_DOCK_TEST_FOLDER=menu.
    if let Some(test) = std::env::var_os("NYX_DOCK_TEST_FOLDER") {
        let actions = gio::SimpleActionGroup::new();
        let open = gio::SimpleAction::new("open", None);
        let (folder_t, dir_t, host_t, tile_t, name_t, set_t) =
            (folder.clone(), dir.clone(), host.clone(), tile.downgrade(), name.clone(), set.clone());
        open.connect_activate(move |_, _| {
            let Some(tile) = tile_t.upgrade() else { return };
            let folder = folder_t.borrow().clone();
            if test == "menu" {
                let uri = gio::File::for_path(&dir_t).uri();
                folder_menu(&tile, &uri, &folder, set_t.clone(), host_t.clone());
                return;
            }
            let view = if folder.open == FolderOpen::Files { FolderOpen::Grid } else { folder.open };
            open_view(&tile, &folder, &dir_t, &name_t, view, host_t.clone(), set_t.clone());
        });
        actions.add_action(&open);
        tile.insert_action_group("folder-test", Some(&actions));
    }

    // Drag the folder out: into an app it's the folder; let go elsewhere, it leaves the dock.
    let drag = gtk::DragSource::new();
    drag.set_actions(gdk::DragAction::COPY | gdk::DragAction::LINK);
    let dir_g = dir.clone();
    drag.connect_prepare(move |_, _, _| {
        let file = gio::File::for_path(&dir_g);
        Some(gdk::ContentProvider::for_value(&gdk::FileList::from_array(std::slice::from_ref(&file)).to_value()))
    });
    let h = host.clone();
    drag.connect_drag_begin(move |_, _| (h.hold)(true));
    let h = host.clone();
    drag.connect_drag_end(move |_, _, _| (h.hold)(false));
    let h = host.clone();
    drag.connect_drag_cancel(move |_, _, reason| {
        if reason == gdk::DragCancelReason::NoTarget {
            (h.remove)();
            return true;
        }
        false
    });
    tile.add_controller(drag);

    // Files dropped on the folder are copied into it.
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::COPY);
    let dir_d = dir.clone();
    drop.connect_drop(move |_, value, _, _| {
        let Ok(list) = value.get::<gdk::FileList>() else { return false };
        for f in list.files() {
            let Some(name) = f.basename() else { continue };
            let dest = gio::File::for_path(dir_d.join(name));
            f.copy_async(&dest, gio::FileCopyFlags::NONE, glib::Priority::DEFAULT, gio::Cancellable::NONE, None, |r| {
                if let Err(e) = r {
                    eprintln!("nyx-dock: couldn't copy into the folder: {e}");
                }
            });
        }
        true
    });
    hold_while_dragging(&drop, &tile, host.clone());
    tile.add_controller(drop);
    tile.upcast()
}

/// A folder as a 2.0 tile: the satin square the Trash uses, with the folder glyph (or
/// the folder's own icon name, if the settings give one).
fn folder_face(icon: Option<&str>, size: i32) -> gtk::Widget {
    let satin = gtk::Box::new(gtk::Orientation::Vertical, 0);
    satin.add_css_class("dock-satin");
    satin.set_size_request(size, size);
    satin.set_halign(gtk::Align::Center);
    let glyph_size = (size / 2).max(16);
    let glyph: gtk::Widget = match icon {
        Some(name) => {
            let image = gtk::Image::from_icon_name(name);
            image.set_pixel_size(glyph_size);
            image.upcast()
        }
        None => folder_glyph(glyph_size).upcast(),
    };
    glyph.set_vexpand(true);
    glyph.set_halign(gtk::Align::Center);
    glyph.set_valign(gtk::Align::Center);
    satin.append(&glyph);
    satin.upcast()
}

/// The 2.0 folder glyph (24 px grid, 1.6 px round strokes), drawn in the tile's colour.
fn folder_glyph(size: i32) -> gtk::DrawingArea {
    use std::f64::consts::{FRAC_PI_2, PI};
    let area = gtk::DrawingArea::new();
    area.set_content_width(size);
    area.set_content_height(size);
    area.set_draw_func(|area, cr, w, h| {
        let scale = w.min(h) as f64 / 24.0;
        cr.scale(scale, scale);
        let c = area.color();
        cr.set_source_rgba(c.red() as f64, c.green() as f64, c.blue() as f64, c.alpha() as f64);
        cr.set_line_width(1.6);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        cr.set_line_join(gtk::cairo::LineJoin::Round);
        cr.move_to(3.0, 7.5);
        cr.arc(5.5, 7.5, 2.5, PI, PI + FRAC_PI_2);
        cr.line_to(9.0, 5.0);
        cr.line_to(11.0, 7.2);
        cr.line_to(18.5, 7.2);
        cr.arc(18.5, 9.7, 2.5, -FRAC_PI_2, 0.0);
        cr.line_to(21.0, 17.5);
        cr.arc(18.5, 17.5, 2.5, 0.0, FRAC_PI_2);
        cr.line_to(5.5, 20.0);
        cr.arc(5.5, 17.5, 2.5, FRAC_PI_2, PI);
        cr.close_path();
        let _ = cr.stroke();
    });
    area
}

fn clear_overlay(o: &gtk::Overlay) {
    let main = o.child();
    let mut c = o.first_child();
    while let Some(w) = c {
        c = w.next_sibling();
        if Some(&w) != main.as_ref() {
            o.remove_overlay(&w);
        }
    }
    o.set_child(gtk::Widget::NONE);
}

/// Highlight a drop target and keep the dock up while something is dragged over it.
pub fn hold_while_dragging(drop: &gtk::DropTarget, tile: &gtk::Box, host: Rc<Host>) {
    let (h, t) = (host.clone(), tile.downgrade());
    drop.connect_enter(move |_, _, _| {
        (h.hold)(true);
        if let Some(t) = t.upgrade() {
            t.add_css_class("drop-hover");
        }
        gdk::DragAction::COPY
    });
    let (h, t) = (host, tile.downgrade());
    drop.connect_leave(move |_| {
        (h.hold)(false);
        if let Some(t) = t.upgrade() {
            t.remove_css_class("drop-hover");
        }
    });
}

fn sort_name(s: Sort) -> &'static str {
    match s {
        Sort::Date => "date",
        Sort::Name => "name",
        Sort::Kind => "kind",
    }
}

fn look_name(l: FolderLook) -> &'static str {
    match l {
        FolderLook::Stack => "stack",
        FolderLook::Folder => "folder",
    }
}

fn open_name(o: FolderOpen) -> &'static str {
    match o {
        FolderOpen::Fan => "fan",
        FolderOpen::Grid => "grid",
        FolderOpen::List => "list",
        FolderOpen::Files => "files",
    }
}

/// A section of radio items for one of a folder's settings.
fn choices(model: &gio::Menu, title: &str, action: &str, options: &[(&str, &str)]) {
    let section = gio::Menu::new();
    for (label, value) in options {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some(&format!("folder.{action}")), Some(&value.to_variant()));
        section.append_item(&item);
    }
    model.append_section(Some(title), &section);
}

/// A radio action: its state is the chosen value, and choosing one changes the folder.
fn choice_action(name: &str, current: &str, set: SetFolder, apply: fn(&mut Folder, &str)) -> gio::SimpleAction {
    let action = gio::SimpleAction::new_stateful(name, Some(glib::VariantTy::STRING), &current.to_variant());
    action.connect_change_state(move |a, value| {
        let Some(value) = value else { return };
        a.set_state(value);
        if let Some(v) = value.get::<String>() {
            set(&|f| apply(f, &v));
        }
    });
    action
}

fn folder_menu(tile: &gtk::Box, uri: &str, folder: &Folder, set: SetFolder, host: Rc<Host>) {
    let model = gio::Menu::new();
    choices(&model, "Sort by", "sort", &[("Date Added", "date"), ("Name", "name"), ("Kind", "kind")]);
    choices(&model, "Display as", "look", &[("Stack", "stack"), ("Folder", "folder")]);
    choices(
        &model,
        "View content as",
        "open",
        &[("Fan", "fan"), ("Grid", "grid"), ("List", "list"), ("Open in Files", "files")],
    );
    let last = gio::Menu::new();
    last.append(Some("Open in Files"), Some("folder.open-files"));
    last.append(Some("Remove from Dock"), Some("folder.remove"));
    model.append_section(None, &last);

    let actions = gio::SimpleActionGroup::new();
    actions.add_action(&choice_action("sort", sort_name(folder.sort), set.clone(), |f, v| {
        f.sort = match v {
            "name" => Sort::Name,
            "kind" => Sort::Kind,
            _ => Sort::Date,
        }
    }));
    actions.add_action(&choice_action("look", look_name(folder.look), set.clone(), |f, v| {
        f.look = if v == "folder" { FolderLook::Folder } else { FolderLook::Stack }
    }));
    actions.add_action(&choice_action("open", open_name(folder.open), set, |f, v| {
        f.open = match v {
            "fan" => FolderOpen::Fan,
            "list" => FolderOpen::List,
            "files" => FolderOpen::Files,
            _ => FolderOpen::Grid,
        }
    }));
    let open = gio::SimpleAction::new("open-files", None);
    let uri = uri.to_string();
    open.connect_activate(move |_, _| launch::open_default(&uri));
    actions.add_action(&open);
    let remove = gio::SimpleAction::new("remove", None);
    let h = host.clone();
    remove.connect_activate(move |_, _| (h.remove)());
    actions.add_action(&remove);
    tile.insert_action_group("folder", Some(&actions));
    let pop = gtk::PopoverMenu::from_model(Some(&model));
    pop.add_css_class("dock-menu");
    show_popover(&pop, tile, host);
}

/// Pop a popover up from a tile, holding the dock while it's open.
fn show_popover(pop: &impl IsA<gtk::Popover>, tile: &impl IsA<gtk::Widget>, host: Rc<Host>) {
    let pop = pop.as_ref().clone();
    pop.set_parent(tile);
    pop.set_position(host.side);
    pop.set_autohide(!crate::dock::testing());
    (host.hold)(true);
    pop.connect_closed(move |p| {
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
        (host.hold)(false);
    });
    pop.popup();
}

// ---------- Folder views ----------

/// The most files a grid or list shows (it scrolls); past that, Open in Files.
const SHOWN_MAX: usize = 500;
const FAN_MAX: usize = 8;

/// A view's handle to redraw itself.
type Refill = Rc<std::cell::RefCell<Option<Rc<dyn Fn()>>>>;

/// Scrolls with the wheel or touchpad, without a scrollbar; as tall as its contents
/// up to `max`.
pub fn scroller(child: &impl IsA<gtk::Widget>, max: i32) -> gtk::ScrolledWindow {
    let sw = gtk::ScrolledWindow::new();
    sw.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
    sw.set_propagate_natural_height(true);
    sw.set_propagate_natural_width(true);
    sw.set_max_content_height(max);
    sw.set_child(Some(child));
    sw
}

#[allow(clippy::too_many_arguments)]
fn open_view(
    tile: &gtk::Box,
    folder: &Folder,
    dir: &Path,
    name: &str,
    view: FolderOpen,
    host: Rc<Host>,
    set: SetFolder,
) {
    let pop = gtk::Popover::new();
    pop.add_css_class("dock-stack");
    let uri = gio::File::for_path(dir).uri().to_string();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pop.set_child(Some(&body));
    // `fill` redraws the view (after a sort change) through this handle to itself;
    // the handle is dropped when the popover closes, which ends the cycle.
    let handle: Refill = Rc::default();
    let sort = Rc::new(std::cell::Cell::new(folder.sort));
    let fill: Rc<dyn Fn()> = {
        let (body, dir, name, uri, pop_w, host, handle) =
            (body.clone(), dir.to_path_buf(), name.to_string(), uri.clone(), pop.downgrade(), host.clone(), handle.clone());
        let set = set.clone();
        Rc::new(move || {
            while let Some(c) = body.first_child() {
                body.remove(&c);
            }
            let items = contents(&dir, sort.get());
            let close = {
                let pop_w = pop_w.clone();
                move || {
                    if let Some(p) = pop_w.upgrade() {
                        p.popdown();
                    }
                }
            };
            if view == FolderOpen::Fan {
                body.append(&fan(&items, &uri, close));
                return;
            }
            let refill: Rc<dyn Fn()> = {
                let handle = handle.clone();
                Rc::new(move || {
                    // Not from inside the toggled button's own handler: it's about to go.
                    let handle = handle.clone();
                    glib::idle_add_local_once(move || {
                        let f = handle.borrow().clone();
                        if let Some(f) = f {
                            f();
                        }
                    });
                })
            };
            body.append(&header(&name, items.len(), &uri, sort.clone(), set.clone(), refill, close.clone()));
            if items.is_empty() {
                let empty = gtk::Label::new(Some("Nothing here"));
                empty.add_css_class("stack-empty");
                body.append(&empty);
            } else if view == FolderOpen::List {
                body.append(&scroller(&list_view(&items, close), host.max_height));
            } else {
                body.append(&scroller(&grid_view(&items, close), host.max_height));
            }
        })
    };
    *handle.borrow_mut() = Some(fill.clone());
    fill();
    pop.connect_closed(move |_| {
        handle.borrow_mut().take();
    });
    if view == FolderOpen::Fan {
        pop.add_css_class("dock-fan");
        pop.set_has_arrow(false);
    }
    show_popover(&pop, tile, host);
}

#[allow(clippy::too_many_arguments)]
fn header(
    name: &str,
    count: usize,
    uri: &str,
    sort: Rc<std::cell::Cell<Sort>>,
    set: SetFolder,
    refill: Rc<dyn Fn()>,
    close: impl Fn() + Clone + 'static,
) -> gtk::Widget {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("stack-header");
    let title = gtk::Label::new(Some(name));
    title.add_css_class("stack-title");
    row.append(&title);
    let n = gtk::Label::new(Some(&format!("{count} items")));
    n.add_css_class("stack-n");
    n.set_hexpand(true);
    n.set_xalign(0.0);
    row.append(&n);
    let seg = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    seg.add_css_class("stack-sort");
    let mut first: Option<gtk::ToggleButton> = None;
    for (s, icon, tip) in [
        (Sort::Date, "document-open-recent-symbolic", "Newest first"),
        (Sort::Name, "view-sort-ascending-symbolic", "By name"),
        (Sort::Kind, "view-grid-symbolic", "By kind"),
    ] {
        let b = gtk::ToggleButton::new();
        b.set_icon_name(icon);
        b.set_tooltip_text(Some(tip));
        b.set_active(sort.get() == s);
        if let Some(f) = &first {
            b.set_group(Some(f));
        } else {
            first = Some(b.clone());
        }
        let (sort, set, refill) = (sort.clone(), set.clone(), refill.clone());
        b.connect_toggled(move |b| {
            if b.is_active() && sort.get() != s {
                sort.set(s);
                set(&|f| f.sort = s);
                refill();
            }
        });
        seg.append(&b);
    }
    row.append(&seg);
    let files = gtk::Button::from_icon_name("folder-open-symbolic");
    files.set_tooltip_text(Some("Open in Files"));
    files.add_css_class("flat");
    let uri = uri.to_string();
    files.connect_clicked(move |_| {
        launch::open_default(&uri);
        close();
    });
    row.append(&files);
    row.upcast()
}

/// Open a file on click and let it be dragged out to other apps.
fn file_actions(w: &gtk::Widget, l: &Listed, close: impl Fn() + Clone + 'static) {
    let click = gtk::GestureClick::new();
    let uri = l.file.uri().to_string();
    let c = close.clone();
    click.connect_released(move |_, _, _, _| {
        launch::open_default(&uri);
        c();
    });
    w.add_controller(click);
    let drag = gtk::DragSource::new();
    drag.set_actions(gdk::DragAction::COPY | gdk::DragAction::MOVE);
    let file = l.file.clone();
    drag.connect_prepare(move |_, _, _| {
        Some(gdk::ContentProvider::for_value(&gdk::FileList::from_array(std::slice::from_ref(&file)).to_value()))
    });
    w.add_controller(drag);
}

fn grid_view(items: &[Listed], close: impl Fn() + Clone + 'static) -> gtk::Widget {
    let grid = gtk::FlowBox::new();
    grid.add_css_class("stack-grid");
    grid.set_min_children_per_line(4);
    grid.set_max_children_per_line(4);
    grid.set_selection_mode(gtk::SelectionMode::None);
    grid.set_homogeneous(true);
    for l in items.iter().take(SHOWN_MAX) {
        let cell = gtk::Box::new(gtk::Orientation::Vertical, 4);
        cell.add_css_class("stack-file");
        let pic = file_picture(l, 64);
        pic.set_halign(gtk::Align::Center);
        cell.append(&pic);
        let name = gtk::Label::new(Some(&l.entry.name));
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        name.set_max_width_chars(12);
        name.add_css_class("stack-name");
        cell.append(&name);
        let when = gtk::Label::new(Some(&age(l.entry.modified)));
        when.add_css_class("stack-when");
        cell.append(&when);
        cell.set_tooltip_text(Some(&l.entry.name));
        file_actions(cell.upcast_ref(), l, close.clone());
        grid.insert(&cell, -1);
        if let Some(child) = grid.last_child() {
            child.set_focusable(false);
        }
    }
    grid.upcast()
}

fn list_view(items: &[Listed], close: impl Fn() + Clone + 'static) -> gtk::Widget {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
    col.add_css_class("stack-list");
    for l in items.iter().take(SHOWN_MAX) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.add_css_class("stack-row");
        row.append(&file_picture(l, 28));
        let name = gtk::Label::new(Some(&l.entry.name));
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        name.set_hexpand(true);
        name.set_xalign(0.0);
        name.set_max_width_chars(28);
        row.append(&name);
        let when = gtk::Label::new(Some(&age(l.entry.modified)));
        when.add_css_class("stack-when");
        row.append(&when);
        file_actions(row.upcast_ref(), l, close.clone());
        col.append(&row);
    }
    col.upcast()
}

/// Newest at the bottom, nearest the dock, curving away as it rises.
fn fan(items: &[Listed], uri: &str, close: impl Fn() + Clone + 'static) -> gtk::Widget {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 8);
    col.add_css_class("stack-fan");
    let shown: Vec<&Listed> = items.iter().take(FAN_MAX).collect();
    // The "open in Files" row first (top), then oldest to newest.
    let open = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    open.add_css_class("fan-row");
    open.set_halign(gtk::Align::End);
    let label = gtk::Label::new(Some(&format!("Open in Files · {} items", items.len())));
    label.add_css_class("fan-label");
    open.append(&label);
    let icon = gtk::Image::from_icon_name("folder-open-symbolic");
    icon.set_pixel_size(24);
    let frame = framed(icon.upcast(), 48, 42);
    open.append(&frame);
    let (u, c) = (uri.to_string(), close.clone());
    let click = gtk::GestureClick::new();
    click.connect_released(move |_, _, _, _| {
        launch::open_default(&u);
        c();
    });
    open.add_controller(click);
    // The curve bends right as it rises: the top row sits nearest the edge.
    open.set_margin_end(0);
    col.append(&open);
    for (i, l) in shown.iter().enumerate().rev() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.add_css_class("fan-row");
        row.set_halign(gtk::Align::End);
        let name = gtk::Label::new(Some(&l.entry.name));
        name.add_css_class("fan-label");
        name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        name.set_max_width_chars(26);
        row.append(&name);
        row.append(&file_picture(l, 48));
        row.set_margin_end(arc(shown.len()) - arc(i + 1));
        file_actions(row.upcast_ref(), l, close.clone());
        col.append(&row);
    }
    col.upcast()
}

/// How far a fan row sits from the dock edge: a gentle curve.
fn arc(i: usize) -> i32 {
    let x = i as f64;
    (x * x * 3.0).round() as i32
}

// ---------- Trash ----------

pub fn trash_tile(host: Rc<Host>) -> gtk::Widget {
    let tile = gtk::Box::new(gtk::Orientation::Vertical, 2);
    tile.add_css_class("dock-item");
    tile.add_css_class("dock-trash");
    // A satin tile with a glyph, like the apps' icons (STYLE.md 2.0), rather than the
    // icon theme's full-colour bin.
    let satin = gtk::Box::new(gtk::Orientation::Vertical, 0);
    satin.add_css_class("dock-satin");
    satin.set_size_request(host.icon_size, host.icon_size);
    satin.set_halign(gtk::Align::Center);
    let image = gtk::Image::new();
    image.set_pixel_size((host.icon_size / 2).max(16));
    image.set_vexpand(true);
    image.set_valign(gtk::Align::Center);
    satin.append(&image);
    tile.append(&satin);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.add_css_class("dock-dots");
    tile.append(&spacer);
    let trash = gio::File::for_uri("trash:///");
    let draw = {
        let (image, tile, trash) = (image.clone(), tile.clone(), trash.clone());
        move || {
            let count = trash
                .query_info("trash::item-count", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE)
                .map(|i| i.attribute_uint32("trash::item-count"))
                .unwrap_or(0);
            image.set_icon_name(Some(if count > 0 { "user-trash-full-symbolic" } else { "user-trash-symbolic" }));
            tile.set_tooltip_text(Some(&if count > 0 { format!("Trash · {count} items") } else { "Trash".into() }));
            tile.set_widget_name(if count > 0 { "full" } else { "empty" });
        }
    };
    draw();
    if let Ok(monitor) = trash.monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE) {
        monitor.connect_changed(move |_, _, _, _| draw());
        unsafe { tile.set_data("nyx-dock-monitor", monitor) };
    }

    let click = gtk::GestureClick::new();
    click.set_button(0);
    let (h, tw) = (host.clone(), tile.downgrade());
    click.connect_released(move |g, _, _, _| {
        let Some(tile) = tw.upgrade() else { return };
        match g.current_button() {
            1 => launch::open_trash(),
            3 => trash_menu(&tile, h.clone()),
            _ => {}
        }
    });
    tile.add_controller(click);

    // Files dropped on the Trash go to the Trash.
    let drop = gtk::DropTarget::new(gdk::FileList::static_type(), gdk::DragAction::MOVE | gdk::DragAction::COPY);
    drop.connect_drop(|_, value, _, _| {
        let Ok(list) = value.get::<gdk::FileList>() else { return false };
        for f in list.files() {
            f.trash_async(glib::Priority::DEFAULT, gio::Cancellable::NONE, |r| {
                if let Err(e) = r {
                    eprintln!("nyx-dock: couldn't move to the Trash: {e}");
                }
            });
        }
        true
    });
    hold_while_dragging(&drop, &tile, host);
    tile.add_controller(drop);
    tile.upcast()
}

fn trash_menu(tile: &gtk::Box, host: Rc<Host>) {
    let full = tile.widget_name() == "full";
    let pop = gtk::Popover::new();
    pop.add_css_class("dock-menu");
    let col = gtk::Box::new(gtk::Orientation::Vertical, 2);
    col.add_css_class("trash-menu");
    pop.set_child(Some(&col));
    let open = gtk::Button::with_label("Open Trash");
    open.add_css_class("flat");
    let pw = pop.downgrade();
    open.connect_clicked(move |_| {
        launch::open_trash();
        if let Some(p) = pw.upgrade() {
            p.popdown();
        }
    });
    col.append(&open);
    if full {
        // Emptying can't be undone: the first click asks, the second empties.
        let empty = gtk::Button::with_label("Empty Trash…");
        empty.add_css_class("flat");
        empty.add_css_class("trash-empty");
        let pw = pop.downgrade();
        empty.connect_clicked(move |b| {
            if b.has_css_class("armed") {
                launch::empty_trash();
                if let Some(p) = pw.upgrade() {
                    p.popdown();
                }
            } else {
                b.add_css_class("armed");
                b.set_label("Empty Trash: this can't be undone");
                let bw = b.downgrade();
                glib::timeout_add_local_once(std::time::Duration::from_secs(4), move || {
                    if let Some(b) = bw.upgrade() {
                        b.remove_css_class("armed");
                        b.set_label("Empty Trash…");
                    }
                });
            }
        });
        col.append(&empty);
    }
    show_popover(&pop, tile, host);
}
