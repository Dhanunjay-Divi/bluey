#!/usr/bin/env bash
# Rebuild the whole local Bluey (overlay UI + daemon + CLI + overlay binary) and
# reinstall it over the dev install, so `bluey on` always launches CURRENT code.
#
# Why this exists: the overlay is a compiled binary that EMBEDS the built React
# `ui/dist`, and `bluey`/`bluey-daemon` are copied into a versioned install dir
# that `~/.local/bin/bluey` symlinks to. Editing UI or Rust source does NOT update
# that installed binary — you must rebuild the dist, rebuild the binaries, and copy
# them over. Skipping any step leaves a STALE binary running old code (the "my
# change isn't showing" trap). This does all of it in one command.
#
# Usage:
#   scripts/reinstall-dev.sh            # rebuild + reinstall, then restart the daemon
#   scripts/reinstall-dev.sh --no-start # rebuild + reinstall only (don't (re)start)
#
# Honors the same install location as scripts/install.sh:
#   BLUEY_BIN_DIR   symlink dir (default: ~/.local/bin) — used to LOCATE the real
#                   install dir via the `bluey` symlink, so this always targets the
#                   exact dir `bluey on` runs from.
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"

# On Apple Silicon the default rustup toolchain is x86_64; its emulated binaries
# silently misbehave (ort/Parakeet). ALWAYS build the native arm64 target. On an
# Intel Mac this resolves to the x86_64 triple.
TARGET="$(rustc -vV | awk '/host:/{print $2}')"
# On-device STT lives behind this feature — a plain build ships an STT-less daemon
# that falls through to the cloud "sign in" gate and transcribes nothing.
# local-memory adds the keyless cross-meeting facts memory (bge-small via ort).
DAEMON_FEATURES="cue-daemon/parakeet-stt cue-daemon/local-memory"
# openblas pkg-config for the diarize/daemon link.
export PKG_CONFIG_PATH="${PKG_CONFIG_PATH:-/opt/homebrew/opt/openblas/lib/pkgconfig}"

start_after=1
for arg in "$@"; do
  case "$arg" in
    --no-start) start_after=0 ;;
    *) printf 'reinstall-dev: unknown arg: %s\n' "$arg" >&2; exit 2 ;;
  esac
done

# --- locate the real install dir via the `bluey` symlink (robust to config) ---
bin_dir="${BLUEY_BIN_DIR:-$HOME/.local/bin}"
link="$bin_dir/bluey"
if [[ ! -L "$link" ]]; then
  printf 'reinstall-dev: no bluey symlink at %s — run scripts/install.sh once first.\n' "$link" >&2
  exit 1
fi
install_bin="$(dirname "$(readlink "$link")")"
[[ -d "$install_bin" ]] || { printf 'reinstall-dev: install dir %s missing.\n' "$install_bin" >&2; exit 1; }
printf 'reinstall-dev: target install dir = %s (target-triple %s)\n' "$install_bin" "$TARGET"

# --- 1. build the overlay UI dist (the overlay binary embeds it) ---
printf 'reinstall-dev: building overlay UI dist…\n'
( cd crates/cue-meeting-overlay/ui && npx vite build )

# --- 2. build the Rust binaries (arm64 + STT feature) ---
printf 'reinstall-dev: building daemon + CLI + overlay (%s, %s)…\n' "$TARGET" "$DAEMON_FEATURES"
cargo build --target "$TARGET" --features "$DAEMON_FEATURES" \
  -p cue-daemon -p cue-cli -p cue-meeting-overlay

# --- 2b. build the BlueyShot.app screenshot helper (macOS) ---
# The overlay launches this bundle for "Take a screenshot": a bare binary can't
# hold the Screen Recording TCC grant, only an .app bundle can (proven by
# BlueyAudio.app). Built + staged beside the overlay so it resolves at runtime.
if [[ "$(uname -s)" == "Darwin" ]]; then
  printf 'reinstall-dev: building BlueyShot.app screenshot helper…\n'
  bash native/macos/cue-shot/build.sh >/dev/null
