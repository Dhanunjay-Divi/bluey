#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ "$(uname -s)" == "Darwin" ]]; then
  bash native/macos/cue-overlay/build.sh >/dev/null
  bash native/macos/cue-audio/build.sh >/dev/null
fi

# Build WITH the on-device STT feature — a plain `cargo build` omits
# `cue-daemon/parakeet-stt`, which silently ships an STT-less daemon: `Listen`
# then falls through to the cloud "sign in for transcription" gate and produces
# ZERO transcript, even though on-device STT is the intended keyless default.
# The packaged installer already builds with this feature (scripts/build-macos.sh);
# the dev run-local path must match, or `bluey on` transcribes nothing locally.
cargo build --features cue-daemon/parakeet-stt >/dev/null
exec ./target/debug/bluey run "$@"
