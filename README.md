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

- The Apps button first, then pinned apps, then the apps that are running, in the order
  they started; then your folders (Downloads by default) and the Trash.
- The Apps button opens every app from the app menu in a grid above the dock. Type to
  search (Enter opens the first match), click an app to open it, right-click it to keep
  it in the dock, or drag it onto the dock to pin it in that place.
- Click an app to open it, or to bring its most recent window forward (the one on the
  workspace in front of you if it has one). Click it again to go through its other
  windows. Middle-click opens a new window.
- Rest the pointer on a running app to see its windows, including those on other
  workspaces and minimized ones; click one to go to it, or close it from its card.
- A dot under a running app, two for more than one window; the app you're in gets an
  accent bar. Minimized windows come back to the workspace you're on.
- Badges and progress from apps that use the Unity launcher API, and a warning-colored
  dot for a window asking for attention.
- Right-click for the app's own actions (New Private Window, Compose, …), Keep in dock /
  Remove from dock, Show windows (with Nyx Switcher: the app's windows in its overview),
  and Quit or Close all windows. A player (Nyx Music, a browser
  playing video, …) also shows what's playing with its controls.
- Drag an app along the dock to pin it in that place; the other icons move aside as it
  goes. Drop a pinned app or a folder outside the dock to remove it. Drop files on an
  app to open them with it, on a folder to copy them there, on the Trash to trash them.
- Right-click the dock itself (between icons, or the Apps button) for its settings: icon
  size, position, hiding, window previews, the Apps button and the Trash, with buttons
  to add a folder and, with Nyx Settings, to open its Dock page.
- Add a folder from the dock's menu, or drag one from your file manager onto the dock
  (on the line before the folders, or between them).
- Folders show their newest files stacked (or the folder icon: with the Umbra icon
  theme, the folder's own, such as Downloads, Games or Dropbox) with a count, and open
  as a fan, a grid, a list or in your file manager, sorted by date, name or kind:
  right-click a folder to choose. Grids and lists show every file and scroll. Click a
  file to open it, or drag it out to another app. Removing a folder from the dock never
  touches the folder.
- Trash shows when it has something in it, opens in your file manager, and empties from
  its menu after a second click.
- Intelligent hiding: the dock hides while a window overlaps it or something is
  fullscreen, and comes back when the pointer reaches the screen edge. It stays up while
  a menu, preview or drag is open.
- Hide set to Never keeps the dock up on floating workspaces and keeps its space, as on
  macOS: maximized windows stop above it, and a floating window that reaches behind it
  moves up (or gets smaller) when windows open or you switch workspaces. On tiling and
  scrolling workspaces, which windows fill, it hides as Smart does.
- One dock per screen, each with its own settings if you want them.
- Windows on internal workspaces (tests, scratch outputs) are left out.

Window previews use the standard Wayland capture protocols (`ext-image-copy-capture`), so
nothing extra is installed; a window that hasn't been on screen since it opened shows its
last drawn frame. Folder thumbnails come from the shared thumbnail cache, and images
without one get one made and saved there.

Apps start the way Omarchy starts them (`uwsm-app -- gtk-launch`), each in its own scope,
so restarting the dock never closes them.

## Settings

`~/.config/nyx/dock.toml`, written on first start. On first start the pins come from your
Omarchy dock if you set any there, otherwise your browser and the Nyx apps you have.

```toml
icon_size = 48          # 24 to 96
hide = "smart"          # smart | always | never (never: on floating workspaces)
running = "all"         # all | monitor (only apps with a window on that screen)
position = "bottom"     # bottom | top | left | right
hide_delay_ms = 600
previews = true         # windows of a running app on hover
preview_delay_ms = 350
trash = true
apps = true             # the Apps button
app_names = true        # names under the icons in the Apps panel (false: icons only)
pinned = ["io.github.design_nexus.nyx.Files", "firefox"]

[[folders]]
path = "~/Downloads"
look = "stack"          # stack | folder
open = "grid"           # grid | fan | list | files
sort = "date"           # date | name | kind

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
prints what it decides. For screenshots without a pointer, `NYX_DOCK_TEST_PREVIEW=<app>`,
`NYX_DOCK_TEST_MENU=<app>`, `NYX_DOCK_TEST_FOLDER=1` (or `=menu`), `NYX_DOCK_TEST_APPS=1`
and `NYX_DOCK_TEST_DOCK_MENU=1` open a preview, an app's menu, the first folder (or its
menu), the Apps panel or the dock's menu at start. The service is `nyx-dock.service`
(systemd user unit).

## Remove

```sh
./uninstall.sh            # gives the dock back to Omarchy
./uninstall.sh --purge    # also removes ~/.config/nyx/dock.toml
```

## License

MIT
