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
#
# Speaker diarization (cue-daemon/diarize, speakrs) is OPT-IN via
# BLUEY_DIARIZE_BUILD=1 — it links arm64 OpenBLAS (Homebrew) for one PLDA
# eigendecomposition, so it's not in the default build. Diarization models
# (~60MB, permissive) download on first run; runtime is gated by BLUEY_DIARIZE=1.
DAEMON_FEATURES="cue-daemon/parakeet-stt"
if [ "${BLUEY_DIARIZE_BUILD:-0}" = "1" ]; then
  DAEMON_FEATURES="$DAEMON_FEATURES cue-daemon/diarize"
  # ndarray-linalg's openblas-system backend needs the arm64 OpenBLAS pkg-config.
  export PKG_CONFIG_PATH="/opt/homebrew/opt/openblas/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
  echo "  (diarization enabled — linking Homebrew arm64 OpenBLAS)"
fi
cargo build --release --features "$DAEMON_FEATURES"

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
# System-audio capture ships as a SIGNED .app bundle, not a bare binary: a bare
# CLI can't hold the Screen Recording (System Audio) TCC grant. Pass a signing
# identity via BLUEY_CODESIGN_IDENTITY so the grant persists across updates;
# defaults to ad-hoc ("-") which works but resets the grant on each rebuild.
bash native/macos/cue-audio/bundle-app.sh "${BLUEY_CODESIGN_IDENTITY:--}" >/dev/null
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
# The signed .app the daemon prefers (BlueyAudio.app/Contents/MacOS/BlueyAudio).
# system_capture.rs::platform_binary_path() looks for it next to the daemon.
cp -R native/macos/cue-audio/.build/BlueyAudio.app "$DIST/BlueyAudio.app"
cp native/macos/cue-whisper/.build/cue-whisper "$DIST/cue-whisper"
cp native/macos/cue-whisper/.build/bluey-whisper-macos "$DIST/bluey-whisper-macos"
cp native/macos/cue-picker/.build/bluey-file-picker-macos "$DIST/bluey-file-picker-macos"
cp native/macos/cue-picker/.build/cue-file-picker-macos "$DIST/cue-file-picker-macos"
cp -R native/macos/cue-picker/.build/BlueyFilePicker.app "$DIST/BlueyFilePicker.app"
cp -R native/macos/cue-shot/.build/BlueyShot.app "$DIST/BlueyShot.app"

# Diarization build: the daemon links Homebrew's libopenblas via an absolute
# path. Stage the dylib next to the daemon and rewrite the load command to
# @loader_path so the .app runs without Homebrew present.
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

echo "$DIST"
