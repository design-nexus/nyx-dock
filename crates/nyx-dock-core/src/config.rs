//! The dock's settings: `~/.config/nyx/dock.toml`.
//!
//! ```toml
//! icon_size = 48
//! hide = "smart"          # smart | always | never
//! previews = true         # windows of a running app on hover
//! running = "all"         # all | monitor: running apps from every monitor or this one
//! pinned = ["io.github.design_nexus.nyx.Files", "firefox"]
//! apps = true             # the Apps button first
//! app_names = true        # names under the icons in the Apps panel
//! window_controls = false # hover buttons on floating windows
//!
//! [monitors."Samsung Display Corp.|ATNA60DL01-0|"]   # a monitor's stable id, or its connector
//! icon_size = 40
//!
//! [monitors."DP-2"]
//! enabled = false
//! ```
//!
//! Pinned apps are desktop ids without `.desktop`. A monitor section overrides only what
//! it sets. Virtual (headless) outputs have no dock unless a section enables one.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Position {
    #[default]
    Bottom,
    Left,
    Right,
    Top,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Hide {
    /// Hide while a window overlaps the dock; show at the screen edge.
    #[default]
    Smart,
    /// Always hidden until the pointer reaches the edge. Settings once wrote this as
    /// `auto`, which is read the same way.
    #[serde(alias = "auto")]
    Always,
    /// Always shown.
    Never,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Running {
    /// Apps running on any monitor.
    #[default]
    All,
    /// Only apps with a window on this monitor.
    Monitor,
}

pub const ICON_SIZES: std::ops::RangeInclusive<u32> = 24..=96;

/// How a folder looks in the dock.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FolderLook {
    /// Its newest files, stacked.
    #[default]
    Stack,
    /// The folder icon.
    Folder,
}

/// What clicking a folder does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FolderOpen {
    #[default]
    Grid,
    Fan,
    List,
    /// Open the folder in the file manager.
    Files,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    /// Newest first.
    #[default]
    Date,
    Name,
    Kind,
}

/// A folder in the dock (a shortcut: removing it never touches the folder).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Folder {
    /// `~` stands for the home folder.
    pub path: String,
    pub look: FolderLook,
    pub open: FolderOpen,
    pub sort: Sort,
    /// An icon name to show instead (with `look = "folder"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

impl Default for Folder {
    fn default() -> Self {
        Folder { path: "~/Downloads".into(), look: FolderLook::Stack, open: FolderOpen::Grid, sort: Sort::Date, icon: None }
    }
}

impl Folder {
    pub fn expanded(&self, home: &Path) -> PathBuf {
        match self.path.strip_prefix("~/") {
            Some(rest) => home.join(rest),
            None if self.path == "~" => home.to_path_buf(),
            None => PathBuf::from(&self.path),
        }
    }

    /// A folder at `path`, written with `~` when it's in the home folder.
    pub fn at(path: &Path, home: &Path) -> Folder {
        let path = match path.strip_prefix(home) {
            Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => path.display().to_string(),
        };
        Folder { path, ..Default::default() }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub icon_size: u32,
    pub hide: Hide,
    pub running: Running,
    pub position: Position,
    /// Milliseconds before the dock hides once nothing keeps it up.
    pub hide_delay_ms: u32,
    /// Show a running app's windows when the pointer rests on it.
    pub previews: bool,
    /// Milliseconds the pointer rests on an app before its windows preview.
    pub preview_delay_ms: u32,
    pub pinned: Vec<String>,
    /// Folders after the apps (Downloads by default).
    pub folders: Vec<Folder>,
    /// The Trash at the end.
    pub trash: bool,
    /// The Apps button first, which opens every app in a grid.
    pub apps: bool,
    /// Names under the icons in the Apps panel (off: icons only).
    pub app_names: bool,
    /// Hover buttons (minimize, maximize, close) on the focused floating window, for
    /// when Omarchy's floating titlebar is off. Settings sets it.
    pub window_controls: bool,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub monitors: BTreeMap<String, MonitorConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            icon_size: 48,
            hide: Hide::Smart,
            running: Running::All,
            position: Position::Bottom,
            hide_delay_ms: 600,
            previews: true,
            preview_delay_ms: 350,
            pinned: Vec::new(),
            folders: vec![Folder::default()],
            trash: true,
            apps: true,
            app_names: true,
            window_controls: false,
            monitors: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct MonitorConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pinned: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_size: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hide: Option<Hide>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub running: Option<Running>,
}

/// The settings that apply to one monitor.
#[derive(Debug, Clone, PartialEq)]
pub struct Effective {
    pub enabled: bool,
    pub pinned: Vec<String>,
    pub icon_size: u32,
    pub position: Position,
    pub hide: Hide,
    pub running: Running,
}

impl Config {
    /// The section for a monitor: by stable id first, then by connector name.
    fn section(&self, stable_id: &str, connector: &str) -> Option<(&String, &MonitorConfig)> {
        self.monitors.get_key_value(stable_id).or_else(|| self.monitors.get_key_value(connector))
    }

