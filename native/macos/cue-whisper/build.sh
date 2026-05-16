#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")"
swift build -c release
echo "Built: $(swift build -c release --show-bin-path)/CueWhisper"
