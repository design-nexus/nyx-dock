//! Starting apps and acting on windows.
//!
//! Apps start the way Omarchy's launcher starts them: `uwsm-app -- gtk-launch <id>`, so
//! each runs in its own scope and outlives the dock; terminal apps go through
//! `omarchy-launch-tui`. Without uwsm, GIO launches them directly.

use gtk::prelude::*;
use nyx_desktop::hypr::Instance;
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

fn dispatch(lua: &str, classic: &str) {
    let Ok(hypr) = Instance::find() else { return };
    if hypr.dispatch(lua).is_err() {
        let _ = hypr.dispatch(classic);
    }
}

/// Focus a window and raise it. A minimized one (on Omarchy's minimized workspace)
/// first comes back to the workspace in front of the user.
pub fn focus(address: &str, minimized: bool, workspace: i64) {
    let target = format!("window = \"address:0x{address}\"");
    if minimized && workspace >= 1 {
        dispatch(
            &format!("hl.dsp.window.move({{ workspace = \"{workspace}\", {target} }})"),
            &format!("movetoworkspacesilent {workspace},address:0x{address}"),
        );
    }
    dispatch(&format!("hl.dsp.focus({{ {target} }})"), &format!("focuswindow address:0x{address}"));
    dispatch(
        &format!("hl.dsp.window.alter_zorder({{ mode = \"top\", {target} }})"),
        &format!("alterzorder top,address:0x{address}"),
    );
}

/// Ask a window to close (the app may still ask about unsaved work).
pub fn close(address: &str) {
    dispatch(
        &format!("hl.dsp.window.close({{ window = \"address:0x{address}\" }})"),
        &format!("closewindow address:0x{address}"),
    );
}
