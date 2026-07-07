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
# parakeet-stt = on-device STT; local-memory = keyless cross-meeting facts
# memory (local bge-small embedder — reuses the same ONNX runtime).
DAEMON_FEATURES="cue-daemon/parakeet-stt cue-daemon/local-memory"
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

# Diarization build: the daemon links Homebrew's libopenblas, which TRANSITIVELY
# depends on more Homebrew dylibs (libgfortran, libomp, libquadmath, libgcc_s from
# gcc). A fresh Mac has NONE of them, so bundling only libopenblas made the daemon
# crash at load ("Library not loaded: .../libgfortran.5.dylib"). We must vendor the
# ENTIRE recursive dependency closure next to the daemon with every inter-dylib
# reference rewritten to @loader_path.
#
# dylibbundler (the standard tool for exactly this — auriamg/macdylibbundler) walks
# the closure, copies each dylib in, rewrites its id + every consumer's reference,
# and ad-hoc re-signs. We point its inner path at @loader_path/ because our layout
# is flat (dylibs sit BESIDE the daemon in bin/, not in ../libs/). We fix bluey-
# daemon first, then reuse its already-bundled dylibs when fixing cue-daemon.
if [ "${BLUEY_DIARIZE_BUILD:-0}" = "1" ]; then
  if ! command -v dylibbundler >/dev/null 2>&1; then
    echo "  ERROR: dylibbundler not found — install it: brew install dylibbundler" >&2
    echo "  (required to vendor the OpenBLAS/gfortran/omp dylib closure for diarization)" >&2
    exit 1
  fi
  # Fix each daemon binary. -b bundle deps, -d dest = DIST (flat, beside daemon),
  # -p @loader_path/ inner path, -of overwrite files (2nd daemon reuses 1st's
  # dylibs), -cd create dir, -s search Homebrew's openblas lib dir. dylibbundler
  # ad-hoc codesigns each dylib by default; install.sh re-signs the daemon after
  # any later mutation, and re-signs everything on the receiver anyway.
  for daemon_bin in bluey-daemon cue-daemon; do
    [ -f "$DIST/$daemon_bin" ] || continue
    dylibbundler \
      -x "$DIST/$daemon_bin" \
      -b -d "$DIST" -p "@loader_path/" -of -cd \
      -s /opt/homebrew/opt/openblas/lib \
      >/dev/null 2>&1 || {
        echo "  ERROR: dylibbundler failed on $daemon_bin" >&2; exit 1;
      }
  done

  # De-duplicate LC_RPATH. OpenBLAS ships with SEVERAL rpaths (gcc dirs); when
  # dylibbundler rewrites each to '@loader_path/' they collapse into duplicates,
  # and dyld REFUSES to load a binary with a duplicate LC_RPATH ("duplicate
  # LC_RPATH '@loader_path/'") — the daemon then can't find its own dylibs and
  # crashes at startup on EVERY Mac. Collapse each file's '@loader_path/' rpaths
  # to exactly one, then re-sign.
  for f in "$DIST"/*.dylib "$DIST/bluey-daemon" "$DIST/cue-daemon"; do
    [ -f "$f" ] || continue
    chmod u+w "$f"
    while otool -l "$f" 2>/dev/null | grep -A2 LC_RPATH | grep -q "path @loader_path/ "; do
      install_name_tool -delete_rpath "@loader_path/" "$f" 2>/dev/null || break
    done
    install_name_tool -add_rpath "@loader_path/" "$f" 2>/dev/null || true
    codesign --force --sign - "$f" >/dev/null 2>&1 || true
  done

  # Verify: no Homebrew/absolute paths must remain in the daemon or any bundled
  # dylib, or it crashes on a fresh Mac. Fail the build loudly if any slipped.
  leftover="$(for f in "$DIST"/*.dylib "$DIST/bluey-daemon" "$DIST/cue-daemon"; do
    [ -f "$f" ] && otool -L "$f" 2>/dev/null | tail -n +2 | grep -E "/opt/homebrew|/opt/local|/usr/local"
  done)"
  if [ -n "$leftover" ]; then
    echo "  ERROR: unbundled Homebrew paths remain (would crash on a fresh Mac):" >&2
    echo "$leftover" >&2
    exit 1
  fi

  # Load-test: the daemon MUST start (--version) with only the bundled dylibs.
  # This is the definitive check that the whole closure resolves via @loader_path
  # and no rpath/duplicate/missing-dep issue survives. Fail loudly if it can't.
  if ! ( cd "$DIST" && ./bluey-daemon --version >/dev/null 2>&1 ); then
    echo "  ERROR: bundled daemon fails to load its dylibs (dyld error) — see:" >&2
    ( cd "$DIST" && ./bluey-daemon --version 2>&1 | head -4 >&2 )
    exit 1
  fi

  dylib_count="$(find "$DIST" -maxdepth 1 -name '*.dylib' 2>/dev/null | wc -l | tr -d ' ')"
  echo "  bundled diarization dylib closure ($dylib_count dylibs) → @loader_path, self-contained + load-tested"
fi

echo "  MEETING-ONLY build — interview overlay excluded."
echo "$DIST"
