#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

log() { printf '\n== %s ==\n' "$1"; }
ok() { printf 'ok: %s\n' "$1"; }
skip() { printf 'skip: %s\n' "$1"; }

API_BASE="${BLUEY_API_BASE:-https://bluey.sh}"
API_TOKEN="${BLUEY_API_TOKEN:-}"
RUN_PAID="${BLUEY_RUN_PAID_SMOKE:-0}"

log "Server routing evals"
cargo test --manifest-path "$ROOT/server/Cargo.toml" answer_plan --quiet
cargo test --manifest-path "$ROOT/server/Cargo.toml" web_search --quiet
cargo test --manifest-path "$ROOT/server/Cargo.toml" streaming_idempotency_guard --quiet
ok "AnswerPlan, web-search guard, and streaming idempotency tests passed"

log "Deploy configuration preflight"
bash "$ROOT/scripts/bluey-cloud-preflight.sh"
ok "cloud preflight passed"

log "macOS overlay parse"
if command -v swiftc >/dev/null 2>&1; then
  swiftc -parse "$ROOT/native/macos/cue-overlay/Sources/cue-overlay/main.swift"
  ok "macOS overlay Swift parses"
else
  skip "swiftc not available"
fi

log "Windows overlay syntax"
if command -v x86_64-w64-mingw32-g++ >/dev/null 2>&1; then
  x86_64-w64-mingw32-g++ -x c++ -std=c++17 -fsyntax-only \
    "$ROOT/native/windows/cue-overlay/main.c" \
    -I"$ROOT/native/windows/cue-overlay" \
    -DUNICODE -D_UNICODE
  ok "Windows overlay C++ syntax check passed"
else
  skip "x86_64-w64-mingw32-g++ not available"
fi

log "Public API health"
curl -fsS "$API_BASE/health" >/dev/null
ok "$API_BASE/health is reachable"

if [ -z "$API_TOKEN" ]; then
  skip "BLUEY_API_TOKEN not set; authenticated staging checks skipped"
  exit 0
fi

log "Authenticated account"
curl -fsS "$API_BASE/account/me" \
  -H "Authorization: Bearer $API_TOKEN" >/dev/null
ok "account/me succeeds"

if [ "$RUN_PAID" != "1" ]; then
  skip "BLUEY_RUN_PAID_SMOKE=1 not set; paid provider smoke skipped"
  exit 0
fi

if ! command -v jq >/dev/null 2>&1; then
  skip "jq not available; paid provider smoke skipped"
  exit 0
fi

log "Paid managed answer smoke"
request_id="staging-smoke-$(date +%s)-$$"
payload="$(jq -nc \
  --arg request_id "$request_id" \
  --arg user $'Question:\nCan you write Fibonacci series in Python?' \
  '{
    request_id: $request_id,
    system: "You are Bluey. Answer compactly.",
    user: $user,
    lane: "balanced",
    max_tokens: 160,
    temperature: 0.2
  }')"
response="$(curl -fsS "$API_BASE/router/complete" \
  -H "Authorization: Bearer $API_TOKEN" \
  -H "Content-Type: application/json" \
  -d "$payload")"
printf '%s\n' "$response" | jq -e '.text | contains("fibonacci") or contains("Fibonacci")' >/dev/null
printf '%s\n' "$response" | jq -e '.artifact_type == "code"' >/dev/null
ok "paid managed answer produced a code artifact"
