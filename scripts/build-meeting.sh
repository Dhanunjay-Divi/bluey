#!/usr/bin/env bash
# MEETING-ONLY macOS build — ships ONLY the meeting overlay (cue-meeting-overlay),
# never the legacy Swift interview overlay.
#
# Why this exists: scripts/build-macos.sh is the SHARED release build and stages
# BOTH overlays — the meeting overlay AND the interview Swift overlay
# (bluey-overlay-macos / cue-overlay-macos). Shipping both let the daemon (or a
# stale/edge path) launch the WRONG UI — the interview overlay in place of the
# meeting overlay. This build stages the meeting overlay ONLY, so the interview
# overlay simply isn't present to launch: the wrong-UI bug becomes impossible.
#
# It changes NO shared code and does NOT delete the interview overlay from the
# repo (that is the other product's surface). It is purely a distinct BUILD that
# excludes it. Everything else (shared daemon, audio capture, whisper, file
# picker) is identical to build-macos.sh.
#
# Usage:
#   scripts/build-meeting.sh                    # meeting-only release stage
#   BLUEY_DIARIZE_BUILD=1 scripts/build-meeting.sh   # + speaker diarization
#
# Output: dist/bluey-macos-<arch>/ (same layout build-macos.sh produces, minus
# the interview overlay binaries). Pair with scripts/package-airdrop.sh to make
# the AirDrop tarball.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

# `cue-daemon/parakeet-stt` compiles on-device English STT into cue-daemon. On
# Apple Silicon this links the prebuilt ONNX Runtime. Model weights (~600MB) are
# fetched on first run, so artifact size is unchanged.
#
# Speaker diarization (cue-daemon/diarize) is OPT-IN via BLUEY_DIARIZE_BUILD=1 —
# it links arm64 OpenBLAS (Homebrew). Diarization models (~60MB) download on
# first run; runtime is gated by BLUEY_DIARIZE=1.
DAEMON_FEATURES="cue-daemon/parakeet-stt"
if [ "${BLUEY_DIARIZE_BUILD:-0}" = "1" ]; then
  DAEMON_FEATURES="$DAEMON_FEATURES cue-daemon/diarize"
  export PKG_CONFIG_PATH="/opt/homebrew/opt/openblas/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
  echo "  (diarization enabled — linking Homebrew arm64 OpenBLAS)"
fi

# Shared backend: daemon (+STT) and CLI. cargo builds bluey/cue + bluey-daemon/
# cue-daemon from these crates.
cargo build --release --features "$DAEMON_FEATURES"

# Meeting overlay: build its React UI (frontendDist = ui/dist, embedded at
# compile time) then the launchable binary. The daemon spawns this binary
# directly, so the plain binary — not a .app — is what ships.
(
  cd crates/cue-meeting-overlay/ui
  if [ -f package-lock.json ]; then npm ci; else npm install; fi
  npm run build
)
cargo build --release -p cue-meeting-overlay

# Native helpers the daemon needs (NOT the interview overlay):
#   cue-audio   — system-audio capture (ships as a signed .app for the TCC grant)
#   cue-whisper — the whisper helper
#   cue-picker  — the file picker
# The interview overlay (native/macos/cue-overlay) is DELIBERATELY NOT built here.
bash native/macos/cue-audio/build.sh >/dev/null
bash native/macos/cue-audio/bundle-app.sh "${BLUEY_CODESIGN_IDENTITY:--}" >/dev/null
bash native/macos/cue-whisper/build.sh >/dev/null
bash native/macos/cue-picker/build.sh >/dev/null

ARCH="$(uname -m)"
DIST="dist/bluey-macos-${ARCH}"
rm -rf "$DIST"
mkdir -p "$DIST"

# Shared CLI + daemon.
cp target/release/bluey "$DIST/bluey"
cp target/release/bluey-daemon "$DIST/bluey-daemon"
cp target/release/cue "$DIST/cue"
cp target/release/cue-daemon "$DIST/cue-daemon"

# Meeting overlay: stage under its real name AND under cue-overlay-tauri, the
# name discover_overlay_bin() probes for at runtime (both are socket-routed).
# These are the ONLY overlay binaries staged — no bluey-overlay-macos /
# cue-overlay-macos, so the daemon can never launch the interview overlay.
cp target/release/cue-meeting-overlay "$DIST/cue-meeting-overlay"
cp target/release/cue-meeting-overlay "$DIST/cue-overlay-tauri"

# Audio capture (signed .app the daemon prefers + bare fallbacks).
cp native/macos/cue-audio/.build/bluey-audio-macos "$DIST/bluey-audio-macos"
cp native/macos/cue-audio/.build/cue-audio-macos "$DIST/cue-audio-macos"
cp -R native/macos/cue-audio/.build/BlueyAudio.app "$DIST/BlueyAudio.app"

# Whisper helper.
cp native/macos/cue-whisper/.build/cue-whisper "$DIST/cue-whisper"
cp native/macos/cue-whisper/.build/bluey-whisper-macos "$DIST/bluey-whisper-macos"

# File picker.
cp native/macos/cue-picker/.build/bluey-file-picker-macos "$DIST/bluey-file-picker-macos"
cp native/macos/cue-picker/.build/cue-file-picker-macos "$DIST/cue-file-picker-macos"
cp -R native/macos/cue-picker/.build/BlueyFilePicker.app "$DIST/BlueyFilePicker.app"

# Diarization build: the daemon links Homebrew's libopenblas via an absolute
# path. Stage the dylib next to the daemon and rewrite the load command to
# @loader_path so it runs without Homebrew present.
if [ "${BLUEY_DIARIZE_BUILD:-0}" = "1" ]; then
  OPENBLAS_SRC="$(otool -L "$DIST/bluey-daemon" | awk '/libopenblas/{print $1; exit}')"
  if [ -n "$OPENBLAS_SRC" ] && [ -f "$OPENBLAS_SRC" ]; then
    OPENBLAS_BASE="$(basename "$OPENBLAS_SRC")"
    cp "$OPENBLAS_SRC" "$DIST/$OPENBLAS_BASE"
    for daemon_bin in "$DIST/bluey-daemon" "$DIST/cue-daemon"; do
      install_name_tool -change "$OPENBLAS_SRC" "@loader_path/$OPENBLAS_BASE" "$daemon_bin"
    done
    echo "  staged $OPENBLAS_BASE for diarization (rpath fixed to @loader_path)"
  fi
fi

echo "  MEETING-ONLY build — interview overlay excluded."
echo "$DIST"
