#!/usr/bin/env bash
set -euo pipefail

# Local QA helper: restart Bluey with the overlay visible to screenshots/recording.
# This is intentionally guarded by three dev-only flags and should never be used
# from production deploy or release scripts.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BLUEY_BIN="${BLUEY_BIN:-}"

if [ -z "$BLUEY_BIN" ] || [ ! -x "$BLUEY_BIN" ]; then
    if [ -x "$ROOT/target/debug/bluey" ]; then
        BLUEY_BIN="$ROOT/target/debug/bluey"
    elif [ -x "$ROOT/target/release/bluey" ]; then
        BLUEY_BIN="$ROOT/target/release/bluey"
    elif [ -x "$HOME/.bluey/bin/bluey" ]; then
        BLUEY_BIN="$HOME/.bluey/bin/bluey"
    else
        echo "bluey binary not found. Build or install Bluey first." >&2
        exit 1
    fi
fi

if [ "$(uname -s)" = "Darwin" ] && [ -x "$ROOT/native/macos/cue-overlay/build.sh" ]; then
    echo "Building debug macOS overlay for visible local QA..."
    BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug "$ROOT/native/macos/cue-overlay/build.sh"
fi

daemon_bin="${BLUEY_DAEMON_BIN:-}"
if [ -z "$daemon_bin" ]; then
    case "$BLUEY_BIN" in
        "$ROOT"/target/debug/bluey)
            if [ -x "$ROOT/target/debug/bluey-daemon" ]; then
                daemon_bin="$ROOT/target/debug/bluey-daemon"
            fi
            ;;
        "$ROOT"/target/release/bluey)
            if [ -x "$ROOT/target/release/bluey-daemon" ]; then
                daemon_bin="$ROOT/target/release/bluey-daemon"
            fi
            ;;
    esac
fi

overlay_bin="${BLUEY_OVERLAY_BIN:-}"
if [ -z "$overlay_bin" ] && [ "$(uname -s)" = "Darwin" ]; then
    if [ -x "$ROOT/native/macos/cue-overlay/.build/host-overlay" ]; then
        overlay_bin="$ROOT/native/macos/cue-overlay/.build/host-overlay"
    elif [ -x "$ROOT/native/macos/cue-overlay/.build/bluey-overlay-macos" ]; then
        overlay_bin="$ROOT/native/macos/cue-overlay/.build/bluey-overlay-macos"
    elif [ -x "$ROOT/native/macos/cue-overlay/.build/cue-overlay-macos" ]; then
        overlay_bin="$ROOT/native/macos/cue-overlay/.build/cue-overlay-macos"
    fi
fi

daemon_running() {
    local status
    status="$("$BLUEY_BIN" status 2>&1 || true)"
    printf '%s\n' "$status" | grep -q '"pid"' \
        && ! printf '%s\n' "$status" | grep -qi 'IPC is not reachable'
}

"$BLUEY_BIN" off >/dev/null 2>&1 || true
for _ in $(seq 1 40); do
    if ! daemon_running; then
        break
    fi
    sleep 0.15
done

if daemon_running; then
    echo "Bluey daemon did not stop cleanly before visible restart." >&2
    exit 1
fi

BLUEY_DEV_OVERLAY=1 \
BLUEY_OVERLAY_CAPTURE_VISIBLE=1 \
BLUEY_LOCAL_VISIBLE_OVERLAY=1 \
BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 \
BLUEY_DAEMON_BIN="$daemon_bin" \
BLUEY_OVERLAY_BIN="$overlay_bin" \
BLUEY_OVERLAY_FORCE_RAW_HELPER=1 \
"$BLUEY_BIN" on "$@"

for _ in $(seq 1 50); do
    status="$("$BLUEY_BIN" status 2>&1 || true)"
    if printf '%s\n' "$status" | grep -q '"overlay_capture_excluded": false'; then
        echo "Bluey is running in local visible overlay mode."
        echo "Return to normal capture-excluded mode with: $BLUEY_BIN off && $BLUEY_BIN on"
        exit 0
    fi
    sleep 0.1
done

echo "Bluey started, but visible mode did not take effect." >&2
echo "Latest status:" >&2
"$BLUEY_BIN" status >&2 || true
echo "Try rebuilding the debug daemon and overlay, then run this helper again." >&2
exit 1
