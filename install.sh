#!/usr/bin/env bash
# Install Nyx Dock.
#
#   curl -fsSL https://raw.githubusercontent.com/design-nexus/nyx-dock/main/install.sh | bash
#
# Installing doesn't switch docks. Turn Nyx Dock on (and Omarchy's dock off) with
#   nyx-dock --enable
# and back with `nyx-dock --disable`.
#
# Nyx Dock has a Quickshell interface (the default) and a GTK one; switch with
# `nyx-dock --ui qml|gtk`. The Quickshell one runs in the shared runtime, nyx-shell-qs.
#   NYX_UI=gtk|qml ./install.sh     choose the interface on a first install
set -euo pipefail

DEFAULT_UI=qml

REPO="design-nexus/nyx-dock"
say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[33m==>\033[0m %s\n' "$*" >&2; }
die() { printf '\033[31m==>\033[0m %s\n' "$*" >&2; exit 1; }

command -v hyprctl >/dev/null || warn "Hyprland wasn't found. Nyx Dock is made for Omarchy (Hyprland)."

if command -v pacman >/dev/null; then
  need=()
  for p in gtk4 gtk4-layer-shell pkgconf gcc git; do pacman -Qq "$p" >/dev/null 2>&1 || need+=("$p"); done
  if ((${#need[@]})); then
    say "Installing ${need[*]}"
    sudo pacman -S --needed --noconfirm "${need[@]}"
  fi
fi

# This checkout, or a fresh one when piped from curl.
here=""
[[ -n ${BASH_SOURCE[0]:-} && -f ${BASH_SOURCE[0]} ]] && here=$(cd "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
if [[ -n $here && -f $here/Cargo.toml && -d $here/apps/dock ]]; then
  src=$here
else
  say "Fetching the source"
  git clone -q --depth 1 "https://github.com/$REPO.git" "$work/src"
  src="$work/src"
fi

if ! command -v cargo >/dev/null; then
  command -v pacman >/dev/null || die "Rust (cargo) is needed to build Nyx Dock. Install it and run this again."
  say "Installing rust"
  sudo pacman -S --needed --noconfirm rust
fi
say "Building Nyx Dock (a few minutes the first time)"
(cd "$src" && cargo build --release --locked -p nyx-dock -p nyx-dock-backend)

bin="$HOME/.local/bin"
units="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
mkdir -p "$bin" "$units"
target="${CARGO_TARGET_DIR:-$src/target}/release"
# Replace the file rather than writing into it, so a running dock keeps its old binary
# until it restarts.
install -m 755 "$target/nyx-dock" "$bin/nyx-dock.new" && mv -f "$bin/nyx-dock.new" "$bin/nyx-dock"
install -m 755 "$target/nyx-dock-backend" "$bin/nyx-dock-backend.new" && mv -f "$bin/nyx-dock-backend.new" "$bin/nyx-dock-backend"
install -m 644 "$src/apps/dock/data/nyx-dock.service" "$units/nyx-dock.service"
install -m 644 "$src/apps/dock-backend/data/nyx-dock-backend.service" "$units/nyx-dock-backend.service"
# The Quickshell interface, and the shared runtime it runs in (kept if a newer one is in place).
data="${XDG_DATA_HOME:-$HOME/.local/share}"
mkdir -p "$data/nyx-dock"
rm -rf "$data/nyx-dock/qml.new"
cp -r "$src/qml/Dock" "$data/nyx-dock/qml.new"
rm -rf "$data/nyx-dock/qml" && mv "$data/nyx-dock/qml.new" "$data/nyx-dock/qml"
bash "$src/qml/runtime/install-runtime.sh" "$src/qml/runtime"
systemctl --user daemon-reload 2>/dev/null || true
# The interface: chosen once (NYX_UI, else the default), then whatever --ui last chose.
[[ -f $data/nyx-dock/ui ]] || "$bin/nyx-dock" --ui "${NYX_UI:-$DEFAULT_UI}" >/dev/null

# The Hyprland plugin that lets the dock slide with a workspace swipe. It's built
# against this Hyprland, so run this again after Hyprland updates.
plugins="$HOME/.local/lib/nyx-dock"
if pkg-config --exists hyprland 2>/dev/null && command -v g++ >/dev/null; then
  say "Building the swipe plugin for Hyprland $(pkg-config --modversion hyprland)"
  if make -s -C "$src/hyprland-plugin" clean all; then
    mkdir -p "$plugins"
    install -m 755 "$src/hyprland-plugin/nyx-dock-swipe.so" "$plugins/nyx-dock-swipe.so.new"
    mv -f "$plugins/nyx-dock-swipe.so.new" "$plugins/nyx-dock-swipe.so"
  else
    warn "The swipe plugin didn't build; the dock still hides, once a swipe ends."
  fi
else
  warn "Hyprland's headers weren't found, so the dock won't slide with workspace swipes."
fi

if [[ $("$bin/nyx-dock" --ui) == qml ]]; then
  say "Restarting the Quickshell dock"
  systemctl --user restart nyx-dock-backend.service 2>/dev/null || true
  systemctl --user restart nyx-shell-qs.service 2>/dev/null || true
elif systemctl --user is-active --quiet nyx-dock.service 2>/dev/null; then
  say "Installed. The running dock uses the new build after: systemctl --user restart nyx-dock"
else
  say "Installed. Turn it on (and Omarchy's dock off) with: nyx-dock --enable"
fi
case ":$PATH:" in
  *":$bin:"*) ;;
  *) warn "$bin isn't on your PATH." ;;
esac
