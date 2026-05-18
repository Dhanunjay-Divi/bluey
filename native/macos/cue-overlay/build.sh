#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
ROOT="$(cd "$SCRIPT_DIR/../../.." && pwd)"

cd "$SCRIPT_DIR"
swift build -c release
mkdir -p .build
BIN="$(swift build -c release --show-bin-path)/cue-overlay"
cp "$BIN" .build/bluey-overlay-macos
cp "$BIN" .build/cue-overlay-macos

for profile in debug release; do
  target_dir="$ROOT/target/$profile"
  if [[ -d "$target_dir" ]]; then
    cp "$BIN" "$target_dir/bluey-overlay-macos"
    cp "$BIN" "$target_dir/cue-overlay-macos"
  fi
done

echo ".build/bluey-overlay-macos"
