#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
swift build -c release
mkdir -p .build
BIN="$(swift build -c release --show-bin-path)/CueWhisper"
cp "$BIN" .build/cue-whisper
cp "$BIN" .build/bluey-whisper-macos
echo ".build/cue-whisper"
