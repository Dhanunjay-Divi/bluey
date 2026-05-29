# Provider Capacity Hardening - 2026-05-29

## Why

Paid Bluey users should not feel provider 429s as a normal part of the product.
Wallet balance controls spending; provider capacity management is Bluey's job.
Per-account limits remain emergency abuse/runaway-client guardrails only.

## Implemented

- Added `server/src/provider_health.rs`.
- Added a local + Redis-backed provider/model/key cooldown ledger.
- Added `UpstreamKeys::key_candidates()` so managed routing can inspect every
  approved key in a pool without logging raw secrets.
- Added SHA-256 key fingerprints for logs and cooldown keys.
- Preserved existing single-key env vars and comma-separated key pools:
  `OPENAI_API_KEYS`, `ANTHROPIC_API_KEYS`, `DEEPGRAM_API_KEYS`.
- Changed LLM `/router/complete` to:
  - choose a healthy key for each provider/model route,
  - call the upstream with that exact key,
  - parse provider Retry-After on 429/529,
  - cool the exact provider/model/key,
  - immediately retry the next healthy key or next provider route.
- Changed `/router/embed` and `/router/transcribe` to use the same key-health
  path.
- Added structured upstream HTTP errors so 429s are no longer hidden inside
  stringified `anyhow` messages.
- Added `/admin/metrics` provider-health counters:
  `bluey_provider_key_cooldowns_total`,
  `bluey_provider_key_all_cooling_total`, and
  `bluey_provider_health_redis_errors_total`.
- Updated `docs/MODEL-ROUTING.md`.

## Operational knobs

```bash
BLUEY_REDIS_URL=redis://127.0.0.1:6379
BLUEY_REDIS_NAMESPACE=bluey-prod
BLUEY_PROVIDER_429_COOLDOWN_SECS=30
BLUEY_PROVIDER_MAX_COOLDOWN_SECS=300
OPENAI_API_KEYS=...
ANTHROPIC_API_KEYS=...
DEEPGRAM_API_KEYS=...
```

Redis is required before running multiple `bluey-server` instances. Without
Redis, cooldowns are process-local and suitable only for local/dev or a
single-server alpha.

## Tests added

- `provider_health::tests::choose_key_skips_cooling_candidate`
- `provider_health::tests::choose_key_reports_retry_when_all_candidates_are_cooling`
- `provider_health::tests::snapshot_counts_cooldowns_and_all_keys_cooling`
- `router_complete_retries_next_openai_key_on_429_without_customer_wait`

## Verification

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
cd server && cargo clippy --all-targets -- -D warnings
cd server && cargo test
git diff --check
```

Results on 2026-05-29:

- Root workspace clippy clean.
- Root workspace tests: 422 passed, expected ignored hardware/keychain tests only.
- Server clippy clean.
- Server tests: 118 passed.
- Diff whitespace check clean.

## Remaining scale work

- Add an admin capacity dashboard that shows provider/model/key health by
  fingerprint.
- Add labelled provider/model breakdowns for cooldown metrics if Prometheus
  cardinality remains acceptable in staging.
- Add a shared priority queue for background work so embeddings/RAG backfills
  yield to realtime audio and answers.
- Add true upstream SSE streaming through `bluey-server`; current server
  streaming still uses the completed-response chunker.
- Add provider allocation runbook: approved provider limits, key pool ownership,
  and emergency failover procedure.

## What To Tell Kiro / Next Agent

Provider buckets and key pools now have a health-aware routing layer plus basic
Prometheus counters. Continue with admin visibility next: active STT sessions,
provider capacity utilization, and key-health views by fingerprint. Do not add
normal customer-facing per-account throttles; use wallet balance for spend and
provider health for availability.
