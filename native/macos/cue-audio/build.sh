#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
mkdir -p .build
swiftc -O \
  -framework AVFoundation \
  -framework CoreMedia \
  -framework ScreenCaptureKit \
  main.swift \
  -o .build/bluey-audio-macos
cp .build/bluey-audio-macos .build/cue-audio-macos
echo ".build/bluey-audio-macos"
