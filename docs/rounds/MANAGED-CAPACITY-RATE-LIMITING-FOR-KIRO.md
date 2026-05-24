# Managed Capacity + Provider Fallback Pass

Date: 2026-05-24  
Branch: `feat/phase-3-round-12`

## Why

User concern: if 1000 users are using Bluey during realtime calls, a single
provider/API key can hit rate limits and make answers/STT unreliable.

This pass adds the first production safety layer for managed Bluey cloud:
customers do not hit provider keys directly, and server-side buckets protect
against runaway clients while also protecting upstream provider capacity.
Paid customer usage is governed by wallet balance and provider availability,
not per-account throttling.

## What Changed

- Added optional per-account emergency guardrail buckets, disabled by default:
  - LLM answers
  - embeddings/RAG writes
  - chunked STT
- Changed authenticated router edge buckets to optional env-only guardrails, so
  many paying users behind the same NAT/VPN do not get per-IP throttled.
- Added provider/model buckets:
  - OpenAI chat/vision
  - Anthropic chat
  - OpenAI embeddings
  - Deepgram STT
  - OpenAI STT fallback
- Added provider-approved key-pool selection:
  - `OPENAI_API_KEYS`, `ANTHROPIC_API_KEYS`, and `DEEPGRAM_API_KEYS` can hold
    comma-separated pools.
  - Requests shard deterministically across the configured pool.
  - Single-key env vars remain supported for local/staging.
- Added Redis-backed shared capacity:
  - `BLUEY_REDIS_URL` enables global buckets across server instances.
  - `BLUEY_REDIS_NAMESPACE` separates staging/prod keys.
  - `BLUEY_RATE_LIMIT_REDIS_STRICT=1` makes Redis failures fail closed; default
    is fail-open to the local limiter so realtime work can continue during a
    Redis blip.
- Added `/router/embed` and `/router/transcribe` edge rate-limit middleware.
- Added server-side chunked STT cloud fallback:
  - `/router/transcribe` tries Deepgram `nova-3` first.
  - If Deepgram is busy/unavailable, it falls back to OpenAI
    `gpt-4o-mini-transcribe`.
  - Billing uses the selected provider/model and usage is marked
    `was_fallback=true` when the OpenAI fallback serves the request.
- Changed managed LLM routing from a single route to ordered fallback
  candidates:
  - `instant`: OpenAI -> Anthropic
  - `balanced`: Anthropic -> OpenAI
  - `deep`: Anthropic 3.7 -> OpenAI 4o -> Anthropic 3.5
  - `vision`: OpenAI 4o
- Closed the stale managed `local` resolver edge. Local/Ollama fallback remains
  daemon-only and cannot be priced or dispatched through `bluey-server`.
- `/router/complete` now:
  - checks account runaway-loop guardrails only when explicitly enabled by env,
  - checks provider/model capacity before each upstream attempt,
  - skips busy providers and tries the next candidate,
  - bills against the selected provider/model,
  - records `was_fallback=true` in usage when a fallback route served the call.
- `ApiError` now includes optional `retry_after_secs` for typed 429 responses.
- Documented the capacity env vars in `docs/MODEL-ROUTING.md` and the scaling
  path in `docs/DEPLOYMENT-SCALING.md`.

## Defaults

Provider limits are in-process for v0.2 alpha and configurable by env:

- `BLUEY_LIMIT_PROVIDER_OPENAI_LLM_PER_MIN=900`, burst 180
- `BLUEY_LIMIT_PROVIDER_ANTHROPIC_LLM_PER_MIN=300`, burst 60
- `BLUEY_LIMIT_PROVIDER_OPENAI_EMBED_PER_MIN=900`, burst 180
- `BLUEY_LIMIT_PROVIDER_DEEPGRAM_STT_PER_MIN=600`, burst 120
- `BLUEY_LIMIT_PROVIDER_OPENAI_STT_PER_MIN=600`, burst 120

Account guardrails default to **off**. Set these only during abuse response or
runaway-client mitigation:

- `BLUEY_LIMIT_ACCOUNT_LLM_PER_MIN`
- `BLUEY_LIMIT_ACCOUNT_EMBED_PER_MIN`
- `BLUEY_LIMIT_ACCOUNT_STT_PER_MIN`

Authenticated router per-IP guardrails also default to **off**. Set these only
during endpoint abuse incidents:

- `BLUEY_LIMIT_ROUTER_COMPLETE_PER_MIN`
- `BLUEY_LIMIT_ROUTER_EMBED_PER_MIN`
- `BLUEY_LIMIT_ROUTER_TRANSCRIBE_PER_MIN`

Every capacity env var supports `_BURST`.

## Tests Added

- `router_complete_falls_back_when_preferred_provider_429s`
  - Anthropic returns 429 for a balanced request.
  - Bluey falls back to OpenAI and returns 200.
- `router_complete_enforces_account_burst_before_second_upstream_hit`
  - Optional per-account burst is set to 1 for the test only.
  - Second managed answer returns 429 with `account_llm_busy`.
  - Wiremock proves only one upstream provider call happened.
- Unit coverage in `server/src/rate_limit.rs` for account and provider capacity
  reasons/isolation.
- Unit coverage proving account and authenticated-router edge guardrails are
  disabled by default.

## Known Follow-Ups

- Run production with `BLUEY_REDIS_URL` set before multiple server instances.
- Move provider-key health and capacity to a shared ledger before running
  multiple server instances, so every server sees the same provider budget and
  unhealthy key state.
- Add dashboard copy for typed 429 responses: "Bluey is busy, retrying in Ns"
  instead of a generic provider error.
- Keep optional per-account guardrails operator-only. Do not turn them into
  customer-facing plan quotas unless product explicitly introduces capped plans.

## Review Notes For Kiro

Focus on:

- Money path: entry check uses the max estimated cost across candidates, actual
  charge uses the selected provider's pricing.
- Product semantics: per-account limits are disabled by default. Paying users
  should not hit them in normal realtime use; balance and provider capacity are
  the real usage controls.
- NAT/VPN semantics: authenticated router per-IP limits are disabled by default.
  Provider capacity and wallet balance remain the active controls.
- Idempotency: capacity rejections release the reservation so the client can
  retry with the same `request_id`.
- Provider fallback: failing/busy preferred providers should not leak raw
  provider details to customers.
- Scaling: with `BLUEY_REDIS_URL`, provider capacity is global across multiple
  server instances. Without Redis, the implementation is alpha-safe for one
  server process only.
