#!/usr/bin/env bash
# Wrap the cue-audio helper in a proper macOS .app bundle so it can hold the
# System Audio Recording and Microphone TCC grants.
#
# A certificate-backed signature with the stable sh.bluey.audio identifier keeps
# the helper's designated requirement stable across rebuilds. The restricted
# persistent-content-capture entitlement is intentionally OFF by default: Apple
# documents it for approved VNC apps, and macOS refuses to launch a process that
# claims it without a matching embedded provisioning profile.
#
# Optional build controls:
#   BLUEY_AUDIO_SWIFT_TRIPLE  Swift target triple (for example
#                             arm64-apple-macosx14.0).
#   BLUEY_AUDIO_APP_BINARY    Prebuilt helper to wrap instead of running SwiftPM.
#   BLUEY_REQUIRE_PERSISTENT_CAPTURE_ENTITLEMENT
#                             Set to 1 only after Apple approves the entitlement.
#   BLUEY_AUDIO_PROVISIONING_PROFILE
#                             Matching macOS .provisionprofile path. Required
#                             when the restricted entitlement is enabled.
#
# Usage: bash bundle-app.sh ["Apple Development: You (TEAMID)"]
set -euo pipefail
cd "$(dirname "$0")"

SIGN_ID="${1:-${BLUEY_CODESIGN_IDENTITY:-}}"
if [ -z "$SIGN_ID" ] && command -v security >/dev/null 2>&1; then
  SIGN_ID="$(
    security find-identity -v -p codesigning 2>/dev/null \
      | awk -F '"' '/^[[:space:]]*[0-9]+\)/ && NF >= 2 { print $2; exit }'
  )" || true
fi
SIGN_ID="${SIGN_ID:--}"
APP=".build/BlueyAudio.app"
SWIFT_TRIPLE="${BLUEY_AUDIO_SWIFT_TRIPLE:-}"
BIN="${BLUEY_AUDIO_APP_BINARY:-}"
REQUIRE_PERSISTENT="${BLUEY_REQUIRE_PERSISTENT_CAPTURE_ENTITLEMENT:-0}"
PROFILE="${BLUEY_AUDIO_PROVISIONING_PROFILE:-}"
ENTITLEMENTS=".build/bluey-audio.entitlements"

case "$REQUIRE_PERSISTENT" in
  0|1) ;;
  *)
    echo "bundle-app: BLUEY_REQUIRE_PERSISTENT_CAPTURE_ENTITLEMENT must be 0 or 1" >&2
    exit 2
    ;;
esac

if [ "$REQUIRE_PERSISTENT" = "1" ]; then
  if [ "$SIGN_ID" = "-" ]; then
    echo "bundle-app: persistent-content-capture requires certificate signing" >&2
    exit 1
  fi
  if [ -z "$PROFILE" ] || [ ! -f "$PROFILE" ]; then
    echo "bundle-app: persistent-content-capture requires a matching provisioning profile" >&2
    echo "bundle-app: set BLUEY_AUDIO_PROVISIONING_PROFILE to the approved macOS profile" >&2
    exit 1
  fi
fi

if [ -z "$BIN" ]; then
  swift_args=(-c release)
  if [ -n "$SWIFT_TRIPLE" ]; then
    swift_args+=(--triple "$SWIFT_TRIPLE")
  fi
  BIN="$(swift build "${swift_args[@]}" --show-bin-path)/cue-audio"
  swift build "${swift_args[@]}" >/dev/null

  # Keep the bare compatibility aliases on the same architecture as the app
  # bundle. The daemon prefers BlueyAudio.app, but older installations still
  # probe these names as a fallback.
  cp "$BIN" ".build/bluey-audio-macos"
  cp "$BIN" ".build/cue-audio-macos"
fi

if [ ! -f "$BIN" ]; then
  echo "bundle-app: helper binary not found: $BIN" >&2
  exit 1
fi

