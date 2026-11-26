#!/usr/bin/env bash
# Builds Waddle.app (Apple Silicon and Intel in one) and zips it: target/dist/Waddle-macOS.zip
set -euo pipefail
cd "$(dirname "$0")/../.."
VERSION=$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
cargo build --release --target x86_64-apple-darwin
APP=target/dist/Waddle.app
rm -rf "$APP" && mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create -output "$APP/Contents/MacOS/waddle" target/aarch64-apple-darwin/release/waddle target/x86_64-apple-darwin/release/waddle
# the penguin, at every size macOS asks for
ICONSET=target/dist/waddle.iconset && rm -rf "$ICONSET" && mkdir -p "$ICONSET"
for size in 16 32 128 256; do
  sips -z $size $size src/icon256.png --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2)); [ $double -le 256 ] && sips -z $double $double src/icon256.png --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/waddle.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>CFBundleName</key><string>Waddle</string>
  <key>CFBundleDisplayName</key><string>Waddle</string>
  <key>CFBundleIdentifier</key><string>io.github.soleritu.waddle</string>
  <key>CFBundleVersion</key><string>${VERSION}</string>
  <key>CFBundleShortVersionString</key><string>${VERSION}</string>
  <key>CFBundleExecutable</key><string>waddle</string>
  <key>CFBundleIconFile</key><string>waddle</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSMicrophoneUsageDescription</key><string>Speech to text records from the microphone, on this Mac only.</string>
</dict></plist>
PLIST
# not notarized: the first time, right-click Waddle and choose Open
codesign --force --deep --sign - "$APP" || true
(cd target/dist && ditto -c -k --keepParent Waddle.app Waddle-macOS.zip)
echo "Built target/dist/Waddle-macOS.zip"
