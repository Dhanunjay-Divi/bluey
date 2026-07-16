#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
swift_args=(-c release --disable-automatic-resolution)
if [[ -n "${BLUEY_SWIFT_ARCH:-}" ]]; then
  swift_args+=(--arch "$BLUEY_SWIFT_ARCH")
fi
swift build "${swift_args[@]}"
mkdir -p .build
BIN="$(swift build "${swift_args[@]}" --show-bin-path)/CueWhisper"
cp "$BIN" .build/cue-whisper
cp "$BIN" .build/bluey-whisper-macos
echo ".build/cue-whisper"
