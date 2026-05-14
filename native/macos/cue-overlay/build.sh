#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
swift build -c release
mkdir -p .build
cp "$(swift build -c release --show-bin-path)/cue-overlay" .build/bluey-overlay-macos
echo ".build/bluey-overlay-macos"
