#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

if [[ "$(uname -s)" == "Darwin" ]]; then
  bash native/macos/cue-overlay/build.sh >/dev/null
  bash native/macos/cue-audio/build.sh >/dev/null
fi

cargo build >/dev/null
exec ./target/debug/bluey run "$@"
