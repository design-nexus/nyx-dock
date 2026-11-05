#!/usr/bin/env bash
# Remove Nyx Dock and give the dock back to Omarchy.
#
#   ./uninstall.sh            remove the program and its service
#   ./uninstall.sh --purge    also remove its settings (~/.config/nyx/dock.toml)
set -euo pipefail
say() { printf '\033[1m==>\033[0m %s\n' "$*"; }

bin="$HOME/.local/bin"
config="${XDG_CONFIG_HOME:-$HOME/.config}"
if [[ -x $bin/nyx-dock ]]; then
  "$bin/nyx-dock" --disable || true
fi
rm -f "$bin/nyx-dock" "$config/systemd/user/nyx-dock.service"
plugin="$HOME/.local/lib/nyx-dock/nyx-dock-swipe.so"
if [[ -f $plugin ]]; then
  if hyprctl plugin list 2>/dev/null | grep -q "^Plugin nyx-dock-swipe "; then
    hyprctl plugin unload "$plugin" >/dev/null 2>&1 || true
  fi
  rm -f "$plugin"
  rmdir "$(dirname "$plugin")" 2>/dev/null || true
fi
systemctl --user daemon-reload 2>/dev/null || true
if [[ ${1:-} == --purge ]]; then
  rm -f "$config/nyx/dock.toml"
fi
say "Nyx Dock removed."
