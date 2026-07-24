#!/usr/bin/env bash
# Build BlueyShot.app — a tiny .app bundle whose ONLY job is to take a screenshot
# with `screencapture` and write it to a path passed on the command line.
#
# WHY THIS EXISTS (same hard-won lesson as BlueyAudio.app):
#   - macOS TCC Screen Recording CANNOT be granted to a BARE binary — it has no
#     bundle identity for the grant to attach to, so `screencapture` is always
#     denied (produces an empty/zero-byte file with no error). The overlay and
#     daemon are bare binaries, so neither can capture the screen.
#   - An .app BUNDLE with a stable CFBundleIdentifier DOES appear in System
#     Settings → Screen & System Audio Recording and CAN hold the grant. This is
#     proven on this repo's macOS by BlueyAudio.app (ad-hoc signed, grant sticks).
#   - NSScreenCaptureUsageDescription is MANDATORY or macOS kills the app.
#   - Launch it via `/usr/bin/open -n <BlueyShot.app>` (NOT the inner binary) so
#     macOS reads the bundle identity and attributes the capture to sh.bluey.shot.
#   - Ad-hoc signing with a STABLE --identifier keeps the grant across rebuilds
#     (a per-build content hash would reset it every time).
#
# Usage:  bash build.sh
# Result: .build/BlueyShot.app  (launch: open -n .build/BlueyShot.app --args --out <png>)
set -euo pipefail
cd "$(dirname "$0")"

APP=".build/BlueyShot.app"
# Compile the Swift capture binary (a real Mach-O — a shell wrapper around
# `screencapture` would attribute the capture to a child process, not this
# bundle, so TCC would never grant it. See main.swift.)
BIN="$(swift build -c release --show-bin-path)/cue-shot"
swift build -c release >/dev/null

rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$BIN" "$APP/Contents/MacOS/BlueyShot"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>        <string>BlueyShot</string>
  <key>CFBundleIdentifier</key>        <string>sh.bluey.shot</string>
  <key>CFBundleName</key>              <string>BlueyShot</string>
  <key>CFBundlePackageType</key>       <string>APPL</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>LSMinimumSystemVersion</key>    <string>13.0</string>
  <!-- MANDATORY for Screen Recording, or macOS terminates the app. -->
  <key>NSScreenCaptureUsageDescription</key>
  <string>Bluey captures a screenshot on-device so it can attach it to your meeting context and let your agent see what is on screen. The image stays on your machine.</string>
  <!-- Accessory app: no Dock icon (a screenshot helper must not steal focus). -->
  <key>LSUIElement</key>               <true/>
</dict>
</plist>
PLIST

# `--deep` is required for the bundle to LAUNCH via `open`. A STABLE --identifier
# keeps the TCC grant across rebuilds (ad-hoc content-hash ids reset it).
codesign --force --deep --sign - --identifier "sh.bluey.shot" "$APP"

echo "$APP"
echo "  run: open -n $APP --args --out /tmp/shot.png"
