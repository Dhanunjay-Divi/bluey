# REVIEW: Codex Fix Wave 2 — Server Stages 2 + 4-8

**Commit range:** `20cd0b3..e599b22`
**Reviewer:** Codex
**Date:** 2026-05-19

## Per-Task Review

### Stage 2 — Admin Role Gating

| Field | Value |
|-------|-------|
| Files | `server/src/auth/middleware.rs`, `server/src/api/mod.rs` |
| Verdict | 🟢 ACCEPT |

**Findings:**

- No blocking findings. `/admin/customers` is now split into an `admin_only` router and protected by `require_admin` after normal auth, with standalone middleware tests for unauthenticated, non-admin, and admin paths.

---

### Stage 4 — Managed `/router/complete`

| Field | Value |
|-------|-------|
| Files | `server/src/api/router.rs`, `server/src/db/idempotency.rs`, `server/src/routing/dispatcher.rs`, `server/src/pricing/mod.rs` |
| Verdict | 🟡 ACCEPT WITH NITS |

**Findings:**

- 🟡 `crates/cue-llm/src/bluey_managed.rs:52` — the server now has request-id idempotency, but the managed client still mints a fresh UUID inside `BlueyManagedProvider::complete()` for every provider call. That protects token-refresh retries inside one call, but not logical request retries above the provider boundary. Before the daemon ProviderRegistry rewire is treated as production, thread a stable request id from the answer/session request into `CompleteRequest`.
- 🟡 `server/src/api/router.rs:310` — `idempotency::mark_complete()` failure is ignored after upstream work and billing have already happened. A transient DB failure here leaves the request stuck as `in_progress`, so a retry returns 409 instead of the paid/cached response. At minimum emit a hard error metric/log with request id; preferably make terminal cache persistence part of the charged-response contract.

---

### Stage 5 — Managed Provider / Policy

| Field | Value |
|-------|-------|
| Files | `crates/cue-router/src/policy.rs`, `crates/cue-llm/src/bluey_managed.rs`, `crates/cue-llm/src/router.rs` |
| Verdict | 🟡 ACCEPT WITH NITS |

**Findings:**

- 🟡 `crates/cue-llm/src/bluey_managed.rs:24` — `ManagedPolicy` no longer emits a local lane, which clears the shipped routing path. The provider constructor still accepts `"local"` and advertises `bluey-managed-local`. This is okay while construction is internal, but the ProviderRegistry rewire should either make invalid lanes unrepresentable or reject `"local"` explicitly.

---

### Stage 6 — Stripe Checkout / Webhook

| Field | Value |
|-------|-------|
| Files | `server/src/api/billing.rs`, `server/src/db/balance.rs`, `server/src/db/mod.rs` |
| Verdict | 🔴 REQUEST CHANGES |

**Findings:**

- 🔴 `server/src/api/billing.rs:236` / `server/src/api/billing.rs:257` — the webhook fix handles `payment_intent` as a string for credit idempotency, but as an expanded object for `payment_method` capture. In the normal string-shaped Checkout webhook, credit dedupe gets a charge id but `stripe_payment_method_id` is never saved. In an expanded-object webhook, `payment_method` can be read, but `payment_intent_id` becomes `None`, weakening duplicate-credit protection. Normalize both shapes: extract the PaymentIntent id from either string or object, use that id for `balance::credit`, and fetch or expand the PaymentIntent when needed to persist the saved payment method.

---

### Stage 7 — Usage Window / Event Ingest

| Field | Value |
|-------|-------|
| Files | `server/src/api/usage.rs`, `server/src/api/account.rs`, `server/src/db/usage.rs` |
| Verdict | 🟡 ACCEPT WITH NITS |

**Findings:**

- No new blockers. The idempotent `(account_id, request_id, kind)` ingest and datetime comparison fix address the red findings. The deferred product nits remain valid: server-recorded router events should become authoritative for cost/tier analytics, and client-submitted usage should be treated as untrusted telemetry.

---

### Stage 8 — CLI Usage / Credits

| Field | Value |
|-------|-------|
| Files | `crates/cue-cli/src/app.rs`, `crates/cue-cli/src/bluey_cmds.rs`, `crates/cue-cloud-client/src/client.rs` |
| Verdict | 🔴 REQUEST CHANGES |

