#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$SCRIPT_DIR"
swift build -c release
mkdir -p .build
BIN="$(swift build -c release --show-bin-path)/cue-picker"
cp "$BIN" .build/bluey-file-picker-macos
cp "$BIN" .build/cue-file-picker-macos

APP_DIR=".build/BlueyFilePicker.app"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
cp "$BIN" "$APP_DIR/Contents/MacOS/bluey-file-picker-macos"
chmod +x "$APP_DIR/Contents/MacOS/bluey-file-picker-macos"
cat > "$APP_DIR/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
  "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDevelopmentRegion</key>
  <string>en</string>
  <key>CFBundleExecutable</key>
  <string>bluey-file-picker-macos</string>
  <key>CFBundleIdentifier</key>
  <string>sh.bluey.file-picker</string>
  <key>CFBundleName</key>
  <string>Bluey File Picker</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSBackgroundOnly</key>
  <false/>
  <!-- NOT a UI element / accessory: the picker must show a real, frontmost
       NSOpenPanel. With LSUIElement=true the dialog opens behind other windows
       and the user never sees it (the daemon's `open -W` then just blocks until
       timeout). The process is short-lived (opens the panel, writes the paths,
       quits), so a brief Dock presence is fine. -->
  <key>LSUIElement</key>
  <false/>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
PLIST

for profile in debug release; do
  target_dir="$ROOT/target/$profile"
  if [[ -d "$target_dir" ]]; then
    cp "$BIN" "$target_dir/bluey-file-picker-macos"
    cp "$BIN" "$target_dir/cue-file-picker-macos"
    rm -rf "$target_dir/BlueyFilePicker.app"
    cp -R "$APP_DIR" "$target_dir/BlueyFilePicker.app"
  fi
done

echo ".build/bluey-file-picker-macos"
