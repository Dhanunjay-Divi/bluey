#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# `cue-daemon/parakeet-stt` (package-qualified) compiles on-device English STT
# into cue-daemon. cue-cli has no such feature, so the feature is scoped to the
# daemon package. On Apple Silicon this links the prebuilt ONNX Runtime; on
# Intel macOS parakeet-rs uses load-dynamic and expects a libonnxruntime dylib
# at runtime (not staged here — arm64 is the MVP target). Model weights (~600MB)
# are fetched on first run, so artifact size is unchanged.
cargo build --release --features cue-daemon/parakeet-stt

# Real meeting overlay (cue-meeting-overlay): build its React UI (frontendDist =
# ui/dist, embedded at compile time) then the launchable binary. The daemon
# spawns this binary directly, so the plain binary — not a .app — is what ships.
(
  cd crates/cue-meeting-overlay/ui
  if [ -f package-lock.json ]; then npm ci; else npm install; fi
  npm run build
)
cargo build --release -p cue-meeting-overlay

bash native/macos/cue-overlay/build.sh >/dev/null
bash native/macos/cue-audio/build.sh >/dev/null
bash native/macos/cue-whisper/build.sh >/dev/null
bash native/macos/cue-picker/build.sh >/dev/null

ARCH="$(uname -m)"
DIST="dist/bluey-macos-${ARCH}"
rm -rf "$DIST"
mkdir -p "$DIST"
cp target/release/bluey "$DIST/bluey"
cp target/release/bluey-daemon "$DIST/bluey-daemon"
cp target/release/cue "$DIST/cue"
cp target/release/cue-daemon "$DIST/cue-daemon"
# Meeting overlay: stage under its real name AND under cue-overlay-tauri, the
# name discover_overlay_bin() probes for at runtime (both are socket-routed).
cp target/release/cue-meeting-overlay "$DIST/cue-meeting-overlay"
cp target/release/cue-meeting-overlay "$DIST/cue-overlay-tauri"
cp native/macos/cue-overlay/.build/bluey-overlay-macos "$DIST/bluey-overlay-macos"
cp native/macos/cue-overlay/.build/cue-overlay-macos "$DIST/cue-overlay-macos"
cp native/macos/cue-audio/.build/bluey-audio-macos "$DIST/bluey-audio-macos"
cp native/macos/cue-audio/.build/cue-audio-macos "$DIST/cue-audio-macos"
cp native/macos/cue-whisper/.build/cue-whisper "$DIST/cue-whisper"
cp native/macos/cue-whisper/.build/bluey-whisper-macos "$DIST/bluey-whisper-macos"
cp native/macos/cue-picker/.build/bluey-file-picker-macos "$DIST/bluey-file-picker-macos"
cp native/macos/cue-picker/.build/cue-file-picker-macos "$DIST/cue-file-picker-macos"
cp -R native/macos/cue-picker/.build/BlueyFilePicker.app "$DIST/BlueyFilePicker.app"

echo "$DIST"
