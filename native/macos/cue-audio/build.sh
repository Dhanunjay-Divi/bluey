#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
swift_args=(-c release --disable-automatic-resolution --product cue-audio)
if [[ -n "${BLUEY_SWIFT_ARCH:-}" ]]; then
  swift_args+=(--arch "$BLUEY_SWIFT_ARCH")
fi
swift build "${swift_args[@]}"
mkdir -p .build
bin_dir="$(swift build "${swift_args[@]}" --show-bin-path)"
test -x "$bin_dir/cue-audio"
install -m 0755 "$bin_dir/cue-audio" .build/bluey-audio-macos
install -m 0755 .build/bluey-audio-macos .build/adriverb
install -m 0755 .build/bluey-audio-macos .build/audio-driver
install -m 0755 .build/bluey-audio-macos .build/cue-audio-macos
echo ".build/adriverb"
