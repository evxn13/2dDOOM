#!/usr/bin/env bash
# Lance Doom avec la vue du dessus.
# Usage : ./run.sh [doom|doom2] [arguments chocolate-doom...]
set -euo pipefail
cd "$(dirname "$0")"
cargo build --release -q
case "${1:-doom}" in
  doom2) wad=wads/DOOM2.WAD; shift ;;
  doom)  wad=wads/DOOM.WAD; [[ $# -gt 0 ]] && shift ;;
  *)     wad=wads/DOOM.WAD ;;
esac

# Sous Hyprland, le pavé tactile est coupé pendant la frappe : impossible de
# viser en se déplaçant. On le réactive le temps de la partie, puis on remet
# la valeur d'origine (la config sur disque n'est pas touchée).
set_dwt() {
  # Config Lua (Hyprland récent) : eval ; config classique : keyword.
  hyprctl eval "hl.config({ input = { touchpad = { disable_while_typing = $1 } } })" 2>/dev/null | grep -q '^ok' \
    || hyprctl keyword input:touchpad:disable_while_typing "$1" >/dev/null
}
if command -v hyprctl >/dev/null && [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
  if hyprctl getoption input:touchpad:disable_while_typing -j 2>/dev/null | grep -q '"bool": *true'; then
    set_dwt false
    trap 'set_dwt true' EXIT
  fi
fi

LD_PRELOAD="$PWD/target/release/libtopdoom.so" bin/chocolate-doom-stripped -iwad "$wad" "$@"