**Findings:**

- 🔴 `crates/cue-cli/src/app.rs:782` — the `bluey login` bridge only writes keyring tokens when both `access_token` and `refresh_token` are present. `handle_login_callback()` accepts an optional refresh token, and token/env-token login can also be access-only, so those successful login paths still leave `bluey usage` / `bluey credits` without keyring credentials. Save keyring tokens whenever an access token exists, using an empty refresh token if none is available; `cue-cloud-client` already loads refresh as optional/defaultable.
- 🟡 `crates/cue-cli/src/bluey_cmds.rs:71` — `bluey usage` still tells users to run `bluey credits` for batch-by-batch dates, while `bluey credits` says per-batch listing is future work. Soften this line to match the new command help.

## Cross-Task Findings

- 🟡 `docs/rounds/CODEX-FIX-WAVE-2-FOR-CODEX-REVIEW.md:4` — the re-review doc tip is stale (`0a3829e`) while the actual branch tip is `e599b22`. Minor, but worth correcting so future agents do not review the wrong point.
- The server-side money path is much closer, but there are still two customer-facing correctness gaps before v0.2 alpha: reliable Stripe PaymentIntent/payment-method normalization, and a login bridge that populates the token store for access-token-only flows.

## Build & Test Verification

```bash
cd server && cargo test
# ✅ 44 passed

cargo test -p cue-llm billing_error
# ✅ 2 passed

cargo test -p cue-router managed_never_emits_managed_local_provider
# ✅ 1 passed
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Most prior blockers are fixed, but Stage 6 payment-method/idempotency handling and Stage 8 access-only login bridging still block merging this fix wave as production-ready.

## Follow-ups for Next Batch

- Fix `handle_checkout_completed()` to normalize `payment_intent` string/object shapes, preserve idempotent crediting, and actually persist `stripe_payment_method_id`.
- Save keyring tokens on any successful `bluey login` with an access token, even when no refresh token is present.
- Before ProviderRegistry rewire ships, plumb a stable logical request id into `BlueyManagedProvider` and make managed local-lane construction impossible or explicitly rejected.
- Keep the existing deferred nits queued: server-owned pricing/tier numbers, sanitized cloud-client server bodies, and authoritative server-recorded usage events.

---

## Recheck 1 — Commit `3d514f2`

**Date:** 2026-05-19

### Updated Findings

- 🟢 `server/src/api/billing.rs:218` / `server/src/api/billing.rs:257` — Stage 6 blocker is cleared. `extract_payment_intent_id()` now handles both string and expanded-object `payment_intent` shapes, and `handle_checkout_completed()` always passes the normalized PaymentIntent id into `balance::credit()`. If the session did not include an expanded payment method, `fetch_payment_method_from_stripe()` retrieves `/v1/payment_intents/{id}` and persists the returned `payment_method` when available. The missing wiremock seam is acceptable as a follow-up, not a blocker.
- 🔴 `crates/cue-cli/src/app.rs:782` — Stage 8 blocker is **not** cleared on this tip. Commit `3d514f2` says it changed `cue_login`, but `git show --name-only 3d514f2` does not include `crates/cue-cli/src/app.rs`, and the code still uses `if let (Some(access), Some(refresh)) = (...)`. Access-only logins still do not populate the keyring used by `bluey usage` and `bluey credits`.
- 🟢 `crates/cue-cli/src/bluey_cmds.rs:71` — The copy nit is cleared. `bluey usage` no longer tells users to run `bluey credits` for unavailable batch-by-batch dates.

### Recheck Verification

```bash
cd server && cargo test
# ✅ 44 passed

cargo test -p cue-cloud-client
# ✅ 4 passed

cargo test -p cue-llm billing_error
# ✅ 2 passed

cargo test -p cue-router managed_never_emits_managed_local_provider
# ✅ 1 passed
```

### Recheck Verdict

🔴 **REQUEST CHANGES** — Stage 6 is now acceptable, but Stage 8 remains blocked because the access-only login bridge was described in the commit message but not actually applied to `crates/cue-cli/src/app.rs`.
