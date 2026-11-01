#!/usr/bin/env bash
# Install Nyx Dock.
#
#   curl -fsSL https://raw.githubusercontent.com/design-nexus/nyx-dock/main/install.sh | bash
#
# Installing doesn't switch docks. Turn Nyx Dock on (and Omarchy's dock off) with
#   nyx-dock --enable
# and back with `nyx-dock --disable`.
set -euo pipefail

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
(cd "$src" && cargo build --release --locked -p nyx-dock)

bin="$HOME/.local/bin"
units="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
mkdir -p "$bin" "$units"
target="${CARGO_TARGET_DIR:-$src/target}/release"
# Replace the file rather than writing into it, so a running dock keeps its old binary
# until it restarts.
install -m 755 "$target/nyx-dock" "$bin/nyx-dock.new" && mv -f "$bin/nyx-dock.new" "$bin/nyx-dock"
install -m 644 "$src/apps/dock/data/nyx-dock.service" "$units/nyx-dock.service"
systemctl --user daemon-reload 2>/dev/null || true

if systemctl --user is-active --quiet nyx-dock.service 2>/dev/null; then
  say "Installed. The running dock uses the new build after: systemctl --user restart nyx-dock"
else
  say "Installed. Turn it on (and Omarchy's dock off) with: nyx-dock --enable"
fi
case ":$PATH:" in
  *":$bin:"*) ;;
  *) warn "$bin isn't on your PATH." ;;
esac
