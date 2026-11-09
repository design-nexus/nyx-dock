//! Switching between Omarchy's dock and this one. Omarchy's dock is turned off with
//! Omarchy's own command (it records the choice in `~/.config/omarchy/shell.json`), so
//! nothing of Omarchy's is edited by hand and `--disable` puts it back exactly.
//!
//! Nyx Dock itself comes in two interfaces: the GTK dock (`nyx-dock.service`) and the
//! Quickshell one (`nyx-dock-backend.service` plus the dock's QML in the shared
//! `nyx-shell-qs` runtime). `--ui` chooses which; the choice is kept in
//! `~/.local/share/nyx-dock/ui` and `--enable` turns that one on.

use std::process::Command;

use nyx_ui_link::switch::{self, Component, Ui};

const SERVICE: &str = "nyx-dock.service";
const OMARCHY_DOCK: &str = "omarchy.dock";

pub const COMPONENT: Component = Component { app: "nyx-dock", gtk_unit: SERVICE, backend_units: &["nyx-dock-backend.service"] };

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let out = Command::new(program).args(args).output().map_err(|e| format!("couldn't run {program}: {e}"))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!("{program} {}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()))
    }
}

fn has(program: &str) -> bool {
    Command::new("sh").args(["-c", &format!("command -v {program}")]).output().is_ok_and(|o| o.status.success())
}

fn choice_file() -> std::path::PathBuf {
    switch::qml_dir(COMPONENT.app).with_file_name("ui")
}

/// The interface `--enable` turns on: the one last chosen, else GTK.
pub fn chosen() -> Ui {
    std::fs::read_to_string(choice_file()).ok().and_then(|s| Ui::parse(s.trim())).unwrap_or(Ui::Gtk)
}

fn is_on() -> bool {
    switch::current(&COMPONENT) == Ui::Qml || run("systemctl", &["--user", "is-enabled", "--quiet", SERVICE]).is_ok()
}

/// `--ui qml|gtk`: remember the choice, and switch now if Nyx Dock is on.
pub fn choose(ui: Ui) -> Result<String, String> {
    if let Some(dir) = choice_file().parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(choice_file(), if ui == Ui::Qml { "qml\n" } else { "gtk\n" }).map_err(|e| e.to_string())?;
    if is_on() {
        switch::set(&COMPONENT, ui)
    } else {
        Ok(format!("Nyx Dock will use the {} interface; turn it on with: nyx-dock --enable", name(ui)))
    }
}

fn name(ui: Ui) -> &'static str {
    if ui == Ui::Qml { "Quickshell" } else { "GTK" }
}

pub fn enable() -> Result<String, String> {
    switch::set(&COMPONENT, chosen())?;
    if has("omarchy-plugin-disable") {
        run("omarchy-plugin-disable", &[OMARCHY_DOCK])?;
        Ok("Nyx Dock is on; Omarchy's dock is off.".into())
    } else {
        Ok("Nyx Dock is on.".into())
    }
}

pub fn disable() -> Result<String, String> {
    // Omarchy's dock comes back even if the service was never set up.
    let _ = std::fs::remove_file(switch::components_dir().join(COMPONENT.app));
    for unit in COMPONENT.backend_units.iter().chain([&SERVICE]) {
        if let Err(e) = run("systemctl", &["--user", "disable", "--now", unit]) {
            eprintln!("nyx-dock: {e}");
        }
    }
    let empty = std::fs::read_dir(switch::components_dir()).map(|mut d| d.next().is_none()).unwrap_or(true);
    if empty {
        let _ = run("systemctl", &["--user", "disable", "--now", switch::RUNTIME_UNIT]);
    }
    if has("omarchy-plugin-enable") {
        run("omarchy-plugin-enable", &[OMARCHY_DOCK])?;
        Ok("Nyx Dock is off; Omarchy's dock is back.".into())
    } else {
        Ok("Nyx Dock is off.".into())
    }
}
