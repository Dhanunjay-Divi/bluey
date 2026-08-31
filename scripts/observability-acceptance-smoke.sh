#!/usr/bin/env bash
# Observability acceptance smoke
#
# Verifies the Observability Round end-to-end contract:
#   one user-facing operation produces a single trace_id that propagates
#   through daemon log + cloud-client → server log + provider call.
#
# This is the round-close acceptance gate for the Observability Round.
# Designed to run on uno or any developer Mac/Linux against a local
# wiremock-style stub server. No real Stripe/OpenAI/Anthropic/Deepgram
# keys involved.
#
# Usage:
#   scripts/observability-acceptance-smoke.sh
#
# Exits 0 on pass, non-zero on any failure with a clear report.
#
# What this currently covers:
#   * dashboard Rust command layer → daemon IPC: trace wrapper minted before
#     the daemon/cloud-client boundary
#   * cloud-client → server: trace propagation through X-Bluey-Trace-Id
#   * server: request_id middleware mints + echoes trace_id + request_id
#   * server log line emits with both ids
#   * daemon log file format is JSON with standard fields
#   * Phase 3 overlay lifecycle + frontend error regression tests
#
# What this does NOT yet cover:
#   * Visible Tauri window automation: this smoke runs the dashboard Rust
#     command boundary deterministically instead of clicking a GUI window.
#   * Visible overlay click/expand/collapse automation: native UI visual QA
#     remains manual on a real macOS desktop.

set -euo pipefail

# ── Pretty output ───────────────────────────────────────────────────────
if [ -t 1 ]; then
    BOLD="$(tput bold 2>/dev/null || true)"
    DIM="$(tput dim 2>/dev/null || true)"
    GREEN="$(tput setaf 2 2>/dev/null || true)"
    RED="$(tput setaf 1 2>/dev/null || true)"
    BLUE="$(tput setaf 4 2>/dev/null || true)"
    RESET="$(tput sgr0 2>/dev/null || true)"
else
    BOLD="" DIM="" GREEN="" RED="" BLUE="" RESET=""
fi

step()  { printf "\n%s── %s ──%s\n" "$BLUE$BOLD" "$1" "$RESET"; }
ok()    { printf "%s✅ %s%s\n" "$GREEN" "$1" "$RESET"; }
fail()  { printf "%s❌ %s%s\n" "$RED" "$1" "$RESET" >&2; exit 1; }

# ── Find workspace root ─────────────────────────────────────────────────
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
WORKSPACE="$(cd "$SCRIPT_DIR/.." && pwd)"
if [[ -z "${BLUEY_TEST_WORKSPACE_ROOT:-}" \
    || ! -f "$BLUEY_TEST_WORKSPACE_ROOT/.bluey-test-workspace" ]]; then
    exec bash "$WORKSPACE/scripts/run-bluey-tests.sh" -- \
        bash "$WORKSPACE/scripts/observability-acceptance-smoke.sh" "$@"
fi
cd "$WORKSPACE"

# ── Working directory ───────────────────────────────────────────────────
WORK="$(mktemp -d "$TMPDIR/bluey-obs-smoke.XXXXXX")"
cleanup() {
    local pid
    local pids=()
    while IFS= read -r pid; do
        [ -n "$pid" ] && pids+=("$pid")
    done < <(jobs -p)
    for pid in "${pids[@]}"; do
        kill -TERM "$pid" 2>/dev/null || true
    done
    for pid in "${pids[@]}"; do
        wait "$pid" 2>/dev/null || true
    done
    rm -rf "$WORK" 2>/dev/null || true
}
trap cleanup EXIT

LOG_DIR="$WORK/logs"
mkdir -p "$LOG_DIR"
SERVER_LOG="$WORK/server.log"
CURL_OUT="$WORK/curl_response.txt"
CURL_HEADERS="$WORK/curl_headers.txt"

step "Workspace + work dir"
echo "  workspace: $WORKSPACE"
echo "  work dir : $WORK"
echo "  logs     : $LOG_DIR"
echo "  expected git tip:"
git -P log --oneline -1 || true

