#!/usr/bin/env bash
# Lance Doom en vue du dessus.
# Usage : ./run.sh [doom|doom2|CHEMIN.wad] [options de chocolate-doom…]
#   ex. : ./run.sh doom2 -warp 1 -skill 3
set -euo pipefail
cd "$(dirname "$0")"

[[ -x bin/chocolate-doom && -f doom2d.conf ]] || { echo "Lance d'abord ./install.sh" >&2; exit 1; }
# shellcheck source=/dev/null
source doom2d.conf

case "${1:-doom}" in
  doom)    wad="$DOOM_WAD";  [[ $# -gt 0 ]] && shift ;;
  doom2)   wad="$DOOM2_WAD"; shift ;;
  *.wad|*.WAD) wad="$(realpath "$1")"; shift ;;
  *)       wad="$DOOM_WAD" ;;
esac
[[ -n "$wad" && -f "$wad" ]] || { echo "WAD introuvable — relance ./install.sh --wad CHEMIN" >&2; exit 1; }

# Sous Hyprland, le pavé tactile est coupé pendant la frappe : impossible de
# viser en se déplaçant. On le réactive le temps de la partie, puis on remet
# la valeur d'origine (la configuration sur disque n'est pas modifiée).
set_dwt() {
  # Configuration Lua (Hyprland récent) : eval ; configuration classique : keyword.
  hyprctl eval "hl.config({ input = { touchpad = { disable_while_typing = $1 } } })" 2>/dev/null | grep -q '^ok' \
    || hyprctl keyword input:touchpad:disable_while_typing "$1" >/dev/null
}
if command -v hyprctl >/dev/null && [[ -n "${HYPRLAND_INSTANCE_SIGNATURE:-}" ]]; then
  if hyprctl getoption input:touchpad:disable_while_typing -j 2>/dev/null | grep -q '"bool": *true'; then
    set_dwt false
    trap 'set_dwt true' EXIT
  fi
fi

LD_PRELOAD="$PWD/target/release/libdoom2d.so" bin/chocolate-doom -iwad "$wad" "$@"
