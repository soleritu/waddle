#!/bin/sh
# Installs Waddle on Linux or macOS. Run: curl -fsSL https://raw.githubusercontent.com/soleritu/waddle/main/install.sh | sh
set -e
echo "Finding the newest Waddle..."
releases=$(curl -fsSL "https://api.github.com/repos/soleritu/waddle/releases?per_page=5")
link() { printf '%s' "$releases" | grep -o "\"browser_download_url\": *\"[^\"]*$1\"" | head -1 | sed 's/.*"\(https[^"]*\)"/\1/'; }

if [ "$(uname -s)" = "Darwin" ]; then
  url=$(link "Waddle-macOS.zip")
  [ -n "$url" ] || { echo "The newest release has no macOS app yet. Try again in a few minutes."; exit 1; }
  dest="${WADDLE_APPS:-$HOME/Applications}"
  mkdir -p "$dest"
  tmp=$(mktemp -d)
  curl -fsSL "$url" -o "$tmp/Waddle.zip"
  rm -rf "$dest/Waddle.app"
  ditto -x -k "$tmp/Waddle.zip" "$dest"
  rm -rf "$tmp"
  echo "Done. Waddle is in $dest. Opening it now."
  open "$dest/Waddle.app"
else
  url=$(link "Waddle-x86_64.AppImage")
  [ -n "$url" ] || { echo "The newest release has no AppImage yet. Try again in a few minutes."; exit 1; }
  dest="${WADDLE_BIN:-$HOME/.local/bin}"
  mkdir -p "$dest"
  curl -fsSL "$url" -o "$dest/Waddle.AppImage"
  chmod +x "$dest/Waddle.AppImage"
  echo "Done. Opening Waddle now. It adds itself to your app menu."
  nohup "$dest/Waddle.AppImage" >/dev/null 2>&1 &
fi