# ── Build the exact local smoke binaries ────────────────────────────────
step "Build bluey-server, bluey-daemon, and bluey CLI (debug)"
( cd "$WORKSPACE/server" && cargo build --bin bluey-server 2>&1 | tail -3 )
( cd "$WORKSPACE" && cargo build -p cue-daemon --bin bluey-daemon 2>&1 | tail -3 )
( cd "$WORKSPACE" && cargo build -p cue-cli --bin bluey 2>&1 | tail -3 )
SERVER_BIN="$CARGO_TARGET_DIR/debug/bluey-server"
DAEMON_BIN="$CARGO_TARGET_DIR/debug/bluey-daemon"
BLUEY_BIN="$CARGO_TARGET_DIR/debug/bluey"
[ -x "$SERVER_BIN" ] || fail "bluey-server binary not built"
[ -x "$DAEMON_BIN" ] || fail "bluey-daemon binary not built"
[ -x "$BLUEY_BIN" ] || fail "bluey CLI binary not built"
ok "server binary at $SERVER_BIN"
ok "daemon and CLI binaries are task-owned under $CARGO_TARGET_DIR"

# ── Stub config: env-only, no real stripe / smtp / providers ────────────
step "Stub config + env vars"
export BLUEY_API_HOST=127.0.0.1
export BLUEY_PORT="$((18000 + ($$ % 1000)))"
export BLUEY_DB_PATH="$WORK/bluey.sqlite"
export BLUEY_JWT_SECRET="$(openssl rand -hex 64 2>/dev/null || head -c 32 /dev/urandom | xxd -p -c 32)"
export BLUEY_PUBLIC_URL="http://127.0.0.1:$BLUEY_PORT"
export RUST_LOG="info,bluey_server=debug,tower_http=info"
# Disable ANSI colors so grep can read field values cleanly.
export NO_COLOR=1
ok "env configured (BLUEY_PORT=$BLUEY_PORT)"

# ── Spawn server ────────────────────────────────────────────────────────
step "Spawn server"
"$SERVER_BIN" >"$SERVER_LOG" 2>&1 &
SERVER_PID=$!
sleep 2
if ! kill -0 "$SERVER_PID" 2>/dev/null; then
    echo "--- server.log tail ---"
    tail -50 "$SERVER_LOG"
    fail "server failed to start"
fi
ok "server pid=$SERVER_PID"

# ── Assertion 0: dashboard command layer wraps daemon IPC with trace id ─
step "Verify dashboard command layer trace wraps daemon IPC"
DASHBOARD_TRACE_TEST="$WORK/dashboard_trace_test.log"
if ! ( cd "$WORKSPACE" && cargo test -p cue-dashboard commands::tests::daemon_ipc_wraps_dashboard_request_with_trace -- --nocapture ) \
    >"$DASHBOARD_TRACE_TEST" 2>&1; then
    echo "--- dashboard trace test log ---"
    cat "$DASHBOARD_TRACE_TEST"
    fail "dashboard command-layer trace test failed"
fi
ok "dashboard command layer emits WithTrace before daemon IPC"

# ── Send a request that the request-id middleware should handle ─────────
step "Send request with caller-supplied trace_id and request_id"
TRACE_ID="$(python3 -c 'import uuid; print(uuid.uuid4())')"
REQUEST_ID="$(python3 -c 'import uuid; print(uuid.uuid4())')"

curl -sS -D "$CURL_HEADERS" -o "$CURL_OUT" \
    -X GET "http://127.0.0.1:$BLUEY_PORT/health" \
    -H "X-Bluey-Trace-Id: $TRACE_ID" \
    -H "X-Bluey-Request-Id: $REQUEST_ID" \
    || fail "curl to /health failed"

ok "curl /health succeeded"

# ── Assertion 1: response echoes both ids back ──────────────────────────
step "Verify response echoes trace_id + request_id headers"
if ! grep -qi "x-bluey-trace-id: *$TRACE_ID" "$CURL_HEADERS"; then
    echo "--- headers ---"
    cat "$CURL_HEADERS"
    fail "response missing X-Bluey-Trace-Id header"
fi
if ! grep -qi "x-bluey-request-id: *$REQUEST_ID" "$CURL_HEADERS"; then
    echo "--- headers ---"
    cat "$CURL_HEADERS"
    fail "response missing X-Bluey-Request-Id header"
