#!/usr/bin/env bash
# Baut Trackwright im Release-Modus und installiert es nach $PREFIX (Standard: ~/.local).
#
#   packaging/install.sh            # nach ~/.local
#   PREFIX=/usr/local sudo -E packaging/install.sh
set -euo pipefail
cd "$(dirname "$0")/.."
PREFIX="${PREFIX:-$HOME/.local}"
cargo build --release --locked
install -Dm755 target/release/trackwright-gui  "$PREFIX/bin/trackwright-gui"
install -Dm644 packaging/trackwright.desktop   "$PREFIX/share/applications/trackwright.desktop"
install -Dm644 packaging/trackwright.svg       "$PREFIX/share/icons/hicolor/scalable/apps/trackwright.svg"
command -v update-desktop-database >/dev/null && update-desktop-database "$PREFIX/share/applications" || true
echo "Installiert nach $PREFIX (bin/, share/applications/, share/icons/)."
