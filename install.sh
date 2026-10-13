#!/usr/bin/env bash
# Installe 2dDOOM : compile Chocolate Doom 3.1.1 et le mod, puis trouve les WAD.
#
# Usage : ./install.sh [--deps] [--wad CHEMIN] [--wad2 CHEMIN]
#   --deps        installe les dépendances système manquantes (via sudo)
#   --wad CHEMIN  DOOM.WAD (ou doom1.wad, freedoom1.wad…) à utiliser
#   --wad2 CHEMIN DOOM2.WAD (ou freedoom2.wad…) à utiliser
set -euo pipefail
cd "$(dirname "$0")"

CHOCOLATE_TAG=chocolate-doom-3.1.1
CHOCOLATE_REPO=https://github.com/chocolate-doom/chocolate-doom.git

say()  { printf '\033[1;32m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1;33m==>\033[0m %s\n' "$*" >&2; }
die()  { printf '\033[1;31m==>\033[0m %s\n' "$*" >&2; exit 1; }

install_deps=0
wad="" wad2=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --deps) install_deps=1 ;;
    --wad)  wad="$(realpath "$2")"; shift ;;
    --wad2) wad2="$(realpath "$2")"; shift ;;
    -h|--help) sed -n '2,8p' "$0"; exit 0 ;;
    *) die "option inconnue : $1 (voir --help)" ;;
  esac
  shift
done

[[ "$(uname -s)" == Linux && "$(uname -m)" == x86_64 ]] \
  || die "seul Linux x86_64 est pris en charge (le mod lit la mémoire du jeu au format 64 bits)"

# --- Dépendances ---------------------------------------------------------------

missing=()
for tool in git cmake make cc pkg-config; do
  command -v "$tool" >/dev/null || missing+=("$tool")
done
for lib in sdl2 SDL2_mixer SDL2_net; do
  pkg-config --exists "$lib" 2>/dev/null || missing+=("$lib")
done

if [[ ${#missing[@]} -gt 0 ]]; then
  if command -v pacman >/dev/null; then
    pkgs=(git cmake base-devel pkgconf sdl2 sdl2_mixer sdl2_net)
    cmd=(sudo pacman -S --needed "${pkgs[@]}")
  elif command -v apt-get >/dev/null; then
    pkgs=(git cmake build-essential pkg-config libsdl2-dev libsdl2-mixer-dev libsdl2-net-dev)
    cmd=(sudo apt-get install -y "${pkgs[@]}")
  elif command -v dnf >/dev/null; then
    pkgs=(git cmake gcc make pkgconf-pkg-config SDL2-devel SDL2_mixer-devel SDL2_net-devel)
    cmd=(sudo dnf install -y "${pkgs[@]}")
  else
    die "dépendances manquantes : ${missing[*]} — installe-les avec le gestionnaire de paquets de ta distribution"
  fi
  if [[ $install_deps == 1 ]]; then
    say "Installation des dépendances : ${cmd[*]}"
    "${cmd[@]}"
  else
    die "dépendances manquantes : ${missing[*]}
    Installe-les avec :  ${cmd[*]}
    ou relance :         ./install.sh --deps"
  fi
fi

if ! command -v cargo >/dev/null; then
  die "Rust (cargo) est introuvable — installe-le via https://rustup.rs puis relance ce script"
fi

# --- Chocolate Doom ------------------------------------------------------------

mkdir -p build bin
if [[ ! -d build/chocolate-doom ]]; then
  say "Téléchargement de Chocolate Doom ($CHOCOLATE_TAG)"
  git clone -q --depth 1 --branch "$CHOCOLATE_TAG" "$CHOCOLATE_REPO" build/chocolate-doom
fi

say "Compilation de Chocolate Doom (avec ses symboles, dont le mod a besoin)"
log=build/chocolate-doom.log
{
  cmake -S build/chocolate-doom -B build/chocolate-doom/build -DCMAKE_BUILD_TYPE=RelWithDebInfo &&
  cmake --build build/chocolate-doom/build --target chocolate-doom -j"$(nproc)"
} >"$log" 2>&1 || { tail -20 "$log" >&2; die "échec de la compilation de Chocolate Doom (journal : $log)"; }
cp build/chocolate-doom/build/src/chocolate-doom bin/chocolate-doom

# --- Mod -----------------------------------------------------------------------

say "Compilation du mod"
cargo build --release --quiet

# --- WAD -----------------------------------------------------------------------

# Premier fichier existant parmi les candidats (insensible à la casse du nom).
find_wad() {
  local candidate
  for candidate in "$@"; do
    local dir="${candidate%/*}" name="${candidate##*/}"
    [[ -d "$dir" ]] || continue
    local found
    found=$(find "$dir" -maxdepth 1 -iname "$name" -print -quit 2>/dev/null)
    [[ -n "$found" ]] && { realpath "$found"; return 0; }
  done
  return 1
}

steam_roots=(
  "$HOME/.local/share/Steam/steamapps/common"
  "$HOME/.steam/steam/steamapps/common"
  "$HOME/.var/app/com.valvesoftware.Steam/.local/share/Steam/steamapps/common"
)
doom1=() doom2=()
for root in "${steam_roots[@]}"; do
  doom1+=("$root/Ultimate Doom/base/DOOM.WAD" "$root/Ultimate Doom/rerelease/DOOM.WAD")
  doom2+=("$root/Ultimate Doom/base/doom2/DOOM2.WAD" "$root/Doom 2/base/DOOM2.WAD" "$root/Ultimate Doom/rerelease/DOOM2.WAD")
done
for dir in "$HOME/GOG Games/DOOM" "$HOME/GOG Games/DOOM 2" "$HOME/.local/share/doom" /usr/share/games/doom /usr/share/doom /usr/local/share/games/doom; do
  doom1+=("$dir/DOOM.WAD" "$dir/doom1.wad" "$dir/freedoom1.wad")
  doom2+=("$dir/DOOM2.WAD" "$dir/freedoom2.wad")
done

[[ -n "$wad"  ]] || wad=$(find_wad "${doom1[@]}") || true
[[ -n "$wad2" ]] || wad2=$(find_wad "${doom2[@]}") || true

cat > doom2d.conf <<EOF
# Généré par install.sh — chemins des WAD utilisés par run.sh
DOOM_WAD="$wad"
DOOM2_WAD="$wad2"
EOF

[[ -n "$wad"  ]] && say "Doom   : $wad"  || warn "Doom : aucun WAD trouvé"
[[ -n "$wad2" ]] && say "Doom II : $wad2" || warn "Doom II : aucun WAD trouvé"
if [[ -z "$wad$wad2" ]]; then
  warn "Indique ton WAD avec : ./install.sh --wad /chemin/vers/DOOM.WAD"
  warn "(sans copie du jeu, Freedoom est gratuit : https://freedoom.github.io)"
  exit 1
fi

say "Installation terminée. Lance le jeu avec : ./run.sh   (ou ./run.sh doom2)"
