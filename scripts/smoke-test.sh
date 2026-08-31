#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
if [[ -z "${BLUEY_TEST_WORKSPACE_ROOT:-}" \
  || ! -f "$BLUEY_TEST_WORKSPACE_ROOT/.bluey-test-workspace" ]]; then
  exec bash "$ROOT/scripts/run-bluey-tests.sh" -- bash "$ROOT/scripts/smoke-test.sh" "$@"
fi
cd "$ROOT"

SMOKE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/bluey-smoke.XXXXXX")"
export BLUEY_DATA_DIR="$SMOKE_ROOT/data"
export BLUEY_CONFIG_DIR="$SMOKE_ROOT/config"
export BLUEY_RUNTIME_DIR="$SMOKE_ROOT/runtime"
export BLUEY_DAEMON_ADDR="127.0.0.1:$((59000 + ($$ % 5000)))"
export BLUEY_SKIP_SIGNIN_OPEN=1
SMOKE_CONTEXT_FILE="$SMOKE_ROOT/context.rs"
printf 'fn main() { println!("bluey context smoke"); }\n' >"$SMOKE_CONTEXT_FILE"

cargo build -p cue-daemon --bin bluey-daemon >/dev/null
cargo build -p cue-cli --bin bluey >/dev/null

BLUEY_BIN="$CARGO_TARGET_DIR/debug/bluey"
export BLUEY_DAEMON_BIN="$CARGO_TARGET_DIR/debug/bluey-daemon"
test -x "$BLUEY_BIN"
test -x "$BLUEY_DAEMON_BIN"

# This Rust smoke verifies daemon/CLI orchestration. Native helpers have their
# own platform build and protocol gates; compiling them here would write Swift
# scratch output outside the disposable workspace. Put a minimal protocol peer
# inside the disposable workspace and select it through Bluey's debug-only
# helper override. Release builds continue to ignore helper overrides.
SMOKE_OVERLAY="$SMOKE_ROOT/bluey-overlay-smoke.sh"
case "$(uname -s)" in
  Darwin) SMOKE_PLATFORM=macos ;;
  MINGW* | MSYS* | CYGWIN*) SMOKE_PLATFORM=windows ;;
  *) SMOKE_PLATFORM=linux ;;
esac
cat >"$SMOKE_OVERLAY" <<SH
#!/bin/sh
printf '{"type":"ready","token":"%s","platform":"$SMOKE_PLATFORM","capture_excluded":true}\n' \
  "\$BLUEY_OVERLAY_SESSION_TOKEN"
cat
SH
chmod 0700 "$SMOKE_OVERLAY"
export BLUEY_OVERLAY_BIN="$SMOKE_OVERLAY"

"$BLUEY_BIN" off >/dev/null 2>&1 || true
trap '"$BLUEY_BIN" off >/dev/null 2>&1 || true' EXIT

assert_contains() {
  local actual="$1"
  local expected="$2"
  if [[ "$actual" != *"$expected"* ]]; then
    printf 'Bluey smoke test failed. Expected output to contain: %s\n\nActual output:\n%s\n' "$expected" "$actual" >&2
    exit 1
  fi
}

on_output="$("$BLUEY_BIN" on --title "Bluey smoke test")"
"$BLUEY_BIN" instructions set "Answer briefly and mention implementation risks." >/dev/null
"$BLUEY_BIN" listen --speaker system "What is the plan for Bluey?" >/dev/null
"$BLUEY_BIN" listen --speaker user "Action item: I will verify the native overlay." >/dev/null
"$BLUEY_BIN" listen --speaker system "We decided to keep Bluey Rust native and avoid Electron." >/dev/null
"$BLUEY_BIN" context add "$SMOKE_CONTEXT_FILE" --title "context.rs" --note "smoke-test code context" >/dev/null

instructions_output="$("$BLUEY_BIN" instructions show)"
context_output="$("$BLUEY_BIN" context list)"
actions_output="$("$BLUEY_BIN" action-items)"
recap_output="$("$BLUEY_BIN" recap)"
memory_output="$("$BLUEY_BIN" memory search native overlay)"
audio_status_output="$("$BLUEY_BIN" audio status)"
audio_runtime_output="$("$BLUEY_BIN" audio status)"
ai_status_output="$("$BLUEY_BIN" ai status)"
cloud_status_output="$("$BLUEY_BIN" cloud status)"
set +e
unlinked_ask_output="$("$BLUEY_BIN" ask "what are the action items?" 2>&1)"
unlinked_ask_status=$?
set -e
"$BLUEY_BIN" audio stop >/dev/null
end_output="$("$BLUEY_BIN" meeting end)"

assert_contains "$on_output" "Bluey is on."
assert_contains "$instructions_output" "Answer briefly"
assert_contains "$context_output" "context.rs"
assert_contains "$context_output" "smoke-test code context"
assert_contains "$actions_output" "[you] I will verify the native overlay."
assert_contains "$recap_output" "We decided to keep Bluey Rust native and avoid Electron."
assert_contains "$recap_output" "I will verify the native overlay."
assert_contains "$recap_output" "context.rs"
assert_contains "$recap_output" "Answer briefly"
assert_contains "$memory_output" "Bluey smoke test"
assert_contains "$audio_status_output" "Audio pipeline"
assert_contains "$audio_runtime_output" "Audio pipeline"
assert_contains "$ai_status_output" "AI routing"
assert_contains "$ai_status_output" "bluey_managed"
assert_contains "$cloud_status_output" "Cloud sync"
assert_contains "$cloud_status_output" "cloud RAG"
assert_contains "$end_output" "Meeting: Bluey smoke test"
if [[ "$unlinked_ask_status" -eq 0 ]]; then
  printf 'Bluey smoke test failed: an isolated signed-out ask unexpectedly reached a provider.\n' >&2
  exit 1
fi
assert_contains "$unlinked_ask_output" "Bluey cloud account is not linked"

if ! "$BLUEY_BIN" off >/dev/null 2>&1; then
  printf 'Bluey smoke test failed: daemon did not accept authenticated shutdown.\n' >&2
  exit 1
fi
daemon_stopped=0
for _ in {1..40}; do
  if ! "$BLUEY_BIN" status >/dev/null 2>&1; then
    daemon_stopped=1
    break
  fi
  sleep 0.1
done
if [[ "$daemon_stopped" -ne 1 ]]; then
  printf 'Bluey smoke test failed: daemon remained reachable after shutdown.\n' >&2
  exit 1
fi
trap - EXIT

printf 'Bluey smoke test passed: daemon, overlay protocol, transcript, instructions, context, memory, audio status, routing/cloud scaffolds, signed-out provider fence, action-items, recap, archive, and shutdown all worked.\n'