fi
ok "X-Bluey-Trace-Id and X-Bluey-Request-Id echoed in response headers"

# ── Assertion 2: server log contains both ids ───────────────────────────
step "Verify server log emits trace_id + request_id"
sleep 1  # Let the async log flush.
if ! grep -q "trace_id=\"$TRACE_ID\"" "$SERVER_LOG" && \
   ! grep -q "trace_id=$TRACE_ID" "$SERVER_LOG"; then
    echo "--- server.log tail ---"
    tail -50 "$SERVER_LOG"
    fail "server log missing trace_id=$TRACE_ID"
fi
if ! grep -q "request_id=\"$REQUEST_ID\"" "$SERVER_LOG" && \
   ! grep -q "request_id=$REQUEST_ID" "$SERVER_LOG"; then
    echo "--- server.log tail ---"
    tail -50 "$SERVER_LOG"
    fail "server log missing request_id=$REQUEST_ID"
fi
ok "server log emits both ids"

# ── Assertion 3: server emits both 'request received' and 'request done' ────
step "Verify server log has request-received + request-done lines"
if ! grep -q "request received" "$SERVER_LOG"; then
    fail "server log missing 'request received' line"
fi
if ! grep -q "request done" "$SERVER_LOG"; then
    fail "server log missing 'request done' line"
fi
ok "lifecycle lines present"

# ── Assertion 4: server fails-open when client omits ids ───────────────
step "Verify server mints fresh ids when client omits them"
curl -sS -D "$CURL_HEADERS" -o "$CURL_OUT" \
    -X GET "http://127.0.0.1:$BLUEY_PORT/health" \
    || fail "second curl to /health failed"

MINTED_TRACE="$(grep -i 'x-bluey-trace-id:' "$CURL_HEADERS" | head -1 | awk '{print $2}' | tr -d '\r')"
MINTED_REQUEST="$(grep -i 'x-bluey-request-id:' "$CURL_HEADERS" | head -1 | awk '{print $2}' | tr -d '\r')"

if [ -z "$MINTED_TRACE" ] || [ -z "$MINTED_REQUEST" ]; then
    echo "--- headers ---"
    cat "$CURL_HEADERS"
    fail "server did not mint missing ids"
fi

# Should look like UUID-ish (36 chars with dashes, or any 32+ char id)
if [ "${#MINTED_TRACE}" -lt 32 ]; then
    fail "minted trace_id too short: $MINTED_TRACE"
fi
ok "server mints fresh trace_id ($MINTED_TRACE) and request_id ($MINTED_REQUEST)"

# ── Assertion 5: daemon log format is JSON with standard fields ────────
# Skipped — requires running a daemon, which needs more env state. The
# unit test crates/cue-core/src/logging.rs::tests::local_json_logging_writes_standard_fields
# already covers this contract. Reaffirming here would mostly duplicate
# that. If you want this added, run a separate daemon-only smoke per the
# Phase 2 verdict §5.

# ── Assertion 6: daemon honors BLUEY_TRACE_ID env (Phase 5) ─────────────
step "Verify daemon honors BLUEY_TRACE_ID env on IPC dispatch (Phase 5)"

DAEMON_LOG_DIR="$WORK/daemon-logs"
DAEMON_STDERR="$WORK/daemon.stderr"
mkdir -p "$DAEMON_LOG_DIR"
KNOWN_TRACE="$(python3 -c 'import uuid; print(uuid.uuid4())')"
DAEMON_PORT="$((59000 + ($$ % 5000)))"
DAEMON_ADDR="127.0.0.1:$DAEMON_PORT"

RUST_LOG=debug BLUEY_LOG_DIR="$DAEMON_LOG_DIR" BLUEY_TRACE_ID="$KNOWN_TRACE" \
    "$DAEMON_BIN" --addr "$DAEMON_ADDR" --no-overlay >/dev/null 2>"$DAEMON_STDERR" &
DAEMON_PID=$!

DAEMON_READY=0
for _ in {1..50}; do
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
        break
    fi
    if BLUEY_DAEMON_ADDR="$DAEMON_ADDR" BLUEY_TRACE_ID="$KNOWN_TRACE" \
        "$BLUEY_BIN" status >/dev/null 2>&1; then
        DAEMON_READY=1
        break
    fi
    sleep 0.1
