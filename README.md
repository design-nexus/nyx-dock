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

The dock is drawn in Quickshell (the shared Nyx runtime, `nyx-shell-qs`, with the dock's
services in `nyx-dock-backend`); the earlier GTK dock is still there for now:

```sh
nyx-dock --ui gtk    # back to the GTK dock
nyx-dock --ui qml    # the Quickshell dock (the default)
nyx-dock --ui        # which one is on
```

## What it does

- The Apps button first, then pinned apps, then the apps that are running, in the order
  they started; then your folders (Downloads by default) and the Trash.
- The Apps button opens every app from the app menu in a grid above the dock. Type to
  search (Enter opens the first match), click an app to open it, right-click it to keep
  it in the dock or uninstall it, or drag it onto the dock to pin it in that place.
  Uninstall hands the app to Nyx Store, which shows it with its Remove button and
  reviews the removal as usual; it's there when Nyx Store is installed.
- Click an app to open it, or to bring its most recent window forward (the one on the
  workspace in front of you if it has one). Click it again to go through its other
  windows. Middle-click opens a new window.
- Rest the pointer on a running app to see its windows, including those on other
  workspaces and minimized ones; click one to go to it, or close it from its card.
- A dot under a running app, two for more than one window; the app you're in gets an
  accent bar. Minimized windows come back to the workspace you're on.
- Badges and progress from apps that use the Unity launcher API, and a warning-colored
  dot for a window asking for attention.
- Right-click an app for its card: what the app has for you, its windows (with more
  than one), its own actions (New Private Window, Compose, …), Keep in Dock / Remove
  from Dock, Show windows (with Nyx Switcher: the app's windows in its overview), and
  Quit or Close all. A player (Nyx Music, a browser playing video, …) also shows what's
  playing with its controls. What the card lists, read from what the apps already keep:
  - Nyx Calendar: the next five events (from its agenda), each opening its day, with
    Today and Agenda.
  - Your file manager: your bookmarked folders, then the standard ones.
  - Your browser: the bookmarks toolbar, or the sites you visit most without one
    (Firefox, LibreWolf, Zen, Chromium, Chrome, Brave, Vivaldi, Edge).
  - Nyx Music: its Recently played, Most played, Recently added, Radio and Podcasts.
  - Any other app: the files it opened lately.
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
  right-click a folder to choose, and to show or hide its count. Grids and lists show
  every file and scroll. Click a file to open it, or drag it out to another app.
  Removing a folder from the dock never touches the folder.
- Trash shows when it has something in it, opens in your file manager, and empties from
  its menu after a second click.
- Intelligent hiding: the dock hides while a window overlaps it or something is
  fullscreen, and comes back when the pointer reaches the screen edge. It stays up while
  a menu, preview or drag is open. It slides out of sight and back, at once when you
  switch to a workspace whose windows are in its way, and follows a trackpad swipe as
  it goes: out before the windows coming in reach it, back once the ones leaving have
  passed. Between two workspaces that both hide it, it stays hidden.
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

Hyprland doesn't tell other programs how far a swipe has gone, so `install.sh` also
builds a small Hyprland plugin (`hyprland-plugin/`, installed as
`~/.local/lib/nyx-dock/nyx-dock-swipe.so`) that only reads where the workspaces are
drawn and reports it. The dock loads it, and loads it again after Hyprland reloads its
config. It's built against the Hyprland on your machine: after a Hyprland update, run
`./install.sh` again. Without it the dock still hides and comes back, once a swipe ends.

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
folder_counts = true    # item count badges on folders
apps = true             # the Apps button
app_names = true        # names under the icons in the Apps panel (false: icons only)
pinned = ["io.github.design_nexus.nyx.Files", "firefox"]

[[folders]]
path = "~/Downloads"
look = "stack"          # stack | folder
open = "grid"           # grid | fan | list | files
sort = "date"           # date | name | kind
count = false           # this folder's count badge (unset: folder_counts)

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
cargo build --release --locked -p nyx-dock -p nyx-dock-backend
cargo test --locked --workspace
make -C hyprland-plugin    # the swipe plugin (needs Hyprland's headers)
```

`nyx-dock --only HEADLESS-1` shows the dock on one output only, and `NYX_DOCK_DEBUG=1`
prints what it decides. For screenshots without a pointer, `NYX_DOCK_TEST_PREVIEW=<app>`,
`NYX_DOCK_TEST_CARD=<app>`, `NYX_DOCK_TEST_FOLDER=1` (or `=menu`), `NYX_DOCK_TEST_APPS=1`
and `NYX_DOCK_TEST_DOCK_MENU=1` open a preview, an app's card, the first folder (or its
menu), the Apps panel or the dock's menu at start; `NYX_DOCK_TEST_OFFSET=0.5` holds the
bar that far through its slide out of sight. The service is `nyx-dock.service`
(systemd user unit).

The Quickshell dock's QML is in `qml/Dock` (`qml/runtime` is a copy of the shared runtime,
from nyx-shell's `tools/vendor-runtime.sh`). To try it on a test output, run
`NYX_DOCK_ONLY=HEADLESS-1 NYX_DOCK_UI_SOCKET=/tmp/dock.sock nyx-dock-backend` and load
`qml/Dock` in `qs` with the same `NYX_DOCK_UI_SOCKET`, `NYX_SHELL_SCREEN=HEADLESS-1`
and `NYX_SHELL_NO_KEYBOARD=1`; the `NYX_DOCK_TEST_*` hooks above work there too, and
`NYX_DOCK_TEST_CONTROLS=open` opens the window controls.

## Remove

```sh
./uninstall.sh            # gives the dock back to Omarchy
./uninstall.sh --purge    # also removes ~/.config/nyx/dock.toml
```

## License

MIT
