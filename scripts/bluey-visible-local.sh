#!/usr/bin/env bash
set -euo pipefail

# Local QA helper: restart Bluey with the overlay visible to screenshots/recording.
# This is intentionally guarded by three dev-only flags and should never be used
# from production deploy or release scripts.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BLUEY_BIN="${BLUEY_BIN:-$HOME/.bluey/bin/bluey}"

if [ ! -x "$BLUEY_BIN" ]; then
    if [ -x "$ROOT/target/release/bluey" ]; then
        BLUEY_BIN="$ROOT/target/release/bluey"
    elif [ -x "$ROOT/target/debug/bluey" ]; then
        BLUEY_BIN="$ROOT/target/debug/bluey"
    else
        echo "bluey binary not found. Build or install Bluey first." >&2
        exit 1
    fi
fi

"$BLUEY_BIN" off >/dev/null 2>&1 || true
for _ in $(seq 1 40); do
    if ! "$BLUEY_BIN" status >/dev/null 2>&1; then
        break
    fi
    sleep 0.15
done

if "$BLUEY_BIN" status >/dev/null 2>&1; then
    echo "Bluey daemon did not stop cleanly before visible restart." >&2
    exit 1
fi

BLUEY_DEV_OVERLAY=1 \
BLUEY_LOCAL_VISIBLE_OVERLAY=1 \
BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 \
"$BLUEY_BIN" on "$@"

echo "Bluey is running in local visible overlay mode."
echo "Return to normal capture-excluded mode with: bluey off && bluey on"