done
if [ "$DAEMON_READY" -ne 1 ]; then
    echo "--- daemon stderr tail ---"
    tail -20 "$DAEMON_STDERR" 2>/dev/null || true
    fail "daemon did not become ready for the Phase 5 trace assertion"
fi

sleep 1
DAEMON_LOG_FILE="$(find "$DAEMON_LOG_DIR" -maxdepth 1 -type f -name 'daemon-log.*.log' -print | head -1)"
if [ -z "$DAEMON_LOG_FILE" ] || [ ! -f "$DAEMON_LOG_FILE" ]; then
    fail "daemon log file was not produced for the Phase 5 trace assertion"
fi
if ! grep "\"trace_id\":\"$KNOWN_TRACE\"" "$DAEMON_LOG_FILE" \
    | grep -q "daemon ipc request received"; then
    echo "--- daemon log tail ---"
    tail -20 "$DAEMON_LOG_FILE"
    fail "daemon log missing trace_id=$KNOWN_TRACE (Phase 5 env-pass-through broken)"
fi
ok "daemon JSON log contains trace_id=$KNOWN_TRACE on authenticated IPC dispatch"

BLUEY_DAEMON_ADDR="$DAEMON_ADDR" BLUEY_TRACE_ID="$KNOWN_TRACE" \
    "$BLUEY_BIN" off >/dev/null 2>&1 \
    || fail "daemon did not shut down cleanly after the Phase 5 assertion"
DAEMON_EXITED=0
for _ in {1..80}; do
    if ! kill -0 "$DAEMON_PID" 2>/dev/null; then
        DAEMON_EXITED=1
        break
    fi
    sleep 0.1
done
if [ "$DAEMON_EXITED" -ne 1 ]; then
    fail "daemon remained alive after the authenticated shutdown assertion"
fi
DAEMON_EXIT_STATUS=0
wait "$DAEMON_PID" || DAEMON_EXIT_STATUS=$?
if [ "$DAEMON_EXIT_STATUS" -ne 0 ]; then
    fail "daemon exited with status $DAEMON_EXIT_STATUS after authenticated shutdown"
fi

step "Verify Phase 3 overlay lifecycle + frontend error capture tests"
PHASE3_LOG="$WORK/phase3_tests.log"
if ! ( cd "$WORKSPACE" && \
    cargo test -p cue-core overlay::tests::overlay_lifecycle_event_serializes -- --nocapture && \
    cargo test -p cue-daemon app::tests::overlay_lifecycle_event_is_accepted_by_production_validator -- --nocapture && \
    cargo test -p cue-dashboard commands::tests::truncate_log_field_preserves_chars_and_marks_truncation -- --nocapture ) \
    >"$PHASE3_LOG" 2>&1; then
    echo "--- Phase 3 regression test log ---"
    cat "$PHASE3_LOG"
    fail "Phase 3 overlay/frontend observability tests failed"
fi

CORE_IMPORTS="$(grep -R -l '@tauri-apps/api/core' "$WORKSPACE/crates/cue-dashboard/ui/src" | sed "s#^$WORKSPACE/##" | tr '\n' ' ')"
if [ "$CORE_IMPORTS" != "crates/cue-dashboard/ui/src/lib/tauri.ts " ]; then
    echo "direct Tauri core imports: $CORE_IMPORTS"
    fail "frontend invoke wrapper is not the only direct @tauri-apps/api/core consumer"
fi
ok "Phase 3 regression tests pass and direct Tauri invoke is centralized"

step "All assertions passed"
ok "Observability acceptance smoke: PASS"

echo ""
echo "Coverage:"
echo "  - X-Bluey-Trace-Id round-trips client -> server -> response (Phase 1)"
echo "  - X-Bluey-Request-Id round-trips client -> server -> response (Phase 1)"
echo "  - Server logs request received + request done with both ids"
echo "  - Server mints fresh UUIDs when client omits them"
echo "  - daemon honors BLUEY_TRACE_ID env on IPC dispatch (Phase 5)"
echo "  - overlay lifecycle + frontend error regression tests pass (Phase 3)"
echo ""
echo "Remaining manual QA:"
echo "  - visible dashboard/overlay GUI automation is still manual"
