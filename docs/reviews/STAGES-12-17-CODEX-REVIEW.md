# REVIEW: Stages 12-17 — Managed Server Follow-Through

**Commit range:** `7a37373^..84c6882`
**Reviewer:** Codex
**Date:** 2026-05-20

## Per-Task Review

### Stage 9 Fixes — Managed Registry / Speculative Fallback

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/src/commands.rs`, `crates/cue-router/src/speculative.rs`, `crates/cue-llm/src/bluey_managed.rs` |
| Verdict | 🔴 REQUEST CHANGES |

**Findings:**

- 🔴 `crates/cue-dashboard/src/commands.rs:1069` / `crates/cue-dashboard/src/commands.rs:1144` — the all-lanes-failed blocker is still present. The fix commit collects `lane_errors`, but never reads them and still returns `Ok(Some((final_text.unwrap_or(accumulated_draft).trim().to_string(), cost)))`. If every speculative lane emits only `SpeculativeChunk::Error`, `try_speculative_dispatch()` returns `Some("")`, so the caller persists/emits an empty answer instead of falling back to the legacy path. Add the advertised final check: if the resolved text is empty and lane errors exist, log the errors and return `Ok(None)` or `Err(...)`, plus a regression test where every lane fails.
- 🟢 Managed mode detection, `ManagedPolicy` selection, lane-scoped request ids, and managed-local being unrepresentable are otherwise materially improved.

---

### Stage 11 Fix — Trusted Proxy XFF Rate Limiting

| Field | Value |
|-------|-------|
| Files | `server/src/main.rs`, `server/src/rate_limit.rs` |
| Verdict | 🔴 REQUEST CHANGES |

**Findings:**

- 🔴 `server/src/main.rs:34` / `server/src/main.rs:44` and `server/src/rate_limit.rs:138` — the limiter depends on `ConnectInfo<SocketAddr>`, but the server passes the `Router` directly to `axum::serve(listener, app)`. That does not install `ConnectInfo`, so `client_key()` falls through to `"unknown"` for every request. Result: all clients share one global bucket, and `BLUEY_TRUSTED_PROXIES`/XFF handling is never active in the real server. Serve with `app.into_make_service_with_connect_info::<SocketAddr>()` and add a production-path test or smoke that proves a real request carries peer address metadata.
- 🟢 The trusted-proxy parsing itself is safer than the previous unconditional XFF trust model.

---

### Stage 12 — `/router/embed` and `/router/transcribe`

| Field | Value |
|-------|-------|
| Files | `server/src/api/router.rs`, `server/src/routing/dispatcher.rs`, `server/src/pricing/mod.rs` |
| Verdict | 🔴 REQUEST CHANGES |

**Findings:**

- 🔴 `server/src/pricing/mod.rs:73` / `server/src/pricing/mod.rs:82` — the new embedding and Deepgram prices are off by 10x under the module's own microcent definition. `$0.02/1M` is `2 cents = 20_000 microcents`, not `200_000`. `$0.0043/min` is `0.43 cents = 4_300 microcents`, not `43_000`, so the per-1M-second value should be about `71_666_667`, not `716_666_667`. As written, Stage 12 overestimates cost, entry checks, usage, and customer charges for embed/STT by an order of magnitude. Fix the constants and add unit tests that encode the dollars-to-microcents conversions.
- 🟡 `server/src/api/router.rs:441` and `server/src/api/router.rs:689` — Stage 12 endpoints have no dedicated server tests for idempotency replay, upstream-error release, and billing math. They are close clones of `/router/complete`, but the binary-body transcribe path and vector response cache are new enough to deserve at least deterministic unit/wiremock coverage after the pricing constants are corrected.

---

### Stage 13 — Email Verification / Password Reset

| Field | Value |
|-------|-------|
| Files | `server/src/api/auth_routes.rs`, `server/src/db/auth_tokens.rs`, `server/src/mail.rs` |
| Verdict | 🟡 ACCEPT WITH NITS |

**Findings:**

- 🟡 `server/src/api/auth_routes.rs:505` / `server/src/api/auth_routes.rs:524` — password reset confirmation consumes the single-use token before validating/hash-building the new password. A short or too-long password returns `400` after the token is already burned, forcing the customer to request another reset. Validate/hash first, then consume+update in one transaction, or provide a consume-after-validation helper. The same pattern is worth tightening for email verification because the account update result is currently ignored after consuming the token.
- 🟢 Tokens are hashed at rest, expiry is enforced, and `UPDATE ... RETURNING` gives the intended single-use consume semantics.

---

### Stage 14 — GDPR Delete / Export / Stripe Portal

| Field | Value |
|-------|-------|
| Files | `server/src/api/account.rs`, `server/src/db/mod.rs`, `server/src/api/billing.rs` |
| Verdict | 🔴 REQUEST CHANGES |

**Findings:**

- 🔴 `server/src/api/account.rs:265` / `server/src/api/account.rs:297` and `server/src/db/mod.rs:127` — `/account/delete` does not remove `stripe_webhook_events`, even though export already proves those rows are account-linked via `body.data.object.client_reference_id`. The webhook table has no `account_id` FK, stores raw JSON, and will retain Stripe/customer/account metadata after the endpoint returns `"All account data has been removed"`. For GDPR hard-delete, add an account linkage column with cascade or explicitly delete/scrub matching webhook rows by all account identifiers used in Stripe metadata/client_reference_id before returning success. Also add a test that creates a webhook body for the account and verifies it is gone or anonymized after deletion.
- 🟡 `server/src/api/account.rs:205` — export only returns a count of matched Stripe webhook events, not the associated raw/audit data. If those rows are kept for financial audit, document the legal retention policy and include/summarize them explicitly in export.

---

### Stage 15 — `/admin/metrics`

| Field | Value |
|-------|-------|
| Files | `server/src/api/metrics.rs`, `server/src/api/mod.rs` |
| Verdict | 🟡 ACCEPT WITH NITS |

**Findings:**

- 🟡 `server/src/api/metrics.rs:63` — `bluey_mark_complete_failures_estimated` is a useful proxy for alpha, but it is not a true counter and can include legitimate long-running requests. Acceptable if documented as an estimate; for GA, prefer a real counter when `mark_complete()` fails.
- 🟢 `/admin/metrics` is mounted under the admin-only router and then protected by the authenticated router layer.

---

### Stage 16 — CLI Account Commands

| Field | Value |
|-------|-------|
| Files | `crates/cue-cli/src/bluey_cmds.rs`, `crates/cue-cloud-client/src/client.rs` |
| Verdict | 🟢 ACCEPT |

**Findings:**

- No blocking findings. `logout`, `portal`, `export`, and `delete-account` use the keyring-backed cloud client and expose sensible terminal UX. The per-batch credit listing is clearly messaged as future work.

---

### Stage 17 — Daemon Balance Watch

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/cloud/balance.rs`, `crates/cue-dashboard/src/commands.rs` |
| Verdict | 🟡 ACCEPT WITH NITS |

