//! The Apps button, first in the dock: every app from the app menu in a grid that
//! rises from the dock, with a search field. Click an app to open it, right-click to
//! keep it in the dock, or drag it onto the dock to pin it there.

use crate::dock::DockDrag;
use crate::launch;
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::RefCell;
use std::rc::Rc;

type SetPinned = Box<dyn Fn(&str, bool)>;
type Moving = Box<dyn Fn(Option<&str>)>;

/// What the Apps button needs from its dock.
pub struct Host {
    /// Keep the dock up while the panel is open.
    pub hold: Box<dyn Fn(bool)>,
    /// The keyboard for the search field while the panel is open.
    pub keyboard: Box<dyn Fn(bool)>,
    pub is_pinned: Box<dyn Fn(&str) -> bool>,
    pub set_pinned: SetPinned,
    /// An app from the panel starts (`Some`) or stops (`None`) moving onto the dock.
    pub drag: Moving,
    pub side: gtk::PositionType,
    pub icon_size: i32,
    /// The tallest the grid grows before it scrolls.
    pub max_height: i32,
    /// Names under the icons; without them, icons only (tooltips name them).
    pub names: bool,
}

struct App {
    id: String,
    name: String,
    icon: Option<gio::Icon>,
    /// Name, id, generic name and keywords, lowercased, for search.
    words: String,
}

impl App {
    fn matches(&self, query: &str) -> bool {
        query.split_whitespace().all(|w| self.words.contains(w))
    }
}

/// The apps the app menu shows (those not hidden from menus), by name.
fn all() -> Vec<App> {
    let mut apps: Vec<App> = gio::AppInfo::all()
        .into_iter()
        .filter(|a| a.should_show())
        .filter_map(|a| {
            let id = a.id()?.trim_end_matches(".desktop").to_string();
            let name = a.display_name().to_string();
            let mut words = format!("{name} {id}");
            if let Some(d) = a.downcast_ref::<gio_unix::DesktopAppInfo>() {
                if let Some(g) = d.generic_name() {
                    words.push(' ');
                    words.push_str(&g);
                }
                for k in d.keywords() {
                    words.push(' ');
                    words.push_str(&k);
                }
            }
            Some(App { id, name, icon: a.icon(), words: words.to_lowercase() })
        })
        .collect();
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.id == b.id);
    apps
}

pub fn tile(host: Rc<Host>) -> gtk::Widget {
    let tile = gtk::Box::new(gtk::Orientation::Vertical, 2);
    tile.add_css_class("dock-item");
    tile.add_css_class("dock-apps");
    let satin = gtk::Box::new(gtk::Orientation::Vertical, 0);
    satin.add_css_class("dock-satin");
    satin.set_size_request(host.icon_size, host.icon_size);
    satin.set_halign(gtk::Align::Center);
    let image = gtk::Image::from_icon_name("view-app-grid-symbolic");
    image.set_pixel_size((host.icon_size / 2).max(16));
    image.set_vexpand(true);
    image.set_valign(gtk::Align::Center);
    satin.append(&image);
    tile.append(&satin);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.add_css_class("dock-dots");
    tile.append(&spacer);
    tile.set_tooltip_text(Some("Apps"));

    let click = gtk::GestureClick::new();
    click.set_button(1);
    let (h, tw) = (host.clone(), tile.downgrade());
    click.connect_released(move |_, _, _, _| {
        if let Some(tile) = tw.upgrade() {
            open(&tile, h.clone());
        }
    });
    tile.add_controller(click);

    // A test hook (NYX_DOCK_TEST_APPS): opens the panel as a click would.
    if std::env::var_os("NYX_DOCK_TEST_APPS").is_some() {
        let actions = gio::SimpleActionGroup::new();
        let action = gio::SimpleAction::new("open", None);
        let tw = tile.downgrade();
        action.connect_activate(move |_, _| {
            if let Some(tile) = tw.upgrade() {
                open(&tile, host.clone());
            }
        });
        actions.add_action(&action);
        tile.insert_action_group("apps-test", Some(&actions));
    }
    tile.upcast()
}

