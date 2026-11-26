#!/bin/sh
# Builds Waddle-x86_64.AppImage on Linux. Needs Rust, the WebKitGTK and libayatana-appindicator
# development packages, and linuxdeploy with its GTK plugin on PATH.
#   Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-dev libayatana-appindicator3-dev libxdo-dev
set -e
cd "$(dirname "$0")/../.."
cargo build --release
app=target/AppDir
rm -rf "$app"
mkdir -p "$app/usr/bin" "$app/usr/share/applications" "$app/usr/share/icons/hicolor/256x256/apps"
cp target/release/waddle "$app/usr/bin/waddle"
cp src/icon256.png "$app/usr/share/icons/hicolor/256x256/apps/waddle.png"
cat > "$app/usr/share/applications/waddle.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=Waddle
Comment=Local AI hosting
Exec=waddle
Icon=waddle
Terminal=false
Categories=Development;
DESKTOP
export DEPLOY_GTK_VERSION=3
linuxdeploy --appdir "$app" --plugin gtk --output appimage \
  --desktop-file "$app/usr/share/applications/waddle.desktop" --icon-file "$app/usr/share/icons/hicolor/256x256/apps/waddle.png"
mkdir -p target/dist
mv Waddle-*.AppImage target/dist/Waddle-x86_64.AppImage
echo "Built target/dist/Waddle-x86_64.AppImage"