fi

out="$REPO/target/$TARGET/debug"

# --- 3. stop the running daemon BEFORE overwriting its binary ---
if [[ "$start_after" -eq 1 ]] && "$link" status >/dev/null 2>&1; then
  printf 'reinstall-dev: stopping running daemon…\n'
  "$link" off >/dev/null 2>&1 || true
fi
pkill -f cue-meeting-overlay 2>/dev/null || true
sleep 1

# --- 4. copy the fresh binaries over the install ---
for b in bluey bluey-daemon cue-meeting-overlay; do
  if [[ -x "$out/$b" ]]; then
    cp "$out/$b" "$install_bin/$b"
    printf 'reinstall-dev: installed %s\n' "$b"
  else
    printf 'reinstall-dev: WARN missing build artifact %s (skipped)\n' "$out/$b" >&2
  fi
done

# --- 4a. stage BlueyShot.app beside the overlay in BOTH the target dir (where
# the daemon launches the overlay from) and the install dir, so the overlay's
# bundle resolver finds it at runtime. ---
if [[ "$(uname -s)" == "Darwin" && -d native/macos/cue-shot/.build/BlueyShot.app ]]; then
  for dest in "$out" "$install_bin"; do
    rm -rf "$dest/BlueyShot.app"
    cp -R native/macos/cue-shot/.build/BlueyShot.app "$dest/" \
      && printf 'reinstall-dev: staged BlueyShot.app in %s\n' "$dest"
  done
fi

# --- 4b. STABLE codesign identity for the daemon (TCC persistence) ---------
# macOS TCC keys a permission grant (Screen Recording, Microphone) to the code
# signature's IDENTIFIER. An ad-hoc `codesign --sign -` derives that identifier
# from a per-build content hash (bluey_daemon-<hash>), so EVERY rebuild looks
# like a brand-new app and the user's grant does not carry over — the "screen
# capture failed or was denied" + duplicate "bluey-daemon" rows in Settings. We
# pin a STABLE identifier so the grant sticks across rebuilds. macOS still
# prompts once the first time this identifier appears; never again after.
if command -v codesign >/dev/null 2>&1; then
  codesign --force --sign - --identifier "sh.bluey.daemon" "$install_bin/bluey-daemon" \
    >/dev/null 2>&1 \
    && printf 'reinstall-dev: signed bluey-daemon with stable identifier sh.bluey.daemon\n' \
    || printf 'reinstall-dev: WARN stable codesign of bluey-daemon failed\n' >&2
  # The OVERLAY captures the screenshot now (Screen Recording is keyed to the
  # capturing process), so it needs the same stable-identifier treatment or its
  # grant resets on every rebuild too. IMPORTANT: the daemon launches the overlay
  # from the TARGET dir (target/<triple>/debug/cue-meeting-overlay), not the
  # install dir — so sign BOTH copies, or the running process keeps a hash id.
  for overlay_bin in "$install_bin/cue-meeting-overlay" "$out/cue-meeting-overlay"; do
    if [[ -x "$overlay_bin" ]]; then
      codesign --force --sign - --identifier "sh.bluey.overlay" "$overlay_bin" \
        >/dev/null 2>&1 \
        && printf 'reinstall-dev: signed %s with stable identifier sh.bluey.overlay\n' "$overlay_bin" \
        || printf 'reinstall-dev: WARN stable codesign of %s failed\n' "$overlay_bin" >&2
    fi
  done
fi

# --- 5. (re)start ---
if [[ "$start_after" -eq 1 ]]; then
  printf 'reinstall-dev: starting daemon…\n'
  "$link" on || true
  printf 'reinstall-dev: done - %s now runs the fresh build.\n' "'bluey on'"
else
  printf 'reinstall-dev: done (not started; run %s when ready).\n' "'bluey on'"
fi