fn open(tile: &gtk::Box, host: Rc<Host>) {
    let apps = Rc::new(all());
    crate::dock::debug(|| format!("apps panel: {} apps", apps.len()));
    let pop = gtk::Popover::new();
    pop.add_css_class("dock-stack");
    pop.add_css_class("dock-apps-panel");
    let col = gtk::Box::new(gtk::Orientation::Vertical, 8);
    pop.set_child(Some(&col));

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search apps"));
    search.add_css_class("apps-search");
    col.append(&search);

    let close = {
        let pw = pop.downgrade();
        move || {
            if let Some(p) = pw.upgrade() {
                p.popdown();
            }
        }
    };
    let grid = gtk::FlowBox::new();
    grid.add_css_class("apps-grid");
    if !host.names {
        grid.add_css_class("icons-only");
    }
    let columns = if host.names { 7 } else { 9 };
    grid.set_min_children_per_line(columns);
    grid.set_max_children_per_line(columns);
    grid.set_selection_mode(gtk::SelectionMode::None);
    grid.set_homogeneous(true);
    grid.set_valign(gtk::Align::Start);
    for app in apps.iter() {
        grid.insert(&cell(app, &host, close.clone()), -1);
        if let Some(child) = grid.last_child() {
            child.set_focusable(false);
        }
    }
    let query: Rc<RefCell<String>> = Rc::default();
    let (q, list) = (query.clone(), apps.clone());
    grid.set_filter_func(move |child| list.get(child.index() as usize).is_some_and(|a| a.matches(&q.borrow())));
    col.append(&crate::stacks::scroller(&grid, host.max_height));
    let empty = gtk::Label::new(Some("No apps found"));
    empty.add_css_class("stack-empty");
    empty.set_visible(false);
    col.append(&empty);

    let list = apps.clone();
    search.connect_search_changed(move |e| {
        *query.borrow_mut() = e.text().to_lowercase();
        grid.invalidate_filter();
        empty.set_visible(!list.iter().any(|a| a.matches(&query.borrow())));
    });
    let c = close.clone();
    search.connect_activate(move |e| {
        let q = e.text().to_lowercase();
        if let Some(app) = apps.iter().find(|a| a.matches(&q)) {
            launch::launch(&app.id, None);
            c();
        }
    });
    search.connect_stop_search(move |_| close());

    pop.set_parent(tile);
    pop.set_position(host.side);
    let keyboard = !crate::dock::testing();
    pop.set_autohide(keyboard);
    (host.hold)(true);
    if keyboard {
        (host.keyboard)(true);
    }
    pop.connect_closed(move |p| {
        crate::dock::debug(|| "apps panel closed".into());
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
        if keyboard {
            (host.keyboard)(false);
        }
        (host.hold)(false);
    });
    pop.popup();
    search.grab_focus();
}

fn cell(app: &App, host: &Rc<Host>, close: impl Fn() + Clone + 'static) -> gtk::Box {
    let cell = gtk::Box::new(gtk::Orientation::Vertical, 4);
    cell.add_css_class("apps-cell");
    let image = gtk::Image::new();
    image.set_pixel_size(48);
    match &app.icon {
        Some(icon) => image.set_from_gicon(icon),
        None => image.set_icon_name(Some("application-x-executable")),
    }
    cell.append(&image);
    if host.names {
        let name = gtk::Label::new(Some(&app.name));
        name.add_css_class("apps-name");
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_max_width_chars(9);
        name.set_width_chars(9);
        name.set_justify(gtk::Justification::Center);
        cell.append(&name);
    }
    cell.set_tooltip_text(Some(&app.name));

    let click = gtk::GestureClick::new();
    click.set_button(0);
    let (id, h, cw, c) = (app.id.clone(), host.clone(), cell.downgrade(), close.clone());
    click.connect_released(move |g, _, _, _| match g.current_button() {
        1 => {
            launch::launch(&id, None);
            c();
        }
        3 => {
            if let Some(cell) = cw.upgrade() {
                cell_menu(&cell, &id, h.clone(), c.clone());
            }
        }
        _ => {}
    });
    cell.add_controller(click);

    // Drag onto the dock to pin it there.
    let drag = gtk::DragSource::new();
    drag.set_actions(gdk::DragAction::MOVE);
    let id = app.id.clone();
    drag.connect_prepare(move |_, _, _| {
        Some(gdk::ContentProvider::for_value(&glib::BoxedAnyObject::new(DockDrag(id.clone())).to_value()))
    });
    let (id, h, iw, cw) = (app.id.clone(), host.clone(), image.downgrade(), cell.downgrade());
    drag.connect_drag_begin(move |src, _| {
        if let Some(image) = iw.upgrade() {
            let paintable = gtk::WidgetPaintable::new(Some(&image)).current_image();
            src.set_icon(Some(&paintable), image.width() / 2, image.height() / 2);
        }
        if let Some(pop) = cw.upgrade().and_then(|c| c.ancestor(gtk::Popover::static_type())) {
            pop.add_css_class("moving");
        }
        (h.drag)(Some(&id));
    });
    let h = host.clone();
    drag.connect_drag_end(move |_, _, _| {
        (h.drag)(None);
        close();
    });
    cell.add_controller(drag);
    cell
}

fn cell_menu(cell: &gtk::Box, id: &str, host: Rc<Host>, close: impl Fn() + Clone + 'static) {
    let pinned = (host.is_pinned)(id);
    let model = gio::Menu::new();
    model.append(Some("Open"), Some("app.open"));
    model.append(Some(if pinned { "Remove from Dock" } else { "Keep in Dock" }), Some("app.pin"));
    let actions = gio::SimpleActionGroup::new();
    let open = gio::SimpleAction::new("open", None);
    let (i, c) = (id.to_string(), close);
    open.connect_activate(move |_, _| {
        launch::launch(&i, None);
        c();
    });
    actions.add_action(&open);
    let pin = gio::SimpleAction::new("pin", None);
    let i = id.to_string();
    pin.connect_activate(move |_, _| (host.set_pinned)(&i, !pinned));
    actions.add_action(&pin);
    cell.insert_action_group("app", Some(&actions));
    let pop = gtk::PopoverMenu::from_model(Some(&model));
    pop.add_css_class("dock-menu");
    pop.set_parent(cell);
    pop.connect_closed(|p| {
        let p = p.clone();
        glib::idle_add_local_once(move || p.unparent());
    });
    pop.popup();
}
