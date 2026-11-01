//! The dock's logic, kept apart from GTK so it can be tested on its own.
//!
//! - [`config`]: `~/.config/nyx/dock.toml`, with per-monitor overrides.
//! - [`apps`]: which installed app a window belongs to.
//! - [`items`]: what the dock shows: pinned apps, then running ones.
//! - [`hide`]: when intelligent hiding tucks the dock away.

pub mod apps;
pub mod config;
pub mod hide;
pub mod items;
