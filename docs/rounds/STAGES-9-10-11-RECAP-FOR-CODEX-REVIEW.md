# Stages 9-11 End-to-End Recap

> **Branch:** `feat/phase-3-round-12`
> **Tip:** `af6bf7c feat(server): Stage 11 per-IP rate limiting`
> **Status:** v0.2 alpha-ready core. Codex round-2 carry-forward nits closed; auto top-up live; rate limiting live.

## What landed in this push

```
af6bf7c feat(server): Stage 11 per-IP rate limiting on auth + router endpoints
d549cce feat(server): Stage 10 auto top-up + /pricing/tiers + duplicate-email atomic 409
0a56beb feat(daemon): Stage 9 daemon ProviderRegistry rewire + 4 carry-forward nits
fbd43e1 fix(cli): actually apply Stage 8 access-only login bridge   (round-2 fix)
3d514f2 fix: codex round-2 blockers — PI normalization + access-only login bridge
```

## Carry-forward nit closure (codex round-2 → now)

| # | Nit | Status | Where |
|---|---|---|---|
| 1 | Stable logical `request_id` into `BlueyManagedProvider` | ✅ closed | Stage 9: `LlmRequest.request_id: Option<String>`, threaded from `cue-dashboard::commands.rs` (response_id), `cue-router::SpeculativeRouter` (per-lane), `BlueyManagedProvider::complete` consumes |
| 2 | `mark_complete()` failure → hard error metric | ✅ closed (logging part) | Stage 9: explicit match with tracing::error including account_id + request_id; "manual reconciliation required" message. Prometheus counter deferred to observability stage |
| 3 | Make managed local lane unrepresentable | ✅ closed | Stage 9: typed `ManagedLane { Instant, Balanced, Deep, Vision }` enum (no Local variant) — constructing `BlueyManagedProvider::new(client, ManagedLane::Local)` is a type error |
| 4 | Server-owned `/pricing/tiers` | ✅ closed | Stage 10: new public endpoint `/pricing/tiers`; cue-cli `bluey usage` fetches and renders, with hardcoded fallback if unreachable |
| 5 | Sanitized cue-cloud-client server bodies | ✅ closed | Stage 9: `Error::Server { status }` (body removed); body logged at warn inside `client::parse_or_err` and `auth::*` |
| 6 | Authoritative server-recorded usage events | ✅ closed (docstring) | Stage 10: `/usage/event` docstring marks client-supplied cost fields as untrusted; the router-side row written by `/router/complete` with the same request_id is canonical (UNIQUE wins on tie) |

## New runtime gaps closed beyond the nit list

| Gap | Status | Stage |
|---|---|---|
| **Auto top-up trigger** (the v0.2 dealbreaker) | ✅ live | Stage 10. Fire-and-forget tokio::spawn after deduct; Stripe payment_intents.create off-session against saved PaymentMethod; in-flight 60s dedupe + per-(account, hour) idempotency key |
| `Account.stripe_customer_id` + `stripe_payment_method_id` fields | ✅ added | Stage 10 (schema + struct + queries + create defaults) |
| Duplicate-email signup atomic 409 (S2.4 nit) | ✅ closed | Stage 10. Account::create maps SQLite UNIQUE → typed marker; signup handler returns 409 |
| Per-IP rate limiting on /auth/{login,signup,refresh,device/poll} + /router/complete | ✅ live | Stage 11. governor 0.6 keyed limiter; 429 + Retry-After |
| Daemon ProviderRegistry token-aware construction | ✅ live | Stage 9. Detects keyring token; constructs BlueyManagedProvider per cloud lane; legacy BYOK gated behind `BLUEY_DEV_BYOK=1`; Ollama always available for local fallback |
| Failover safety: managed billing never silently falls over to direct | ✅ enforced | Stage 9. Direct providers NOT registered when in managed mode unless `BLUEY_DEV_BYOK=1`. Stage 5's `LlmError::Billing` terminal variant is the runtime backstop |

## Test growth

| Crate | Stage 8 ended | Stage 11 ended | Delta |
|---|---|---|---|
| server | 44 | 51 | +7 (3 topup + 1 dup-email + 3 rate-limit) |
| cue-llm | 34 | 34 | net zero (request_id field added; existing tests still pass) |
| cue-router | 30 | 30 | unchanged |
| cue-cloud-client | 4 | 4 | unchanged |
| **Workspace cue** | 402 | 402 | unchanged |

Pipeline at `af6bf7c`: fmt + clippy -D + 51 server + 402 cue + 15 vitest + builds all green across cue + server.

## What's STILL pending for v0.2 GA

Grouped by priority. None of these are blockers for paid alpha (the money path is now closed and rate-limited), but most are real gaps before public launch.

