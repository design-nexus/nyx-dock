//! The dock's logic, kept apart from GTK so it can be tested on its own.
//!
//! - [`config`]: `~/.config/nyx/dock.toml`, with per-monitor overrides.
//! - [`card`]: what an app's card lists (events, folders, bookmarks).
//! - [`items`]: what the dock shows: pinned apps, then running ones.
//! - [`hide`]: when intelligent hiding tucks the dock away.
//! - [`swipe`]: following a workspace swipe.

pub mod card;
pub mod config;
pub mod files;
pub mod hide;
pub mod items;
pub mod swipe;