    pub fn for_monitor(&self, stable_id: &str, connector: &str, is_virtual: bool) -> Effective {
        let m = self.section(stable_id, connector).map(|(_, m)| m.clone()).unwrap_or_default();
        Effective {
            enabled: m.enabled.unwrap_or(!is_virtual),
            pinned: m.pinned.unwrap_or_else(|| self.pinned.clone()),
            icon_size: m.icon_size.unwrap_or(self.icon_size).clamp(*ICON_SIZES.start(), *ICON_SIZES.end()),
            position: m.position.unwrap_or(self.position),
            hide: m.hide.unwrap_or(self.hide),
            running: m.running.unwrap_or(self.running),
        }
    }

    /// Pin or unpin an app where this monitor takes its pins from: its own list if its
    /// section has one, else the shared list.
    pub fn set_pinned(&mut self, stable_id: &str, connector: &str, app: &str, pinned: bool, at: Option<usize>) {
        let key = self.section(stable_id, connector).filter(|(_, m)| m.pinned.is_some()).map(|(k, _)| k.clone());
        let list = match key {
            Some(k) => self.monitors.get_mut(&k).and_then(|m| m.pinned.as_mut()).expect("section has pins"),
            None => &mut self.pinned,
        };
        list.retain(|p| p != app);
        if pinned {
            let at = at.unwrap_or(list.len()).min(list.len());
            list.insert(at, app.to_string());
        }
    }

    /// This monitor's section, if it has one.
    fn section_mut(&mut self, stable_id: &str, connector: &str) -> Option<&mut MonitorConfig> {
        let key = self.section(stable_id, connector).map(|(k, _)| k.clone())?;
        self.monitors.get_mut(&key)
    }

    /// Set the icon size where this monitor takes it from: its section if that sets
    /// one, else the shared setting. Likewise [`Config::set_position`] and
    /// [`Config::set_hide`].
    pub fn set_icon_size(&mut self, stable_id: &str, connector: &str, size: u32) {
        let size = size.clamp(*ICON_SIZES.start(), *ICON_SIZES.end());
        match self.section_mut(stable_id, connector).filter(|m| m.icon_size.is_some()) {
            Some(m) => m.icon_size = Some(size),
            None => self.icon_size = size,
        }
    }

    pub fn set_position(&mut self, stable_id: &str, connector: &str, position: Position) {
        match self.section_mut(stable_id, connector).filter(|m| m.position.is_some()) {
            Some(m) => m.position = Some(position),
            None => self.position = position,
        }
    }

    pub fn set_hide(&mut self, stable_id: &str, connector: &str, hide: Hide) {
        match self.section_mut(stable_id, connector).filter(|m| m.hide.is_some()) {
            Some(m) => m.hide = Some(hide),
            None => self.hide = hide,
        }
    }

    /// Add a folder at `at` among the folders (the end if `None`). A folder already in
    /// the dock isn't added twice; returns whether it was added.
    pub fn add_folder(&mut self, path: &Path, home: &Path, at: Option<usize>) -> bool {
        if self.folders.iter().any(|f| f.expanded(home) == path) {
            return false;
        }
        let at = at.unwrap_or(self.folders.len()).min(self.folders.len());
        self.folders.insert(at, Folder::at(path, home));
        true
    }

    pub fn path() -> PathBuf {
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".config"));
        config.join("nyx/dock.toml")
    }

    /// Read the file. A missing file is `None`; a broken one is the defaults, with the
    /// error, so the dock still starts.
    pub fn load(path: &Path) -> Option<Result<Config, String>> {
        let text = std::fs::read_to_string(path).ok()?;
        Some(toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display())))
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).unwrap_or_default()
    }
}

/// The apps pinned on first start: the user's own Omarchy dock pins if they set any,
/// else the browser and the Nyx apps that are installed.
pub fn first_pins(omarchy_pins: Option<Vec<String>>, browser: Option<&str>, installed: impl Fn(&str) -> bool) -> Vec<String> {
    if let Some(pins) = omarchy_pins.filter(|p| !p.is_empty()) {
        return pins.into_iter().filter(|p| installed(p)).collect();
    }
    let mut pins: Vec<String> = Vec::new();
    for app in ["Files", "Mail", "Calendar", "Notes", "Music"] {
        pins.push(format!("io.github.design_nexus.nyx.{app}"));
    }
    if let Some(b) = browser {
        pins.insert(1, b.trim_end_matches(".desktop").to_string());
    }
    pins.push("io.github.design_nexus.nyx.Settings".into());
    pins.retain(|p| installed(p));
    pins
}

#[cfg(test)]
mod tests {
    use super::*;

    const FILE: &str = r#"
icon_size = 52
hide = "always"
pinned = ["a", "b"]

[monitors."Samsung|Panel|"]
icon_size = 40
pinned = ["c"]

[monitors."DP-2"]
enabled = false

[monitors."HEADLESS-20"]
enabled = true
running = "monitor"
"#;

