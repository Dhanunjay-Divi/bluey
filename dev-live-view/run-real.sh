#!/usr/bin/env bash
# REAL live system: captures your actual Mac SYSTEM AUDIO (a YouTube video, a real
# call), runs on-device Parakeet STT + speakrs speaker diarization live, and
# streams real transcript + real "Speaker N" labels + the verified decisions
# ledger to the HTML view (dev-live-view/index.html) over the WebSocket.
#
# This is the real thing — NOT the scripted mock. It needs:
#   - the daemon built with BOTH features: parakeet-stt (STT) + diarize (speakers)
#   - the native cue-audio ScreenCaptureKit helper (Screen Recording TCC grant)
#   - arm64 OpenBLAS (Homebrew) for speakrs
#
# First run will prompt for Screen Recording permission for the cue-audio helper.
set -uo pipefail

ROOT="/Users/ms/Developer/Bluey"
export PKG_CONFIG_PATH="/opt/homebrew/opt/openblas/lib/pkgconfig:${PKG_CONFIG_PATH:-}"
TGT="aarch64-apple-darwin"
BIN="$ROOT/target/$TGT/debug"
DAEMON="$BIN/bluey-daemon"
CLI="$BIN/bluey"

# --- 1. Ensure the real (parakeet-stt + diarize) binaries exist -------------
if [ ! -x "$DAEMON" ] || [ "${REBUILD:-0}" = "1" ]; then
  echo "=== building real daemon+cli (parakeet-stt + diarize, arm64) — one time ==="
  ( cd "$ROOT" && cargo build -p cue-daemon -p cue-cli \
      --features "cue-daemon/parakeet-stt cue-daemon/diarize" --target "$TGT" ) || {
    echo "build failed"; exit 1; }
fi

# --- 2. Ensure the native system-audio helper is built ----------------------
HELPER="$ROOT/native/macos/cue-audio/.build/release/cue-audio"
if [ ! -x "$HELPER" ]; then
  echo "=== building native cue-audio ScreenCaptureKit helper ==="
  ( cd "$ROOT/native/macos/cue-audio" && swift build -c release ) || {
    echo "helper build failed — system audio needs it"; exit 1; }
fi

PORT=57420
ADDR="127.0.0.1:$PORT"
TMP="$(mktemp -d /tmp/bluey-real-view.XXXXXX)"
export BLUEY_DAEMON_ADDR="$ADDR" CUE_DAEMON_ADDR="$ADDR"
export BLUEY_DATA_DIR="$TMP/data" BLUEY_CONFIG_DIR="$TMP/config" BLUEY_RUNTIME_DIR="$TMP/runtime"
export BLUEY_TRANSCRIPT_WS_ADDR="127.0.0.1:8766"
export BLUEY_DIARIZE=1                    # turn ON live diarization at runtime
export BLUEY_DIARIZE_INTERVAL_SECS="${BLUEY_DIARIZE_INTERVAL_SECS:-15}"  # re-diarize cadence
export BLUEY_LEDGER=1                      # turn ON the verified decisions ledger
export BLUEY_LEDGER_INTERVAL_TURNS="${BLUEY_LEDGER_INTERVAL_TURNS:-8}"
# NOTE: the ledger needs a real cheap LLM lane (OPENAI_API_KEY etc.) to extract.
# Without a key it simply skips — transcript + diarization still work fully. No
# mocking: the ledger only produces output from a real model call.
mkdir -p "$BLUEY_DATA_DIR" "$BLUEY_CONFIG_DIR" "$BLUEY_RUNTIME_DIR"
cat > "$BLUEY_CONFIG_DIR/settings.json" <<'JSON'
{ "default_model":"Bluey Auto","default_mode":"General","answer_style":null,
  "overlay_opacity":0.92,"audio_system_enabled":true,"audio_microphone_enabled":false,
  "cloud_sync_enabled":false,"retention_days":30,"updated_at":"0",
  "my_names":[],"auto_trigger_enabled":false }
JSON

cleanup() {
  "$CLI" audio stop >/dev/null 2>&1 || true
  "$CLI" stop >/dev/null 2>&1 || true
  [ -n "${PID:-}" ] && kill "$PID" >/dev/null 2>&1 || true
  rm -rf "$TMP"
}
trap cleanup EXIT INT TERM

echo "=== starting REAL daemon (STT + diarization + ledger, no overlay) ==="
"$DAEMON" --addr "$ADDR" --no-overlay >"$TMP/daemon.log" 2>&1 &
PID=$!
for i in $(seq 1 80); do "$CLI" status >/dev/null 2>&1 && break; sleep 0.2; done
if ! "$CLI" status >/dev/null 2>&1; then echo "daemon failed:"; cat "$TMP/daemon.log"; exit 1; fi
grep -q "live-transcript WebSocket" "$TMP/daemon.log" && echo "  WebSocket bound ✓ (ws://127.0.0.1:8766)"

echo ""
echo ">>> OPEN THE VIEW:  open $ROOT/dev-live-view/index.html"
echo ">>> (or the Launch preview panel)"
echo ""
sleep 1

"$CLI" meeting start --title "Live system-audio session" >/dev/null 2>&1
echo "=== arming REAL system-audio capture ==="
echo "    (first run: macOS will ask to allow Screen Recording for cue-audio —"
echo "     approve it, then re-run this script)"
AUDIO_OUT="$("$CLI" audio start 2>&1)"; echo "$AUDIO_OUT" | sed 's/^/    /'

echo ""
echo "=== NOW PLAY AUDIO on your Mac (a YouTube video, a call). Watch the browser. ==="
echo "    Transcript appears as words are recognized; ~every ${BLUEY_DIARIZE_INTERVAL_SECS}s the"
echo "    diarizer runs and lines upgrade from 'They' → 'Speaker 1/2/3'."
echo ""
echo "    Live daemon signals (tail):"
# Stream the interesting log lines until Ctrl-C.
tail -f "$TMP/daemon.log" | grep --line-buffered -iE "diariz|speaker|transcript|ledger|ScreenCapture|authorized|error" \
  | sed 's/^/    [log] /' &
TAILPID=$!

while kill -0 "$PID" 2>/dev/null; do sleep 2; done
kill "$TAILPID" 2>/dev/null || true