# Keep the destructive replacement scoped to this helper's fixed build output.
# Release scripts copy the completed bundle elsewhere only after verification.
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS"
cp "$BIN" "$APP/Contents/MacOS/BlueyAudio"

cat > "$APP/Contents/Info.plist" <<'PLIST'
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
  <string>Bluey captures meeting system audio for transcription. On-device transcription is the default; audio is sent to a cloud speech provider only when you explicitly configure one.</string>
  <!-- MICROPHONE (--source microphone, VoiceProcessingIO AEC) REQUIRES this.
       macOS TRAPS (SIGTRAP / exit 133) the instant a process touches the mic
       without NSMicrophoneUsageDescription — it is not optional. Its absence is
       why the mic-AEC helper crashed on launch the first time the mic path ran. -->
  <key>NSMicrophoneUsageDescription</key>
  <string>Bluey captures your microphone for meeting transcription. On-device transcription is the default; audio is sent to a cloud speech provider only when you explicitly configure one.</string>
  <!-- Kept for the --pick (SCContentSharingPicker) path, which still uses SCK. -->
  <key>NSScreenCaptureUsageDescription</key>
  <string>Bluey uses Screen and System Audio Recording to capture meeting audio for transcription. On-device transcription is the default; cloud speech is used only when you explicitly configure it.</string>
  <!-- Accessory app: no Dock icon. -->
  <key>LSUIElement</key>               <true/>
</dict>
</plist>
PLIST

if [ "$REQUIRE_PERSISTENT" = "1" ]; then
  profile_plist="$(mktemp -t bluey-audio-profile)"
  cleanup_profile_plist() {
    rm -f "$profile_plist"
  }
  trap cleanup_profile_plist EXIT

  if ! security cms -D -i "$PROFILE" >"$profile_plist"; then
    echo "bundle-app: provisioning profile is not a valid signed macOS profile" >&2
    exit 1
  fi
  profile_entitlement="$(
    /usr/libexec/PlistBuddy \
      -c "Print :Entitlements:com.apple.developer.persistent-content-capture" \
      "$profile_plist" 2>/dev/null || true
  )"
  if [ "$profile_entitlement" != "true" ]; then
    echo "bundle-app: provisioning profile does not authorize persistent-content-capture" >&2
    exit 1
  fi
  profile_app_id="$(
    /usr/libexec/PlistBuddy \
      -c "Print :Entitlements:application-identifier" \
      "$profile_plist" 2>/dev/null || \
      /usr/libexec/PlistBuddy \
        -c "Print :Entitlements:com.apple.application-identifier" \
        "$profile_plist" 2>/dev/null || true
  )"
  case "$profile_app_id" in
    *".sh.bluey.audio") ;;
    *)
      echo "bundle-app: provisioning profile does not match sh.bluey.audio" >&2
      exit 1
      ;;
  esac

  cp "$PROFILE" "$APP/Contents/embedded.provisionprofile"
  cat >"$ENTITLEMENTS" <<'ENT'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.developer.persistent-content-capture</key>
  <true/>
</dict>
</plist>
ENT
fi

if [ "$SIGN_ID" = "-" ]; then
  codesign --force --deep --sign - --identifier "sh.bluey.audio" "$APP"
elif [ "$REQUIRE_PERSISTENT" = "1" ]; then
  codesign --force --deep --sign "$SIGN_ID" \
    --identifier "sh.bluey.audio" \
    --entitlements "$ENTITLEMENTS" \
    --options runtime \
    "$APP"
else
  # Certificate-backed and stable, but deliberately free of restricted
  # entitlements so macOS does not require a provisioning profile to launch it.
  codesign --force --deep --sign "$SIGN_ID" \
    --identifier "sh.bluey.audio" \
    --options runtime \
    "$APP"
fi

echo "$APP"
echo "  signed with: $SIGN_ID"
echo "  persistent-content-capture: $REQUIRE_PERSISTENT"
echo "  run: $APP/Contents/MacOS/BlueyAudio --source system --continuous"
