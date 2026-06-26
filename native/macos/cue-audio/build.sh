#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
swift_args=(-c release)
if [[ -n "${BLUEY_SWIFT_ARCH:-}" ]]; then
  swift_args+=(--arch "$BLUEY_SWIFT_ARCH")
fi
swift build "${swift_args[@]}"
mkdir -p .build
cp "$(swift build "${swift_args[@]}" --show-bin-path)/cue-audio" .build/bluey-audio-macos
cp .build/bluey-audio-macos .build/cue-audio-macos
echo ".build/bluey-audio-macos"
