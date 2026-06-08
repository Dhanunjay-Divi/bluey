#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$SCRIPT_DIR"
CONFIGURATION="${BLUEY_OVERLAY_SWIFT_CONFIGURATION:-release}"
swift build -c "$CONFIGURATION"
mkdir -p .build
BIN="$(swift build -c "$CONFIGURATION" --show-bin-path)/cue-overlay"
cp "$BIN" .build/bluey-overlay-macos
cp "$BIN" .build/cue-overlay-macos

APP_DIR=".build/BlueyOverlay.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
cp "$BIN" "$APP_DIR/Contents/MacOS/bluey-overlay-macos"
chmod +x "$APP_DIR/Contents/MacOS/bluey-overlay-macos"
cat > "$APP_DIR/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>bluey-overlay-macos</string>
  <key>CFBundleIdentifier</key>
  <string>sh.bluey.overlay</string>
  <key>CFBundleName</key>
  <string>Bluey Overlay</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSBackgroundOnly</key>
  <false/>
  <key>LSUIElement</key>
  <true/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

target_profiles=(debug release)
if [[ "$CONFIGURATION" == "debug" ]]; then
  target_profiles=(debug)
fi

for profile in "${target_profiles[@]}"; do
  target_dir="$ROOT/target/$profile"
  if [[ -d "$target_dir" ]]; then
    cp "$BIN" "$target_dir/bluey-overlay-macos"
    cp "$BIN" "$target_dir/cue-overlay-macos"
    rm -rf "$target_dir/BlueyOverlay.app"
    cp -R "$APP_DIR" "$target_dir/BlueyOverlay.app"
  fi
done

echo ".build/bluey-overlay-macos"