    #[test]
    fn monitor_sections_override_what_they_set() {
        let c: Config = toml::from_str(FILE).unwrap();
        let laptop = c.for_monitor("Samsung|Panel|", "eDP-1", false);
        assert_eq!((laptop.icon_size, laptop.hide, laptop.enabled), (40, Hide::Always, true));
        assert_eq!(laptop.pinned, vec!["c"]);
        let dp = c.for_monitor("LG|27UK|123", "DP-2", false);
        assert!(!dp.enabled, "found by connector");
        assert_eq!(dp.pinned, vec!["a", "b"]);
        let other = c.for_monitor("X|Y|", "HDMI-A-1", false);
        assert_eq!((other.icon_size, other.enabled), (52, true));
    }

    #[test]
    fn virtual_outputs_need_a_section() {
        let c: Config = toml::from_str(FILE).unwrap();
        assert!(!c.for_monitor("HEADLESS-3", "HEADLESS-3", true).enabled);
        let h = c.for_monitor("HEADLESS-20", "HEADLESS-20", true);
        assert!(h.enabled);
        assert_eq!(h.running, Running::Monitor);
    }

    #[test]
    fn defaults_and_bad_files() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c, Config::default());
        assert_eq!(c.for_monitor("x", "y", false).icon_size, 48);
        let huge: Config = toml::from_str("icon_size = 500").unwrap();
        assert_eq!(huge.for_monitor("x", "y", false).icon_size, 96);
        assert!(toml::from_str::<Config>("hide = \"sometimes\"").is_err());
        assert_eq!(toml::from_str::<Config>("hide = \"auto\"").unwrap().hide, Hide::Always);
    }

    #[test]
    fn pinning_goes_to_the_list_the_monitor_uses() {
        let mut c: Config = toml::from_str(FILE).unwrap();
        c.set_pinned("Samsung|Panel|", "eDP-1", "d", true, Some(0));
        assert_eq!(c.monitors["Samsung|Panel|"].pinned.as_deref(), Some(&["d".to_string(), "c".to_string()][..]));
        assert_eq!(c.pinned, vec!["a", "b"]);
        c.set_pinned("X|Y|", "HDMI-A-1", "a", false, None);
        assert_eq!(c.pinned, vec!["b"]);
        c.set_pinned("X|Y|", "HDMI-A-1", "b", true, Some(9));
        assert_eq!(c.pinned, vec!["b"], "pinning twice keeps one");
    }

    #[test]
    fn folders() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c.folders, vec![Folder::default()], "Downloads by default");
        assert!(c.trash);
        let c: Config = toml::from_str(
            "trash = false\n[[folders]]\npath = \"~/Projects\"\nlook = \"folder\"\nopen = \"list\"\nsort = \"name\"",
        )
        .unwrap();
        assert_eq!(c.folders.len(), 1);
        assert_eq!(c.folders[0].open, FolderOpen::List);
        assert_eq!(c.folders[0].expanded(Path::new("/home/k")), PathBuf::from("/home/k/Projects"));
        let none: Config = toml::from_str("folders = []").unwrap();
        assert!(none.folders.is_empty(), "an empty list means no folders");
    }

    #[test]
    fn adding_folders() {
        let home = Path::new("/home/k");
        let mut c = Config::default();
        assert!(c.apps, "the Apps button by default");
        assert!(!c.add_folder(Path::new("/home/k/Downloads"), home, None), "Downloads is already there");
        assert!(c.add_folder(Path::new("/home/k/Projects"), home, Some(0)));
        assert!(c.add_folder(Path::new("/srv/media"), home, Some(9)));
        let paths: Vec<&str> = c.folders.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, ["~/Projects", "~/Downloads", "/srv/media"]);
        assert_eq!(Folder::at(home, home).path, "~");
    }

    #[test]
    fn look_goes_where_the_monitor_takes_it_from() {
        let mut c: Config = toml::from_str(FILE).unwrap();
        c.set_icon_size("Samsung|Panel|", "eDP-1", 64);
        assert_eq!((c.monitors["Samsung|Panel|"].icon_size, c.icon_size), (Some(64), 52));
        c.set_icon_size("X|Y|", "HDMI-A-1", 500);
        assert_eq!(c.icon_size, 96);
        c.set_position("Samsung|Panel|", "eDP-1", Position::Left);
        assert_eq!((c.position, c.monitors["Samsung|Panel|"].position), (Position::Left, None));
        c.set_hide("X|Y|", "DP-2", Hide::Never);
        assert_eq!(c.hide, Hide::Never);
    }

    #[test]
    fn round_trip() {
        let c: Config = toml::from_str(FILE).unwrap();
        let again: Config = toml::from_str(&c.to_toml()).unwrap();
        assert_eq!(c, again);
    }

    #[test]
    fn first_pins_prefer_the_users_omarchy_pins() {
        let installed = |id: &str| id != "gone";
        assert_eq!(first_pins(Some(vec!["firefox".into(), "gone".into()]), None, installed), vec!["firefox"]);
        let nyx = first_pins(None, Some("firefox.desktop"), installed);
        assert_eq!(nyx[0], "io.github.design_nexus.nyx.Files");
        assert_eq!(nyx[1], "firefox");
        assert_eq!(nyx.last().unwrap(), "io.github.design_nexus.nyx.Settings");
    }
}
