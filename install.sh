#!/bin/bash
# Showtime Suite on Linux: update this checkout, build and install - in one go, no sudo
# needed for the app itself.
#
#   git clone https://github.com/Cheter0410/showtime-suite.git && cd showtime-suite && ./install.sh
#
# Run it again later to update; only what is needed is done. The app goes into
# the folder all show tools share (~/.local/share/showtime-suite/apps/showtime-suite, as
# Showtime Suite installs it), with a menu entry and the command "showtime-suite".
# Remove with: ./install.sh --uninstall
set -euo pipefail
cd "$(dirname "$0")"
# shellcheck source=scripts/ui.sh
. scripts/ui.sh
# shellcheck source=scripts/setup.sh
. scripts/setup.sh
setup_no_root

if [ "${1:-}" = "--uninstall" ]; then
  ui_header "Showtime Suite" "Entfernen"
  user_app_remove showtime-suite showtime-suite && ui_ok "Aus dem Showtime-Suite-Ordner entfernt"
  if system_copy_there showtime-suite; then
    ui_sudo
    ui_run "Alte Kopie in /usr/local entfernen" system_copy_remove showtime-suite
  fi
  ui_ok "Entfernt (Einstellungen und eigene Dateien bleiben)"
  exit 0
fi

# --- Neuester Stand (danach läuft die neue Fassung dieses Skripts) ---------
if [ -z "${SM_PULLED:-}" ]; then
  ui_header "Showtime Suite" "Installieren / Aktualisieren"
  if setup_pull; then SM_PULLED=1 exec bash "$(pwd)/$(basename "$0")" "$@"; fi
fi

# --- Werkzeuge: nur was fehlt -------------------------------------------------
missing=()
if command -v pacman >/dev/null 2>&1; then
  pkgs=(git base-devel gtk3 libxkbcommon-x11 libglvnd)
  command -v cargo >/dev/null 2>&1 || [ -x "$HOME/.cargo/bin/cargo" ] || pkgs+=(rust)
  while IFS= read -r p; do [ -n "$p" ] && missing+=("$p"); done < <(pacman -T "${pkgs[@]}" 2>/dev/null || true)
  install_pkgs() { sudo pacman -S --needed --noconfirm "$@"; }
elif command -v apt-get >/dev/null 2>&1; then
  for p in git build-essential pkg-config libgtk-3-dev libxkbcommon-x11-dev curl; do
    dpkg-query -W -f='${Status}' "$p" 2>/dev/null | grep -q "install ok installed" || missing+=("$p")
  done
  install_pkgs() { sudo apt-get update && sudo apt-get install -y "$@"; }
elif command -v dnf >/dev/null 2>&1; then
  for p in git gcc pkgconf-pkg-config gtk3-devel libxkbcommon-x11-devel curl; do
    rpm -q "$p" >/dev/null 2>&1 || missing+=("$p")
  done
  install_pkgs() { sudo dnf install -y "$@"; }
else
  ui_warn "Unbekannte Distribution: Compiler, pkg-config und die GTK3-Entwicklungspakete selbst installieren"
fi
if [ ${#missing[@]} -gt 0 ]; then
  ui_sudo
  ui_run "Pakete installieren (${missing[*]})" install_pkgs "${missing[@]}"
else
  ui_ok "Benötigte Pakete sind da"
fi
setup_rust

V=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
ui_section "Showtime Suite $V"
ui_run "Programm bauen" cargo build --release --locked
BIN=target/release/showtime-suite

# --- Installieren (nur wenn sich etwas geändert hat) -------------------------
if user_app_current showtime-suite showtime-suite "$BIN"; then
  ui_skip "Installieren" "Showtime Suite $V ist schon installiert"
elif [ -n "" ] && pgrep -x showtime-suite >/dev/null 2>&1; then
  ui_warn "Showtime Suite läuft gerade – nicht ersetzt (es könnte eine Show laufen)."
  ui_info "Beenden, dann nochmal: ./install.sh"
  ui_done
  exit 0
else
  ui_run "Installieren (Showtime-Suite-Ordner)" user_app_install showtime-suite showtime-suite "$V" "$BIN" packaging/linux/showtime-suite.desktop assets/icon.png
fi
# A copy an older version of this script put into /usr/local would be found first.
if system_copy_there showtime-suite; then
  ui_sudo
  ui_run "Alte Kopie in /usr/local entfernen" system_copy_remove showtime-suite
fi
user_bin_on_path

ui_section "Fertig"
ui_ok "Starten: im Anwendungsmenü oder mit  showtime-suite"
ui_done
