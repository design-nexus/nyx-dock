# Dock

A floating app dock for [Omarchy](https://omarchy.org) and Hyprland. It sits at the
bottom of each screen, steps aside when a window needs the space, and takes its colors
from your Omarchy theme.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/design-nexus/nyx-dock/main/install.sh | bash
```

Installing doesn't replace Omarchy's dock. Switch with:

```sh
nyx-dock --enable    # start Nyx Dock now and at login, turn Omarchy's dock off
nyx-dock --disable   # stop it and turn Omarchy's dock back on
```

`--enable` and `--disable` use Omarchy's own `omarchy-plugin-disable` and
`omarchy-plugin-enable`, so nothing of Omarchy's is edited by hand.

## What it does

- Pinned apps first, then the apps that are running, in the order they started.
- Click an app to open it, or to bring its most recent window forward (the one on the
  workspace in front of you if it has one). Click it again to go through its other
  windows. Middle-click opens a new window.
- A dot under a running app, two for more than one window; the app you're in gets an
  accent bar. Minimized windows come back to the workspace you're on.
- Badges and progress from apps that use the Unity launcher API, and a warning-colored
  dot for a window asking for attention.
- Right-click for the app's own actions (New Private Window, Compose, …), New window,
  Keep in dock / Remove from dock, and Quit or Close all windows.
- Intelligent hiding: the dock hides while a window overlaps it or something is
  fullscreen, and comes back when the pointer reaches the screen edge. It stays up while
  a menu is open.
- One dock per screen, each with its own settings if you want them.
- Windows on internal workspaces (tests, scratch outputs) are left out.

Apps start the way Omarchy starts them (`uwsm-app -- gtk-launch`), each in its own scope,
so restarting the dock never closes them.

## Settings

`~/.config/nyx/dock.toml`, written on first start. On first start the pins come from your
Omarchy dock if you set any there, otherwise your browser and the Nyx apps you have.

```toml
icon_size = 48          # 24 to 96
hide = "smart"          # smart | always | never
running = "all"         # all | monitor (only apps with a window on that screen)
position = "bottom"     # bottom | top | left | right
hide_delay_ms = 600
pinned = ["io.github.design_nexus.nyx.Files", "firefox"]

# One screen: by its stable id (make|model|serial) or its connector name.
[monitors."DP-2"]
enabled = false

[monitors."Samsung Display Corp.|ATNA60DL01-0|"]
icon_size = 40
pinned = ["io.github.design_nexus.nyx.Files"]
```

Pins are desktop ids without `.desktop`. The dock picks up changes to the file as you save.
Headless (virtual) outputs get no dock unless a `[monitors]` section turns one on.

## Build

```sh
cargo build --release --locked -p nyx-dock
cargo test --locked --workspace
```

`nyx-dock --only HEADLESS-1` shows the dock on one output only, and `NYX_DOCK_DEBUG=1`
prints what it decides. The service is `nyx-dock.service` (systemd user unit).

## Remove

```sh
./uninstall.sh            # gives the dock back to Omarchy
./uninstall.sh --purge    # also removes ~/.config/nyx/dock.toml
```

## License

MIT
