# Dock

Application source: apps/dock (GTK, layer-shell). Logic without GTK: crates/nyx-dock-core
(settings, matching windows to apps, items, hiding) — test it there.
Desktop state (Hyprland, monitors, windows) comes from nyx-desktop in nyx-core.

Commands: `cargo test --locked --workspace`; `cargo clippy --workspace --all-targets`;
`./install.sh`. Try it without touching the real dock or settings:
`XDG_CONFIG_HOME=<scratch> nyx-dock --only HEADLESS-<n>` on a headless output
(`NYX_DOCK_DEBUG=1` prints its decisions). Never inject input or steal focus.

Installing never switches docks: `nyx-dock --enable` / `--disable` does, through
Omarchy's own `omarchy-plugin-disable/enable omarchy.dock`. Apps start through
`uwsm-app -- gtk-launch` so they outlive the dock.

Use the public, pinned shared crates; never import another application's internals.
Use canonical STYLE.md from the pinned nyx-design dependency. Reuse existing
widgets, theme tokens, icons, and shortcuts.
Common guidance: https://github.com/design-nexus/nyx/blob/main/docs/AGENTS_COMMON.md
