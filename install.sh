#!/bin/sh
# Installs Waddle on Linux. Run: curl -fsSL https://raw.githubusercontent.com/soleritu/waddle/main/install.sh | sh
set -e
dest="${WADDLE_BIN:-$HOME/.local/bin}"
mkdir -p "$dest"
echo "Finding the newest Waddle..."
url=$(curl -fsSL "https://api.github.com/repos/soleritu/waddle/releases?per_page=5" | grep -o '"browser_download_url": *"[^"]*Waddle-x86_64.AppImage"' | head -1 | sed 's/.*"\(https[^"]*\)"/\1/')
[ -n "$url" ] || { echo "The newest release has no AppImage yet. Try again in a few minutes."; exit 1; }
curl -fsSL "$url" -o "$dest/Waddle.AppImage"
chmod +x "$dest/Waddle.AppImage"
echo "Done. Opening Waddle now. It adds itself to your app menu."
nohup "$dest/Waddle.AppImage" >/dev/null 2>&1 &
