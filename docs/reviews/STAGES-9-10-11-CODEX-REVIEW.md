# REVIEW: Server Stages 9-11 — Managed Registry, Auto Top-Up, Rate Limiting

**Commit range:** `fbd43e1..57b6564`
**Reviewer:** Codex
**Date:** 2026-05-19

## Per-Task Review

### Stage 9 — Daemon ProviderRegistry Rewire + Carry-Forward Nits

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/src/commands.rs`, `crates/cue-router/src/speculative.rs`, `crates/cue-llm/src/bluey_managed.rs`, `server/src/api/router.rs` |
| Verdict | 🔴 REQUEST CHANGES |

**Findings:**

- 🔴 `crates/cue-dashboard/src/commands.rs:1098` — managed-only customers still cannot use `request_cue`. The command eagerly calls `build_llm_provider_from_env(&db).ok_or_else(|| "no LLM provider configured")?` before the managed `ProviderRegistry` speculative path runs. `build_llm_provider_from_env()` only builds an OpenAI BYOK provider (`crates/cue-dashboard/src/commands.rs:1457`), so a logged-in Bluey customer with keyring tokens but no direct `OPENAI_API_KEY` gets rejected before `BlueyManagedProvider` is considered. Move the legacy provider build below the speculative/managed path, or replace it with a unified provider selection path that understands managed mode.
- 🔴 `crates/cue-dashboard/src/commands.rs:983` / `crates/cue-dashboard/src/commands.rs:1448` — managed routing is not lane-correct. `try_speculative_dispatch()` always constructs `StaticPolicy::defaults()`, whose routes target `openai` / `anthropic`; in managed mode the registry contains only `bluey-managed-instant`, `bluey-managed-balanced`, `bluey-managed-deep`, and `bluey-managed-vision`. The current `provider_for()` fallback returns `self.providers.values().next()`, which is HashMap-order dependent, so a Deep or Vision classification can silently dispatch to an arbitrary managed lane. Use `ManagedPolicy` when managed mode is active, or translate static routes to the matching managed provider names deterministically.
- 🔴 `crates/cue-router/src/speculative.rs:207` — speculative draft and final lanes reuse the same `request_id`. The server idempotency table keys on `(account_id, request_id)` and returns `InProgress` or `CachedComplete` for the second request (`server/src/db/idempotency.rs:33`). With default-on speculation, the instant draft and deep final are two distinct provider calls, but the second lane will collide with the first idempotency reservation/cache instead of executing as an independent final answer. Fix with lane-scoped idempotency keys, for example `<logical_response_id>:draft` and `<logical_response_id>:final`, while keeping the parent logical id in metadata/usage if needed.
- 🔴 `crates/cue-dashboard/src/commands.rs:1049` / `crates/cue-dashboard/src/commands.rs:1056` — all speculative lane failures are converted into a successful empty response. `SpeculativeChunk::Error` is only logged, and after the stream closes the function returns `Ok(Some(final_text.unwrap_or(accumulated_draft).trim().to_string()))`. If every lane failed, this persists and emits an empty `cue_response` instead of falling back or returning an actionable error. Track whether any draft/final content was produced; if not, return `Ok(None)` for legacy fallback or `Err(...)` for the UI.

---

### Stage 10 — Auto Top-Up + Pricing Tiers + Duplicate-Email 409

| Field | Value |
|-------|-------|
| Files | `server/src/billing/topup.rs`, `server/src/api/router.rs`, `server/src/api/pricing.rs`, `server/src/api/auth_routes.rs`, `server/src/db/accounts.rs`, `crates/cue-cloud-client/src/client.rs` |
| Verdict | 🟡 ACCEPT WITH NITS |

**Findings:**

- 🟡 `server/src/billing/topup.rs:29` — the 60-second auto-top-up dedupe is process-local. That is acceptable for the stated single-binary alpha, but it must move to a durable DB/Stripe-state guard before multi-process or multi-host deployment, otherwise concurrent instances can initiate duplicate off-session charges.
- 🟡 `server/src/billing/topup.rs:122` — the Stripe idempotency key is bucketed per `(account, hour)`. This is good enough to suppress bursts, but it also means parameter changes inside an hour can replay an earlier Stripe result. Consider keying on a durable top-up attempt id once attempts are stored server-side.
- 🟡 `server/src/api/auth_routes.rs:136` — duplicate-email mapping relies on `e.to_string().contains("duplicate-email")`. The race is handled, but this should become a typed DB error variant so auth behavior does not depend on an error-message marker.

---

### Stage 11 — Per-IP Rate Limiting

| Field | Value |
|-------|-------|
| Files | `server/src/rate_limit.rs`, `server/src/api/mod.rs` |
| Verdict | 🔴 REQUEST CHANGES |

**Findings:**

- 🔴 `server/src/rate_limit.rs:106` — `client_key()` trusts `X-Forwarded-For` unconditionally. When the server is reachable directly, any client can spoof a new XFF value per request and bypass `/auth/login`, `/auth/signup`, `/auth/refresh`, `/auth/device/poll`, and `/router/complete` limits. Only honor XFF behind an explicitly trusted proxy configuration that strips/replaces inbound headers, or ignore XFF and use `ConnectInfo` until that deployment contract is enforced and tested.
- 🟡 `server/src/rate_limit.rs:29` — rate limiter state is in-memory. This is fine for a single-process alpha, but it is not multi-instance safe and should be documented as such in deployment guidance if Stage 11 lands before Redis/DB-backed limiting.

## Cross-Task Findings

- The Stage 9 managed path is the merge blocker. It currently combines a BYOK-only preflight, static BYOK policy, arbitrary managed provider fallback, and shared speculative idempotency keys. Together those issues mean the v0.2 money path can fail before dispatch, route to the wrong managed lane, or collapse draft/final into one server idempotency row.
- Stage 10 is close enough for alpha after the Stage 9 blockers are fixed, provided the single-process assumptions remain explicit.
- Stage 11 adds useful protection, but the unconditional XFF trust gap defeats the limiter on any direct internet exposure.

## Build & Test Verification

```bash
# Static review of fbd43e1..57b6564.
# I did not run the full pipeline because the review found merge-blocking
# request-path defects before verification would change the verdict.
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Do not merge Stages 9-11 yet. Fix the managed request path, lane-correct policy selection, speculative idempotency semantics, all-lanes-failed behavior, and the XFF trust model first.

## Follow-ups for Next Batch

- Add production-path tests for logged-in managed mode with no BYOK keys: `request_cue` should dispatch to `BlueyManagedProvider` instead of returning "no LLM provider configured".
- Add a managed routing test proving Instant/Balanced/Deep/Vision classifications resolve to the matching `bluey-managed-*` providers, with no HashMap-order fallback.
- Add a speculative integration test with two managed lanes proving draft and final use distinct server request ids while remaining tied to one UI response id.
- Add a rate-limit test showing spoofed XFF is ignored unless trusted-proxy mode is explicitly enabled.