### High priority (paid alpha → GA)

- **`/router/embed` real impl** — currently 501. OpenAI text-embedding-3-small is a small surface (~100 LOC). Daemon doesn't use cloud embeddings yet (local fallback works), so this gates only the cloud RAG feature.
- **`/router/transcribe` real impl** — currently 501. Deepgram. Bigger surface (multipart audio upload). Daemon uses local whisper.cpp today, so this gates cloud STT only.
- **Streaming proxy** through bluey-server for the deep lane. Current `BlueyManagedProvider::complete_stream` wraps `complete()` in a single-chunk stream. Real streaming requires SSE on bluey-server + a streaming client variant in cue-cloud-client.
- **Live balance display** in overlay top strip + per-card cost label. Daemon-side polling module + Tauri/IPC plumbing + SwiftUI/dashboard component changes. Multi-process integration; bigger than fits in this push.
- **Stripe live-mode rollover** — currently test keys. Operational; not code.

### Medium priority (security/compliance posture)

- **Email verification flow** — schema field exists (`email_verified_at`), no SMTP integration.
- **Password reset flow** — no `/auth/password-reset/{start,confirm}` endpoints.
- **Account deletion + GDPR data export** — required for EU customers.
- **Stripe Customer Portal session** — `/v1/billing_portal/sessions` for managing card / canceling auto top-up.
- **ToS / Privacy policy** linkage at signup screen.
- **`X-Forwarded-For` trust** behind Caddy/Cloudflare — current rate-limit honors any XFF; production deployment needs a small middleware to strip untrusted XFF before the limit reads it.

### Low priority (operational)

- **`/admin/metrics` Prometheus exporter** — for the mark_complete-failure counter and general health.
- **Wiremock test suite for Stripe** — `fetch_payment_method_from_stripe` (Stage 6 round-2) currently has no unit-test seam.
- **Tier-aware `/router/complete` rate limit** (paid users get higher limit than trial users).
- **Backup/replication** of bluey-server SQLite (operational runbook).
- **Onboarding web pages** on bluey.dev (signup, reload, account dashboard).
- **DigitalOcean droplet provisioning** + Caddy/Let's Encrypt (mirrors Pinky ops shape).

### Doc cleanup (cosmetic)

- 9 files in `docs/work/` (HANDOFF-*, PLAN-*, TEMPLATE-*) — functional but stale. Can be moved to `docs/rounds/` or deleted in a sweep round.

## Codex carry-forward nit at the v0.2 alpha gate

Per codex round-2 doc: "Before ProviderRegistry rewire ships, plumb a stable logical request id into BlueyManagedProvider and make managed local-lane construction impossible or explicitly rejected." **Both done in Stage 9.** The remaining nits codex mentioned ("server-owned pricing/tier numbers, sanitized cloud-client server bodies, authoritative server-recorded usage events") are also closed.

## Suggested codex re-review scope

`docs/reviews/STAGE-9-10-11-CODEX-REVIEW.md` should target:

1. **Stage 9 ProviderRegistry rewire**: managed-mode detection correctness, BYOK gate semantics, request_id threading correctness, mark_complete error visibility, ManagedLane enum API surface.
2. **Stage 10 auto top-up**: race conditions in the 60s in-flight Mutex (multi-process safety), Stripe Idempotency-Key reuse window, payment_method_id COALESCE persistence semantics, the trial path skip.
3. **Stage 10 duplicate-email atomic 409**: the anyhow-marker downstring-match in signup is fragile (codex may want a typed error variant); confirm acceptable.
4. **Stage 11 rate limiting**: in-process state map → multi-instance hazard; XFF trust gap pre-Caddy; whether the limits per tier are right for v0.2 alpha (5/min on signup is conservative).

## Remaining runtime invariants to verify end-to-end

Tests pass in isolation; production happy path is gated on:

- A real Stripe test-mode webhook flow (round-2 fix added the PI normalization but has no unit-test seam).
- A real `/router/complete` call with a full request lifecycle (idempotency reservation → upstream dispatch → deduct → trigger auto top-up → record usage event → mark complete).
- A real first-`bluey on` sign-in → hidden support usage/credits flow against
  the deployed server.

These are integration-test territory. The cleanest path to that confidence is a wiremock-backed test harness (Stripe + OpenAI + Anthropic mocks) that exercises the full money path; that's its own stage.

## Bottom line

The v0.2 alpha money path is now functionally complete: customer logs in → managed provider routes through bluey-server → server idempotently bills → balance drops → auto top-up fires → webhook credits → loop continues. Rate limiting protects the auth surface. All codex round-2 carry-forward nits closed except observability metrics (deferred to a Prometheus/exporter stage).

Codex re-review is the right next gate before any further feature work.
