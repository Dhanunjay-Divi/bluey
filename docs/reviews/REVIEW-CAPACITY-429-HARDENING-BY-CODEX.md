# REVIEW: Capacity / 429 Hardening

**Branch:** `codex/bluey-ai-site`
**Reviewed range:** `6b79880`, `febf0f2`, `53aece5`
**Reviewer:** Codex
**Date:** 2026-06-13
**Source handoff:** `docs/rounds/CAPACITY-429-HARDENING-FOR-CODEX-REVIEW.md`

## Verdict

🟢 **ACCEPT after Codex fix** — the 429/capacity hardening direction is sound.
I found one real P1 implementation miss in the output-token clamp, fixed it
locally, and verified the server gates after the patch.

## Findings

### P1 — OpenAI routes bypassed the new non-thinking output clamp — FIXED

`6b79880` added `effective_max_output_tokens()` and used it for Anthropic
requests and server-side entry/cost estimation. OpenAI non-streaming and
streaming requests still passed the raw desktop-supplied `max_tokens` directly
into `openai_token_limit_fields()`. That meant an Instant or Vision OpenAI lane
could still send `max_tokens=8000` / `max_completion_tokens=8000` upstream while
the server estimated and capacity-checked it as 2048. This left the TPM/cost
safety valve incomplete on the most common fast lane.

Codex fixed this in `server/src/routing/dispatcher.rs` by routing OpenAI limit
serialization through the same effective budget helper before choosing the
legacy `max_tokens` field vs GPT-5-family `max_completion_tokens`. Added
regressions:

- `openai_limit_fields_use_effective_output_budget`
- `openai_thinking_lanes_keep_larger_output_budget`

### N1 — Playbook says 503, current helper returns 429 — non-blocking

`docs/PROVIDER-429-PLAYBOOK.md` says capacity exhaustion should return 503 with
`retry_after_secs`, but `capacity_error()` currently returns 429 with the same
structured retry metadata. This is not blocking: the API response is still
machine-readable and the desktop/cloud client now handles capacity-busy 429/503
paths. Still, the doc or helper should be reconciled in a later cleanup so new
call sites do not cargo-cult the wrong status-code expectation.

## What I Verified

- `effective_max_output_tokens()` clamps non-thinking lanes and preserves deep
  thinking budgets.
- OpenAI request fields now use the clamped/effective output budget for both
  legacy and GPT-5-style token-limit fields.
- `key_candidates_from_pool()` now returns a deterministic per-request
  Fisher-Yates permutation, preserving retry stability while fanning displaced
  load across healthy keys.
- Managed LLM, streaming LLM, embed, and STT server paths all resolve upstream
  candidates with request-scoped shard keys before `provider_health.choose_key`.
- The playbook's code rules match the current managed dispatch shape:
  candidate routes, provider health, per-provider rate checks, cooldown
  recording, bounded token budgets, and retry metadata on capacity exhaustion.

## Verification

```
cargo fmt --all --check
cd server && cargo test openai_limit_fields_use_effective_output_budget -- --nocapture
cd server && cargo test openai_thinking_lanes_keep_larger_output_budget -- --nocapture
cd server && cargo test non_thinking_output_is_clamped_to_ceiling -- --nocapture
cd server && cargo test
cd server && cargo clippy --all-targets -- -D warnings
python3 scripts/analyze-tracing-calls.py --check-only
bash scripts/observability-acceptance-smoke.sh
git diff --check
```

Result: all passed. Server test count after the Codex regression tests is
`130` unit tests plus `33` integration tests.

## Notes For Kiro

At review/fix time, I did not touch the pre-existing local dirty files in
`server/src/api/auth_routes.rs`, `server/src/api/stt.rs`, or
`server/tests/integration_e2e.rs`, and I left `bluey-dev.db` untracked.

Post-review cleanup: after the user asked me to audit the remaining dirty tree,
I inspected those three tracked files, confirmed they were rustfmt-only
line-wrap changes, and committed them separately as
`c782b5f style(server): apply rustfmt cleanup`. `bluey-dev.db` remains
local/untracked and intentionally untouched.
