//! Nyx Dock's Hyprland plugin (`hyprland-plugin/`): loading it, and hearing how far a
//! workspace swipe has gone. Without it the dock still hides and comes back, once a
//! swipe has ended.

use gtk::{gio, glib};
use nyx_desktop::hypr::Instance;
use nyx_dock_core::swipe::{self, SwipeEvent};
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

const PLUGIN: &str = "nyx-dock-swipe";

/// Where the plugin is: `NYX_DOCK_SWIPE_PLUGIN` (NyxOS builds it with its Hyprland),
/// else where `install.sh` puts it.
fn plugin_path() -> PathBuf {
    std::env::var_os("NYX_DOCK_SWIPE_PLUGIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| glib::home_dir().join(format!(".local/lib/nyx-dock/{PLUGIN}.so")))
}

/// Load the plugin into Hyprland if it's installed and not in yet: at start, and after
/// a config reload (Hyprland drops plugins its config doesn't ask for). Off the main
/// thread; a plugin that won't load (built for another Hyprland) is reported once.
pub fn ensure_loaded() {
    let path = plugin_path();
    if !path.exists() {
        return;
    }
    glib::MainContext::default().spawn_local(async move {
        let failed = gio::spawn_blocking(move || {
            // A reload unloads plugins before it says it's done: look once it has.
            std::thread::sleep(Duration::from_millis(500));
            let hypr = Instance::find().ok()?;
            if hypr.request("plugin list").ok()?.contains(&format!("Plugin {PLUGIN} ")) {
                return None;
            }
            let reply = hypr.request(&format!("plugin load {}", path.display())).ok()?;
            (reply.trim() != "ok").then(|| reply.trim().to_string())
        })
        .await
        .ok()
        .flatten();
        crate::dock::debug(|| format!("swipe plugin: {}", failed.as_deref().unwrap_or("loaded")));
        if let Some(why) = failed {
            static TOLD: std::sync::Once = std::sync::Once::new();
            TOLD.call_once(|| eprintln!("nyx-dock: the swipe plugin didn't load ({why}); rebuild it with install.sh"));
        }
    });
}

/// Call `on_event` on the main loop for each swipe the plugin reports. A thread holds
/// Hyprland's event socket, and finds Hyprland again after it restarts.
pub fn listen(on_event: impl Fn(SwipeEvent) + 'static) {
    let (tx, rx) = async_channel::unbounded::<SwipeEvent>();
    std::thread::Builder::new()
        .name("nyx-dock-swipe".into())
        .spawn(move || {
            let mut backoff = Duration::from_millis(100);
            loop {
                if let Some(stream) = Instance::find().ok().and_then(|i| UnixStream::connect(i.event_socket()).ok()) {
                    backoff = Duration::from_millis(100);
                    for line in BufReader::new(stream).lines() {
                        let Ok(line) = line else { break };
                        if line.starts_with("nyxswipe")
                            && let Some(event) = swipe::parse(&line)
                            && tx.send_blocking(event).is_err()
                        {
                            return;
                        }
                    }
                }
                std::thread::sleep(backoff);
                backoff = (backoff * 2).min(Duration::from_secs(3));
            }
        })
        .expect("spawn the swipe listener");
    glib::MainContext::default().spawn_local(async move {
        while let Ok(event) = rx.recv().await {
            on_event(event);
        }
    });
}
