#!/usr/bin/env bash
# Builds Pumice and installs it for your user only (no sudo): the program in ~/.local/bin,
# the launcher entry and the icons in ~/.local/share. Run it again to update.
# The administrator helper is separate: scripts/install-helper.sh.
set -euo pipefail
here="$(cd "$(dirname "$0")/.." && pwd)"
cd "$here"
if [[ "${1:-}" != "--no-build" ]]; then
  npm install --no-audit --no-fund
  npm run tauri build -- --no-bundle
fi
bin="$here/target/release/pumice"
[[ -x "$bin" ]] || { echo "No release build at $bin" >&2; exit 1; }
install -Dm755 "$bin" "$HOME/.local/bin/pumice"
install -Dm644 packaging/pumice.desktop "$HOME/.local/share/applications/pumice.desktop"
for s in 16 24 32 48 64 128 256 512; do
  install -Dm644 "packaging/icons/pumice-$s.png" "$HOME/.local/share/icons/hicolor/${s}x${s}/apps/pumice.png"
done
install -Dm644 packaging/pumice.svg "$HOME/.local/share/icons/hicolor/scalable/apps/pumice.svg"
command -v update-desktop-database >/dev/null && update-desktop-database "$HOME/.local/share/applications" || true
command -v gtk-update-icon-cache >/dev/null && gtk-update-icon-cache -q -t "$HOME/.local/share/icons/hicolor" || true
echo "Installed: ~/.local/bin/pumice, the launcher entry and the icons."
