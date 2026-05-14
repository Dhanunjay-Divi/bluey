#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
swift build -c release
mkdir -p .build
cp "$(swift build -c release --show-bin-path)/cue-audio" .build/bluey-audio-macos
cp .build/bluey-audio-macos .build/cue-audio-macos
echo ".build/bluey-audio-macos"
