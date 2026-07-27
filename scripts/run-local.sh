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
# The packaged installer also includes the cloud calendar provider; keeping both
# features here prevents onboarding from calling a daemon that can only reply
# "cloud calendar not built".
cargo build \
  --features cue-daemon/parakeet-stt,cue-daemon/cloud-calendar >/dev/null
exec ./target/debug/bluey run "$@"
