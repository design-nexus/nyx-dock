//! Nyx Dock: a floating app dock for Omarchy and Hyprland.
//!
//! `nyx-dock` runs the dock (normally as the `nyx-dock` user service).
//! `nyx-dock --enable` starts it now and at login and turns Omarchy's own dock off;
//! `nyx-dock --disable` undoes both. `--only <connector>` shows the dock on one output
//! only (for trying it on a test output). `--ui qml|gtk` picks the Quickshell or the GTK
//! interface.

mod apps;
mod badges;
mod card;
mod controls;
mod dock;
mod handover;
mod launch;
mod mpris;
mod slide;
mod stacks;
mod swipe;
mod theme;

use gtk::prelude::*;

pub const APP_ID: &str = "io.github.design_nexus.nyx.Dock";

fn main() -> std::process::ExitCode {
    let mut only = std::env::var("NYX_DOCK_ONLY").ok().filter(|s| !s.is_empty());
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--enable" => return report(handover::enable()),
            "--disable" => return report(handover::disable()),
            "--only" => only = args.next(),
            "--ui" => {
                return match args.next().as_deref().map(nyx_ui_link::switch::Ui::parse) {
                    Some(Some(ui)) => report(handover::choose(ui)),
                    Some(None) => {
                        eprintln!("nyx-dock: --ui takes qml or gtk");
                        std::process::ExitCode::from(2)
                    }
                    None => {
                        let now = nyx_ui_link::switch::current(&handover::COMPONENT);
                        println!("{}", if now == nyx_ui_link::switch::Ui::Qml { "qml" } else { "gtk" });
                        std::process::ExitCode::SUCCESS
                    }
                };
            }
            "--version" => {
                println!("nyx-dock {}", env!("CARGO_PKG_VERSION"));
                return std::process::ExitCode::SUCCESS;
            }
            "-h" | "--help" => {
                println!(
                    "Usage: nyx-dock [--enable | --disable | --ui [qml|gtk] | --only <output>]\n\n\
                     With no option, run the dock.\n\
                     --enable    start the dock now and at login, and turn Omarchy's dock off\n\
                     --disable   stop the dock and turn Omarchy's dock back on\n\
                     --ui        use the Quickshell (qml) or the GTK interface; alone, show which\n\
                     --only      show the dock on this output only (e.g. HEADLESS-1)"
                );
                return std::process::ExitCode::SUCCESS;
            }
            other => {
                eprintln!("nyx-dock: unknown option {other}");
                return std::process::ExitCode::from(2);
            }
        }
    }

    if only.is_none() && nyx_ui_link::switch::current(&handover::COMPONENT) == nyx_ui_link::switch::Ui::Qml {
        eprintln!("nyx-dock: the Quickshell dock is on (nyx-dock-backend and nyx-shell-qs); switch back with: nyx-dock --ui gtk");
        return std::process::ExitCode::SUCCESS;
    }
    let flags = if only.is_some() { gtk::gio::ApplicationFlags::NON_UNIQUE } else { Default::default() };
    let app = gtk::Application::builder().application_id(APP_ID).flags(flags).build();
    app.connect_startup(|_| theme::install());
    let started = std::cell::Cell::new(false);
    app.connect_activate(move |app| {
        if !started.replace(true) {
            dock::start(app, only.clone());
        }
    });
    let code = app.run_with_args::<&str>(&[]);
    std::process::ExitCode::from(code.get() as u8)
}

fn report(result: Result<String, String>) -> std::process::ExitCode {
    match result {
        Ok(msg) => {
            println!("{msg}");
            std::process::ExitCode::SUCCESS
        }
        Err(msg) => {
            eprintln!("nyx-dock: {msg}");
            std::process::ExitCode::FAILURE
        }
    }
}
