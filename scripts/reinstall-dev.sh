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
# Install location controls:
#   BLUEY_BIN_DIR         symlink dir (default: ~/.local/bin).
#   BLUEY_DEV_INSTALL_BIN bootstrap install bin when the `bluey` link does not
#                         exist (default: ~/.local/bluey-dev/local/bin).
set -euo pipefail

cd "$(dirname "$0")/.."
REPO="$PWD"

# Load local OAuth client credentials (gitignored) so both the build
# (compile-time option_env! fallbacks) and the launched daemon (runtime
# std::env::var) see BLUEY_GOOGLE_CLIENT_ID / BLUEY_GOOGLE_CLIENT_SECRET /
# BLUEY_MICROSOFT_CLIENT_ID without the operator re-exporting them each session.
# Absent file is fine — a release configured another way just skips this.
if [[ -f "$REPO/.bluey.env" ]]; then
  # shellcheck disable=SC1091
  source "$REPO/.bluey.env"
  echo "reinstall-dev: loaded OAuth credentials from .bluey.env"
fi

# On Apple Silicon the default rustup toolchain is x86_64; its emulated binaries
# silently misbehave (ort/Parakeet). ALWAYS build the native arm64 target. On an
# Intel Mac this resolves to the x86_64 triple.
TARGET="$(rustc -vV | awk '/host:/{print $2}')"
# On-device STT lives behind this feature — a plain build ships an STT-less daemon
# that falls through to the cloud "sign in" gate and transcribes nothing.
# local-memory adds keyless cross-meeting facts; cloud-calendar keeps the dev
# install feature-equivalent to the onboarding UI it embeds.
DAEMON_FEATURES="cue-daemon/parakeet-stt cue-daemon/local-memory cue-daemon/cloud-calendar"
# Speaker diarization (cue-daemon/diarize) is OPT-IN via BLUEY_DIARIZE_BUILD=1 —
# it links Homebrew arm64 OpenBLAS + a CoreML backend, so it's off by default
# (a plain reinstall stays lean). With it on, real per-speaker labels replace the
# "You"/"Them" channel fallback. Diarization models (~60MB) download on first use.
if [[ "${BLUEY_DIARIZE_BUILD:-0}" == "1" ]]; then
  DAEMON_FEATURES="$DAEMON_FEATURES cue-daemon/diarize"
  echo "reinstall-dev: diarization ENABLED (linking Homebrew arm64 OpenBLAS)"
fi
# openblas pkg-config for the diarize/daemon link.
export PKG_CONFIG_PATH="${PKG_CONFIG_PATH:-/opt/homebrew/opt/openblas/lib/pkgconfig}"

start_after=1
for arg in "$@"; do
  case "$arg" in
    --no-start) start_after=0 ;;
    *) printf 'reinstall-dev: unknown arg: %s\n' "$arg" >&2; exit 2 ;;
  esac
done

