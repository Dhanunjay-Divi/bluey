# Managed Capacity + Provider Fallback Pass

Date: 2026-05-24  
Branch: `feat/phase-3-round-12`

## Why

User concern: if 1000 users are using Bluey during realtime calls, a single
provider/API key can hit rate limits and make answers/STT unreliable.

This pass adds the first production safety layer for managed Bluey cloud:
customers do not hit provider keys directly, and server-side buckets protect
both customer fairness and upstream provider capacity.

## What Changed

- Added per-account managed capacity buckets:
  - LLM answers
  - embeddings/RAG writes
  - chunked STT
- Added provider/model buckets:
  - OpenAI chat/vision
  - Anthropic chat
  - OpenAI embeddings
  - Deepgram STT
- Added `/router/embed` and `/router/transcribe` edge rate-limit middleware.
- Changed managed LLM routing from a single route to ordered fallback
  candidates:
  - `instant`: OpenAI -> Anthropic
  - `balanced`: Anthropic -> OpenAI
  - `deep`: Anthropic 3.7 -> OpenAI 4o -> Anthropic 3.5
  - `vision`: OpenAI 4o
- `/router/complete` now:
  - checks account fairness after idempotency reservation,
  - checks provider/model capacity before each upstream attempt,
  - skips busy providers and tries the next candidate,
  - bills against the selected provider/model,
  - records `was_fallback=true` in usage when a fallback route served the call.
- `ApiError` now includes optional `retry_after_secs` for typed 429 responses.
- Documented the capacity env vars in `docs/MODEL-ROUTING.md` and the scaling
  path in `docs/DEPLOYMENT-SCALING.md`.

## Defaults

All limits are in-process for v0.2 alpha and configurable by env:

- `BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN=60`, burst 12
- `BLUEY_LIMIT_ACCOUNT_EMBED_PER_MIN=120`, burst 30
- `BLUEY_LIMIT_ACCOUNT_STT_PER_MIN=120`, burst 30
- `BLUEY_LIMIT_PROVIDER_OPENAI_LLM_PER_MIN=900`, burst 180
- `BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN=300`, burst 60
- `BLUEY_LIMIT_PROVIDER_OPENAI_EMBED_PER_MIN=900`, burst 180
- `BLUEY_LIMIT_PROVIDER_DEEPGRAM_STT_PER_MIN=600`, burst 120

Every env var supports `_BURST`.

## Tests Added

- `router_complete_falls_back_when_preferred_provider_429s`
  - Anthropic returns 429 for a balanced request.
  - Bluey falls back to OpenAI and returns 200.
- `router_complete_enforces_account_burst_before_second_upstream_hit`
  - Per-account burst is set to 1.
  - Second managed answer returns 429 with `account_llm_busy`.
  - Wiremock proves only one upstream provider call happened.
- Unit coverage in `server/src/rate_limit.rs` for account and provider capacity
  reasons/isolation.

## Known Follow-Ups

- Replace in-process governor buckets with Redis/shared counters before running
  multiple server instances.
- Add server-side OpenAI transcription fallback for `/router/transcribe`.
  Desktop streaming STT already has Deepgram -> OpenAI Realtime -> LocalWhisper,
  but the chunked REST server endpoint is still Deepgram-only.
- Add dashboard copy for typed 429 responses: "Bluey is busy, retrying in Ns"
  instead of a generic provider error.
- Add per-tier account limits once pricing tiers are finalized.

## Review Notes For Kiro

Focus on:

- Money path: entry check uses the max estimated cost across candidates, actual
  charge uses the selected provider's pricing.
- Idempotency: capacity rejections release the reservation so the client can
  retry with the same `request_id`.
- Provider fallback: failing/busy preferred providers should not leak raw
  provider details to customers.
- Scaling: the implementation is alpha-safe for one server process; Redis is
  required for true multi-instance global limits.
