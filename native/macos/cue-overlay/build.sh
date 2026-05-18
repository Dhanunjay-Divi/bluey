#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
swift build -c release
mkdir -p .build
BIN="$(swift build -c release --show-bin-path)/cue-overlay"
cp "$BIN" .build/bluey-overlay-macos
cp "$BIN" .build/cue-overlay-macos
echo ".build/bluey-overlay-macos"
