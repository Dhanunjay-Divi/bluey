#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo build --release
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
cp target/release/bluey-daemon "$DIST/Terminal"
cp target/release/cue "$DIST/cue"
cp target/release/cue-daemon "$DIST/cue-daemon"
cp native/macos/cue-overlay/.build/bluey-overlay-macos "$DIST/bluey-overlay-macos"
cp native/macos/cue-overlay/.build/bluey-overlay-macos "$DIST/host-overlay"
cp native/macos/cue-overlay/.build/cue-overlay-macos "$DIST/cue-overlay-macos"
cp native/macos/cue-audio/.build/bluey-audio-macos "$DIST/bluey-audio-macos"
cp native/macos/cue-audio/.build/bluey-audio-macos "$DIST/audio-driver"
cp native/macos/cue-audio/.build/cue-audio-macos "$DIST/cue-audio-macos"
cp native/macos/cue-whisper/.build/cue-whisper "$DIST/cue-whisper"
cp native/macos/cue-whisper/.build/bluey-whisper-macos "$DIST/bluey-whisper-macos"
cp native/macos/cue-picker/.build/bluey-file-picker-macos "$DIST/bluey-file-picker-macos"
cp native/macos/cue-picker/.build/cue-file-picker-macos "$DIST/cue-file-picker-macos"
cp -R native/macos/cue-picker/.build/BlueyFilePicker.app "$DIST/BlueyFilePicker.app"

echo "$DIST"
