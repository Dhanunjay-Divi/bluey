#!/usr/bin/env bash
# Dev-install: build the current tree and point `bluey` / `bluey on` at it.
#
# Copies the freshly-built daemon, CLI, meeting overlay, and the SIGNED
# BlueyAudio.app (system-audio capture) into a local install dir, then repoints
# the ~/.local/bin symlinks. After this, plain `bluey on` runs YOUR latest build.
#
#   bash run-local.sh
#
# Notes:
# - arm64 build (the only natively-correct target on Apple Silicon; plain cargo
#   builds emulated x86_64 binaries that fail ort/Parakeet).
# - System-audio capture starts when you click "Listen" in the overlay (not at
#   boot). Grant Screen Recording to BlueyAudio once in System Settings.
set -euo pipefail
cd "$(dirname "$0")"
ROOT="$PWD"
TARGET="aarch64-apple-darwin"
INSTALL="$HOME/.local/bluey-dev/local/bin"
BIN="$HOME/.local/bin"

echo "==> Building daemon + CLI (release, $TARGET, parakeet on)…"
cargo build --release -p cue-daemon -p cue-cli --features cue-daemon/parakeet-stt --target "$TARGET"

echo "==> Building meeting overlay…"
cargo build --release -p cue-meeting-overlay --target "$TARGET"

echo "==> Building + bundling system-audio helper (BlueyAudio.app)…"
bash native/macos/cue-audio/bundle-app.sh "${BLUEY_CODESIGN_IDENTITY:--}" >/dev/null

echo "==> Installing into $INSTALL…"
mkdir -p "$INSTALL" "$BIN"
TGT="$ROOT/target/$TARGET/release"
cp "$TGT/bluey"               "$INSTALL/bluey"
cp "$TGT/bluey-daemon"        "$INSTALL/bluey-daemon"
cp "$TGT/cue-meeting-overlay" "$INSTALL/cue-meeting-overlay"
# The signed .app the daemon auto-discovers next to itself for system audio.
rm -rf "$INSTALL/BlueyAudio.app"
cp -R "$ROOT/native/macos/cue-audio/.build/BlueyAudio.app" "$INSTALL/BlueyAudio.app"

echo "==> Repointing symlinks ($BIN)…"
ln -sf "$INSTALL/bluey"        "$BIN/bluey"
ln -sf "$INSTALL/bluey-daemon" "$BIN/bluey-daemon"

echo
echo "Done. Your 'bluey on' now runs this build."
echo "  - Run:   bluey on"
echo "  - Click 'Listen' in the overlay to start system-audio transcription."
echo "  - First run: grant 'BlueyAudio' in System Settings → Screen Recording."
