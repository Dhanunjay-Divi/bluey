#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
mkdir -p .build
swiftc -O -framework AppKit main.swift -o .build/bluey-overlay-macos
cp .build/bluey-overlay-macos .build/cue-overlay-macos
echo ".build/bluey-overlay-macos"
