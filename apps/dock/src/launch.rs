//! Starting apps and acting on windows.
//!
//! Apps start the way Omarchy's launcher starts them: `uwsm-app -- gtk-launch <id>`, so
//! each runs in its own scope and outlives the dock; terminal apps go through
//! `omarchy-launch-tui`. Without uwsm, GIO launches them directly.

use gtk::prelude::*;
use std::process::{Command, Stdio};

fn has(program: &str) -> bool {
    std::env::var_os("PATH").is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(program).is_file()))
}

/// Run a command detached from the dock (`setsid -f`), so it isn't the dock's child.
fn spawn(args: &[String]) {
    let mut cmd = Command::new("setsid");
    cmd.arg("-f").args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    match cmd.spawn() {
        // setsid -f returns at once; reap it off the main thread.
        Ok(mut child) => {
            std::thread::spawn(move || child.wait());
        }
        Err(e) => eprintln!("nyx-dock: couldn't start {}: {e}", args.join(" ")),
    }
}

pub fn info(id: &str) -> Option<gio_unix::DesktopAppInfo> {
    gio_unix::DesktopAppInfo::new(&format!("{id}.desktop"))
}

/// Start an app, or one of its desktop actions (`new-window`, …).
pub fn launch(id: &str, action: Option<&str>) {
    let Some(app) = info(id) else { return };
    if action.is_none() && app.boolean("Terminal") && has("omarchy-launch-tui") {
        let command: Vec<String> = app
            .commandline()
            .map(|c| c.to_string_lossy().split_whitespace().filter(|w| !w.starts_with('%')).map(str::to_string).collect())
            .unwrap_or_default();
        if !command.is_empty() {
            let mut args = vec!["omarchy-launch-tui".to_string()];
            args.extend(command);
            spawn(&args);
            return;
        }
    }
    if has("uwsm-app") {
        let target = match action {
            Some(a) => format!("{id}.desktop:{a}"),
            None => format!("{id}.desktop"),
        };
        let args: Vec<String> = match action {
            // uwsm resolves `id.desktop:action` itself; gtk-launch can't run actions.
            Some(_) => vec!["uwsm-app".into(), "--".into(), target],
            None => vec!["uwsm-app".into(), "--".into(), "gtk-launch".into(), target],
        };
        spawn(&args);
        return;
    }
    let ctx = gtk::gdk::Display::default().map(|d| d.app_launch_context());
    let result = match action {
        Some(a) => {
            app.launch_action(a, ctx.as_ref());
            Ok(())
        }
        None => app.launch(&[], ctx.as_ref()),
    };
    if let Err(e) = result {
        eprintln!("nyx-dock: couldn't start {id}: {e}");
    }
}

/// A new instance: the app's "new window" action if it has one, else start it again.
pub fn new_instance(id: &str) {
    let action = info(id).and_then(|app| {
        app.list_actions()
            .into_iter()
            .find(|a| matches!(a.as_str(), "new-window" | "new-empty-window" | "NewWindow"))
            .map(|a| a.to_string())
    });
    launch(id, action.as_deref());
}

/// Focus a window and raise it. A minimized one (on Omarchy's minimized workspace)
/// first comes back to the workspace in front of the user.
pub fn focus(address: &str, minimized: bool, workspace: i64) {
    nyx_desktop::actions::focus(address, minimized, workspace);
}

/// Ask a window to close (the app may still ask about unsaved work).
pub fn close(address: &str) {
    nyx_desktop::actions::close(address);
}

/// Open files or folders with an app (dropped on its icon).
pub fn open_with(id: &str, uris: &[String]) {
    if uris.is_empty() {
        return;
    }
    if has("uwsm-app") {
        let mut args = vec!["uwsm-app".to_string(), "--".into(), "gtk-launch".into(), format!("{id}.desktop")];
        args.extend(uris.iter().cloned());
        spawn(&args);
        return;
    }
    let Some(app) = info(id) else { return };
    let files: Vec<gtk::gio::File> = uris.iter().map(|u| gtk::gio::File::for_uri(u)).collect();
    let ctx = gtk::gdk::Display::default().map(|d| d.app_launch_context());
    if let Err(e) = app.launch(&files, ctx.as_ref()) {
        eprintln!("nyx-dock: couldn't open the files with {id}: {e}");
    }
}

/// Open a file or folder with its default app (folders: the file manager).
pub fn open_default(uri: &str) {
    if has("uwsm-app") && has("xdg-open") {
        spawn(&["uwsm-app".into(), "--".into(), "xdg-open".into(), uri.into()]);
        return;
    }
    let ctx = gtk::gdk::Display::default().map(|d| d.app_launch_context());
    if let Err(e) = gtk::gio::AppInfo::launch_default_for_uri(uri, ctx.as_ref()) {
        eprintln!("nyx-dock: couldn't open {uri}: {e}");
    }
}

/// The app a launcher entry stands for: Omarchy's "Browser" entry
/// (`omarchy-launch-browser`, no icon of its own) is the default browser it opens.
/// Other entries stand for themselves.
pub fn stands_for(id: &str) -> Option<String> {
    let exec = info(id)?.string("Exec")?.to_string();
    if exec.split_whitespace().next()?.rsplit('/').next()? != "omarchy-launch-browser" {
        return None;
    }
    let out = Command::new("omarchy-cmd-default-browser").stderr(Stdio::null()).output().ok()?;
    let browser = String::from_utf8_lossy(&out.stdout).trim().trim_end_matches(".desktop").to_string();
    (!browser.is_empty() && browser != id && info(&browser).is_some()).then_some(browser)
}

/// Whether Nyx Switcher is installed (its overview shows one app's windows).
pub fn switcher_installed() -> bool {
    has("nyx-switcher-send")
}

/// Show one app's windows in Nyx Switcher's overview.
pub fn show_windows(key: &str) {
    spawn(&["nyx-switcher-send".into(), "overview".into(), key.into()]);
}

/// Empty the Trash.
pub fn empty_trash() {
    spawn(&["gio".into(), "trash".into(), "--empty".into()]);
}
