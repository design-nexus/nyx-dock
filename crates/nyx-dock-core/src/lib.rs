//! The dock's logic, kept apart from GTK so it can be tested on its own.
//!
//! - [`config`]: `~/.config/nyx/dock.toml`, with per-monitor overrides.
//! - [`items`]: what the dock shows: pinned apps, then running ones.
//! - [`hide`]: when intelligent hiding tucks the dock away.

pub mod config;
pub mod files;
pub mod hide;
pub mod items;
