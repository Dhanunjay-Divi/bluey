#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

SMOKE_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/bluey-smoke.XXXXXX")"
export BLUEY_DATA_DIR="$SMOKE_ROOT/data"
export BLUEY_CONFIG_DIR="$SMOKE_ROOT/config"
export BLUEY_RUNTIME_DIR="$SMOKE_ROOT/runtime"
export BLUEY_DAEMON_ADDR="${BLUEY_DAEMON_ADDR:-127.0.0.1:57329}"
export BLUEY_AUDIO_SIMULATED_ONLY=1
SMOKE_CONTEXT_FILE="$SMOKE_ROOT/context.rs"
printf 'fn main() { println!("bluey context smoke"); }\n' >"$SMOKE_CONTEXT_FILE"

bash native/macos/cue-overlay/build.sh >/dev/null
bash native/macos/cue-audio/build.sh >/dev/null
cargo build >/dev/null

./target/debug/bluey stop >/dev/null 2>&1 || true
trap './target/debug/bluey stop >/dev/null 2>&1 || true; rm -rf "$SMOKE_ROOT"' EXIT

assert_contains() {
  local actual="$1"
  local expected="$2"
  if [[ "$actual" != *"$expected"* ]]; then
    printf 'Bluey smoke test failed. Expected output to contain: %s\n\nActual output:\n%s\n' "$expected" "$actual" >&2
    exit 1
  fi
}

on_output="$(./target/debug/bluey on --title "Bluey smoke test")"
./target/debug/bluey instructions set "Answer briefly and mention implementation risks." >/dev/null
./target/debug/bluey listen --speaker system "What is the plan for Bluey?" >/dev/null
./target/debug/bluey listen --speaker user "Action item: I will verify the native overlay." >/dev/null
./target/debug/bluey listen --speaker system "We decided to keep Bluey Rust native and avoid Electron." >/dev/null
./target/debug/bluey context add "$SMOKE_CONTEXT_FILE" --title "context.rs" --note "smoke-test code context" >/dev/null

ask_output="$(./target/debug/bluey ask "what are the action items?")"
instructions_output="$(./target/debug/bluey instructions show)"
context_output="$(./target/debug/bluey ask "what code context is attached?")"
actions_output="$(./target/debug/bluey action-items)"
recap_output="$(./target/debug/bluey recap)"
memory_output="$(./target/debug/bluey memory search native overlay)"
audio_status_output="$(./target/debug/bluey audio status)"
audio_start_output="$(./target/debug/bluey audio start)"
sleep 2
audio_runtime_output="$(./target/debug/bluey audio status)"
ai_status_output="$(./target/debug/bluey ai status)"
cloud_status_output="$(./target/debug/bluey cloud status)"
./target/debug/bluey audio stop >/dev/null
end_output="$(./target/debug/bluey meeting end)"

assert_contains "$on_output" "Bluey is on."
assert_contains "$ask_output" "I will verify the native overlay."
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
assert_contains "$audio_start_output" "system"
assert_contains "$audio_start_output" "microphone"
assert_contains "$audio_start_output" "development simulator available: yes"
assert_contains "$audio_runtime_output" "transcript segments emitted"
assert_contains "$audio_runtime_output" "chunks:"
assert_contains "$ai_status_output" "AI routing"
assert_contains "$ai_status_output" "bluey_managed"
assert_contains "$cloud_status_output" "Cloud sync"
assert_contains "$cloud_status_output" "cloud RAG"
assert_contains "$end_output" "Meeting: Bluey smoke test"

printf 'Bluey smoke test passed: daemon, overlay, transcript, instructions, context, memory, audio scaffold, AI routing scaffold, cloud scaffold, ask, action-items, recap, and archive all worked.\n'
