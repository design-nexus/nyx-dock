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
systemctl --user daemon-reload 2>/dev/null || true
if [[ ${1:-} == --purge ]]; then
  rm -f "$config/nyx/dock.toml"
fi
say "Nyx Dock removed."
