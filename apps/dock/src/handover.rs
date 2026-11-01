//! Switching between Omarchy's dock and this one. Omarchy's dock is turned off with
//! Omarchy's own command (it records the choice in `~/.config/omarchy/shell.json`), so
//! nothing of Omarchy's is edited by hand and `--disable` puts it back exactly.

use std::process::Command;

const SERVICE: &str = "nyx-dock.service";
const OMARCHY_DOCK: &str = "omarchy.dock";

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

pub fn enable() -> Result<String, String> {
    run("systemctl", &["--user", "enable", "--now", SERVICE])?;
    if has("omarchy-plugin-disable") {
        run("omarchy-plugin-disable", &[OMARCHY_DOCK])?;
        Ok("Nyx Dock is on; Omarchy's dock is off.".into())
    } else {
        Ok("Nyx Dock is on.".into())
    }
}

pub fn disable() -> Result<String, String> {
    // Omarchy's dock comes back even if the service was never set up.
    let stopped = run("systemctl", &["--user", "disable", "--now", SERVICE]);
    if let Err(e) = &stopped {
        eprintln!("nyx-dock: {e}");
    }
    if has("omarchy-plugin-enable") {
        run("omarchy-plugin-enable", &[OMARCHY_DOCK])?;
        Ok("Nyx Dock is off; Omarchy's dock is back.".into())
    } else {
        Ok("Nyx Dock is off.".into())
    }
}