# --- locate the real install dir, bootstrapping a clean dev install if needed ---
bin_dir="${BLUEY_BIN_DIR:-$HOME/.local/bin}"
link="$bin_dir/bluey"
if [[ -L "$link" ]]; then
  link_target="$(readlink "$link")"
  if [[ "$link_target" != /* ]]; then
    link_target="$(cd "$(dirname "$link")" && pwd)/$link_target"
  fi
  install_bin="$(dirname "$link_target")"
else
  install_bin="${BLUEY_DEV_INSTALL_BIN:-$HOME/.local/bluey-dev/local/bin}"
  case "$install_bin" in
    ""|"/"|"$HOME"|"$HOME/")
      printf 'reinstall-dev: refusing unsafe install dir: %s\n' "$install_bin" >&2
      exit 1
      ;;
  esac
  mkdir -p "$install_bin" "$bin_dir"
  ln -sfn "$install_bin/bluey" "$link"
  ln -sfn "$install_bin/bluey-daemon" "$bin_dir/bluey-daemon"
  printf 'reinstall-dev: bootstrapped clean dev links in %s\n' "$bin_dir"
fi
mkdir -p "$install_bin"
printf 'reinstall-dev: target install dir = %s (target-triple %s)\n' "$install_bin" "$TARGET"

# --- 1. build the overlay UI dist (the overlay binary embeds it) ---
printf 'reinstall-dev: installing locked overlay dependencies + building UI dist…\n'
( cd crates/cue-meeting-overlay/ui && npm ci && npm run build )

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
  bash native/macos/cue-shot/build.sh "${BLUEY_CODESIGN_IDENTITY:-}" >/dev/null
fi

# --- 2c. build the BlueyAudio.app mic/system-audio helper (macOS) ---
# The daemon launches this bundle (via `/usr/bin/open`) to capture mic + system
# audio: a bare binary can't hold the Microphone TCC grant and its Info.plist
# NSMicrophoneUsageDescription, only an .app bundle can. Without it staged beside
# the daemon, capture returns SILENT buffers ("mic: audio is flowing but SILENT")
# and the system-audio helper "failed too many times" — transcription gets
# garbage. Built + staged like BlueyShot.app so the resolver finds it at runtime.
if [[ "$(uname -s)" == "Darwin" ]]; then
  printf 'reinstall-dev: building BlueyAudio.app audio helper…\n'
  bash native/macos/cue-audio/bundle-app.sh \
    "${BLUEY_CODESIGN_IDENTITY:-}" >/dev/null
  BLUEY_VERIFY_LAUNCH=1 \
    bash native/macos/cue-audio/verify-app.sh \
    native/macos/cue-audio/.build/BlueyAudio.app
fi

out="$REPO/target/$TARGET/debug"

# --- 3. stop the running daemon BEFORE overwriting its binary ---
# `bluey off` closes only the overlay; it deliberately leaves the daemon alive.
# Copying over that live executable made a "successful" reinstall keep serving
# the old code until a later reboot. Always request the full shutdown, including
# for --no-start, and fail rather than silently installing beneath a stale
# process.
if pgrep -x bluey-daemon >/dev/null 2>&1; then
  printf 'reinstall-dev: stopping running daemon…\n'
  "$link" quit >/dev/null 2>&1 || true
  for _ in $(seq 1 50); do
    pgrep -x bluey-daemon >/dev/null 2>&1 || break
    sleep 0.1
  done
  if pgrep -x bluey-daemon >/dev/null 2>&1; then
    printf 'reinstall-dev: daemon did not stop; refusing to overwrite the live binary.\n' >&2
    exit 1
  fi
fi
pkill -f cue-meeting-overlay 2>/dev/null || true
for helper_name in BlueyAudio BlueyShot; do
  if pgrep -x "$helper_name" >/dev/null 2>&1; then
    printf 'reinstall-dev: stopping detached %s helper…\n' "$helper_name"
    pkill -TERM -x "$helper_name" 2>/dev/null || true
    for _ in $(seq 1 30); do
      pgrep -x "$helper_name" >/dev/null 2>&1 || break
      sleep 0.1
    done
    if pgrep -x "$helper_name" >/dev/null 2>&1; then
      printf 'reinstall-dev: %s did not stop; refusing to replace its bundle.\n' \
        "$helper_name" >&2
      exit 1
    fi
  fi
done

# --- 4. copy the fresh binaries over the install ---
for b in bluey bluey-daemon cue-meeting-overlay; do
  if [[ -x "$out/$b" ]]; then
    cp "$out/$b" "$install_bin/$b"
    printf 'reinstall-dev: installed %s\n' "$b"
  else
    printf 'reinstall-dev: missing required build artifact %s\n' "$out/$b" >&2
    exit 1
  fi
done

# --- 4a. stage BlueyShot.app beside the overlay in BOTH the target dir (where
# the daemon launches the overlay from) and the install dir, so the overlay's
# bundle resolver finds it at runtime. ---
if [[ "$(uname -s)" == "Darwin" ]]; then
  if [[ ! -d native/macos/cue-shot/.build/BlueyShot.app ]]; then
    printf 'reinstall-dev: missing required BlueyShot.app build artifact\n' >&2
    exit 1
  fi
  for dest in "$out" "$install_bin"; do
    rm -rf "$dest/BlueyShot.app"
    cp -R native/macos/cue-shot/.build/BlueyShot.app "$dest/" \
      && printf 'reinstall-dev: staged BlueyShot.app in %s\n' "$dest"
  done
fi

# --- 4a-audio. stage BlueyAudio.app beside the DAEMON (its bundle resolver,
# macos_app_bundle_path, searches current_exe().parent()). Without this the mic +
# system-audio helper can't launch → SILENT capture + "system audio helper failed
# too many times" → garbage STT. bundle-app.sh signs it with a stable certificate
# when one is available. Preserve that signature while staging it; replacing it
# with an ad-hoc signature here would invalidate the existing TCC grant. ---
if [[ "$(uname -s)" == "Darwin" ]]; then
  if [[ ! -d native/macos/cue-audio/.build/BlueyAudio.app ]]; then
    printf 'reinstall-dev: missing required BlueyAudio.app build artifact\n' >&2
    exit 1
  fi
  for dest in "$out" "$install_bin"; do
    rm -rf "$dest/BlueyAudio.app"
    cp -R native/macos/cue-audio/.build/BlueyAudio.app "$dest/" \
      && printf 'reinstall-dev: staged BlueyAudio.app in %s\n' "$dest"
    bash native/macos/cue-audio/verify-app.sh "$dest/BlueyAudio.app"
  done

  # Static codesign verification is not enough: amfid rejects a restricted
  # entitlement without a matching profile only when the executable launches.
  # Probe the exact installed copy through LaunchServices before restarting.
  BLUEY_VERIFY_LAUNCH=1 \
    bash native/macos/cue-audio/verify-app.sh \
    "$install_bin/BlueyAudio.app"
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
  "$link" on
  printf 'reinstall-dev: done - %s now runs the fresh build.\n' "'bluey on'"
else
  printf 'reinstall-dev: done (not started; run %s when ready).\n' "'bluey on'"
fi