**Findings:**

- 🟡 `crates/cue-daemon/src/cloud/balance.rs:72` — the polling loop has no cancellation path. That is fine for daemon-lifetime ownership, but once it is wired into restartable dashboard/runtime lifecycles it should take a cancellation token or be owned by a tracked task handle.
- 🟢 The `watch` channel shape is appropriate for low-frequency balance snapshots.

---

## Cross-Task Findings

- 🔴 Scope mismatch: the recap says the review range is 12 commits ending at `c0ea5a8`, but local `84c6882` actually includes five additional commits after Stage 17: balance UI/SMTP, managed streaming/cost labels, two overlay UX commits, and the recap doc. I focused this verdict on the Stage 9-17 server/product scope requested here; those extra commits should keep their separate Kiro review surface.
- 🔴 The two strongest blockers are money/data correctness: Stage 12 overcharges embed/STT by 10x, and Stage 14 says GDPR data is removed while retaining raw Stripe webhook JSON tied to the account.
- 🔴 The Stage 9 and Stage 11 fix-wave claims are not fully true in production paths: all-lanes-failed still returns empty success, and the real server never populates `ConnectInfo`.

## Build & Test Verification

```bash
# Static review of 7a37373^..84c6882.
# I did not rerun the full pipeline because the review found merge-blocking
# production-path defects that existing tests do not cover.
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Do not merge/tag this batch as production-ready yet. Fix the all-lanes-failed fallback, real-server `ConnectInfo` wiring, Stage 12 pricing constants, and GDPR webhook cleanup/export policy first.

## Follow-ups for Next Batch

- Add regression tests for all-lanes-failed speculative dispatch returning fallback instead of empty success.
- Add a server smoke/integration test proving rate-limit keys use peer IP in the real `axum::serve` setup.
- Add unit tests for every pricing constant that convert published dollar prices into microcents.
- Add account-delete/export tests covering every table or audit store that can contain account identifiers, especially Stripe webhook bodies.
- Decide retention policy for Stripe webhook audit rows: delete, scrub/anonymize, or retain under documented legal/accounting basis and disclose in export.
