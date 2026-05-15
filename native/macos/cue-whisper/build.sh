#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
swift build -c release
echo "Built: .build/release/CueWhisper"
