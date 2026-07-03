#!/usr/bin/env bash
# Wrap the cue-audio helper in a proper macOS .app bundle so it can hold the
# Screen Recording (System Audio) TCC grant.
#
# WHY THIS EXISTS (the hard-won lesson):
#   - A bare CLI binary CANNOT appear in System Settings → Screen & System Audio
#     Recording, so the user can never grant it, and ScreenCaptureKit returns
#     SILENT audio buffers with no error.
#   - An .app bundle DOES appear in that list.
#   - NSScreenCaptureUsageDescription is MANDATORY — macOS terminates the app
#     without it.
#   - Ad-hoc signing changes the code hash every rebuild, which RESETS the TCC
#     grant. Signing with a stable identity (Apple Development cert) keeps the
#     grant across rebuilds. Pass the identity as $1; defaults to ad-hoc (-) for
#     a first local run (you'll just re-grant after a rebuild).
#
# Usage:  bash bundle-app.sh ["Apple Development: You (TEAMID)"]
set -euo pipefail
cd "$(dirname "$0")"

SIGN_ID="${1:--}"   # default: ad-hoc. Pass a real cert to persist the grant.
APP="BlueyAudio.app"
BIN="$(swift build -c release --show-bin-path)/cue-audio"

swift build -c release >/dev/null
rm -rf ".build/$APP"
mkdir -p ".build/$APP/Contents/MacOS"
cp "$BIN" ".build/$APP/Contents/MacOS/BlueyAudio"

cat > ".build/$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>        <string>BlueyAudio</string>
  <key>CFBundleIdentifier</key>        <string>sh.bluey.audio</string>
  <key>CFBundleName</key>              <string>BlueyAudio</string>
  <key>CFBundlePackageType</key>       <string>APPL</string>
  <key>CFBundleShortVersionString</key><string>1.0</string>
  <key>LSMinimumSystemVersion</key>    <string>13.0</string>
  <!-- Core Audio process-tap (system audio) REQUIRES this — without it
       AudioHardwareCreateProcessTap is denied. This is the real capture path. -->
  <key>NSAudioCaptureUsageDescription</key>
  <string>Bluey transcribes your meeting's audio on-device so it can follow along and answer questions for you. Audio never leaves your machine.</string>
  <!-- Kept for the --pick (SCContentSharingPicker) path, which still uses SCK. -->
  <key>NSScreenCaptureUsageDescription</key>
  <string>Bluey transcribes your meeting's audio on-device so it can follow along and answer questions for you. Audio never leaves your machine.</string>
  <!-- Accessory app: no Dock icon. -->
  <key>LSUIElement</key>               <true/>
</dict>
</plist>
PLIST

# Entitlement to help the Screen Recording grant persist across launches.
cat > ".build/bluey-audio.entitlements" <<'ENT'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.developer.persistent-content-capture</key> <true/>
</dict>
</plist>
ENT

# `--deep` is required for the bundle to LAUNCH via `open` (without it macOS
# fails with "Launchd job spawn failed"). With a real cert we also apply the
# persistent-content-capture entitlement + hardened runtime; ad-hoc ("-") falls
# back to a plain deep sign (entitlement needs a real cert).
if [ "$SIGN_ID" = "-" ]; then
  codesign --force --deep --sign - ".build/$APP"
else
  codesign --force --deep --sign "$SIGN_ID" \
    --entitlements ".build/bluey-audio.entitlements" \
    --options runtime \
    ".build/$APP"
fi

echo ".build/$APP"
echo "  signed with: $SIGN_ID"
echo "  run:  .build/$APP/Contents/MacOS/BlueyAudio --source system --continuous"
