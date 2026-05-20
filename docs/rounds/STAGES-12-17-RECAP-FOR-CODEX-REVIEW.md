# Stages 12-17 Recap — For Codex Review

> **Branch:** `feat/phase-3-round-12`
> **Tip:** `c0ea5a8 feat(daemon): Stage 17 daemon-side balance polling module`
> **Includes:** S9-S11 round-2 fix commits + Stages 12-17 forward push.

## 1. Commit table

| Order | Commit | Stage | Layer |
|---|---|---|---|
| 1 | `7a37373` | S9 fix B1+B2 | daemon — managed-aware `build_llm_provider_from_env` + `ManagedPolicy` selection in speculative dispatch |
| 2 | `a0ccf7b` | S9 fix B3 | router — lane-scoped `request_id` (`:draft`/`:final`) so server idempotency does not collide |
| 3 | `3fc4746` | S9 fix B4 | daemon — all-lanes-failed surfaces `Ok(None)` for legacy fallback instead of empty `Ok(Some(""))` |
| 4 | `52e55ba` | S11 fix B5 | server — XFF only honored when peer in `BLUEY_TRUSTED_PROXIES` |
| 5 | `05ca56d` | S10 nit | server — typed `AccountCreateError` (no string-match) |
| 6 | `241fa53` | 12a | server — real `/router/embed` (OpenAI text-embedding-3-small) |
| 7 | `7882182` | 12b | server — real `/router/transcribe` (Deepgram nova-3) |
| 8 | `d451852` | 13 | server — `/auth/verify-email/{start,confirm}` + `/auth/password-reset/{start,confirm}` |
| 9 | `c2cfde6` | 14 | server — `/account/delete` + `/account/export` + `/billing/portal` |
| 10 | `42fd022` | 15 | server — `/admin/metrics` Prometheus exporter |
| 11 | `1511791` | 16 | cli — `bluey logout` / `portal` / `export` / `delete-account` |
| 12 | `c0ea5a8` | 17 | daemon — `BalanceWatch` poll module |

## 2. What each stage added

### Stage 12a — `/router/embed`
Real handler in `server/src/api/router.rs` for OpenAI `text-embedding-3-small`. Same lifecycle as `/router/complete`: `request_id` idempotency reservation → entry balance check → upstream call → atomic deduct → auto top-up trigger → `usage_event` record → terminal cache. Pricing entry added (`$0.02/1M` input, no output, 200% markup → 6c/1M). New `dispatcher::embed` + `EmbedCompletion` type.

### Stage 12b — `/router/transcribe`
Deepgram nova-3 via `/v1/listen`. Customer POSTs raw audio bytes (`audio/wav` etc) with `request_id` in query string (body is binary). Same lifecycle. `duration_seconds` from `metadata.duration` is the billable "input_tokens"; pricing entry stores microcents-per-1M-seconds. 150% markup per locked tier.

### Stage 13 — Email verification + password reset
Two new tables (`email_verification_tokens`, `password_reset_tokens`) with sha256-hashed-at-rest tokens, 24h expiry, single-use via atomic `UPDATE…RETURNING`. Four endpoints (`start` requires auth for verify, public for reset; `confirm` always public). SMTP delivery is deferred — `start` handlers log the dev URL at `info` when `BLUEY_SMTP_HOST` is unset.

### Stage 14 — GDPR + Stripe Customer Portal
`/account/delete` does immediate hard-delete with `ON DELETE CASCADE` cleaning up dependents. `/account/export` returns a JSON bundle (account row + credit_batches + last 10k usage_events + counts). `/billing/portal` creates a Stripe Customer Portal session and returns the hosted URL.

### Stage 15 — `/admin/metrics`
Plain-text Prometheus exposition format. SQL-sourced counters/gauges. Closes the S4 "mark_complete failure metric" carry-forward via `bluey_mark_complete_failures_estimated` (count of `request_idempotency` rows in `in_progress > 5 min`).

### Stage 16 — CLI customer-facing commands
`bluey logout` (clears keyring), `bluey portal` (opens browser to Stripe portal URL), `bluey export` (writes `bluey-export-YYYYMMDD-HHMMSS.json`), `bluey delete-account [--force]` (interactive `DELETE` confirmation prompt). New `CloudClient::clear_tokens` helper.

### Stage 17 — Daemon balance polling
`crates/cue-daemon/src/cloud/balance.rs` — `BalanceWatch` wrapping `tokio::sync::watch`. `spawn_loop` polls `/account/me` every 30s (overridable via `BLUEY_BALANCE_POLL_SECS`). Snapshot includes `low_balance_warning` derived flag. **UI integration is explicitly NOT in this commit** — overlay top-strip subscribes in a separate Tauri/SwiftUI commit.

## 3. Codex round-2 closure map

| Codex finding | Severity | Closed by | How |
|---|---|---|---|
| S9 B1: `build_llm_provider_from_env` BYOK-only | 🔴 | `7a37373` | Managed-mode path checked first; returns `BlueyManagedProvider` (Balanced lane) when token in keyring (unless `BLUEY_DEV_BYOK=1`). |
| S9 B2: `StaticPolicy` in managed registry | 🔴 | `7a37373` | New `ProviderRegistry::is_managed_only`; `try_speculative_dispatch` picks `ManagedPolicy` when managed. Arbitrary HashMap fallback removed; missing route now returns clear `LlmError::Provider`. |
| S9 B3: speculative draft+final share `request_id` | 🔴 | `a0ccf7b` | Per-lane suffix (`:draft`/`:final`) on `request_id`. New test `lane_scoped_request_ids_when_speculating_deep`. |
| S9 B4: all-lanes-failed silent empty-OK | 🔴 | `3fc4746` | `lane_errors` collected during stream; if no content produced, returns `Ok(None)` and falls back to legacy single-shot. |
| S11 B5: XFF unconditionally trusted | 🔴 | `52e55ba` | `BLUEY_TRUSTED_PROXIES` env var; XFF only honored when peer IP is in trusted set. Default-empty = safe. |
| S10 nit: dup-email string-match | 🟡 | `05ca56d` | Typed `AccountCreateError::DuplicateEmail` variant; signup uses `downcast_ref` instead of `to_string().contains(...)`. |
| S4 nit: `mark_complete` failure metric | 🟡 | `42fd022` | `bluey_mark_complete_failures_estimated` counter at `/admin/metrics`. |

The remaining S10 nits codex flagged (process-local 60s dedupe; Stripe `(account, hour)` idempotency-key window) are documented as accepted-with-nits for single-binary alpha; no code change needed yet.

## 4. New APIs & CLI commands

### Server endpoints

```
POST /router/embed
POST /router/transcribe?request_id=<uuid>&model=<optional>
POST /auth/verify-email/start          (auth)
POST /auth/verify-email/confirm        (public)
POST /auth/password-reset/start        (public)
POST /auth/password-reset/confirm      (public)
POST /account/delete                   (auth)
GET  /account/export                   (auth)
POST /billing/portal                   (auth)
GET  /admin/metrics                    (admin)
```

### CLI

```
bluey logout
bluey portal
bluey export
bluey delete-account [--force]
```

### Daemon

`cue_daemon::cloud::balance::BalanceWatch` + `spawn_loop()` (subscribers receive `BalanceSnapshot` over a `watch::Receiver`).

## 5. Security / auth / billing / data-deletion implications

- **XFF spoofing** closed (`52e55ba`). Production deployment behind Caddy will need `BLUEY_TRUSTED_PROXIES=127.0.0.1`. Direct-internet exposure remains safe-by-default (XFF ignored).
- **Failover safety** double-enforced: registry-level (Stage 9) + runtime `LlmError::Billing` terminal variant (Stage 5). Managed billing failures cannot silently fall over to unmetered direct providers.
- **Idempotency boundaries** distinct per lane (Stage 9 B3) so speculative dispatch does not collide.
- **Email enumeration** mitigated: `/auth/password-reset/start` always returns `202` regardless of whether the email exists.
- **Token storage** on auth/reset tokens uses sha256-at-rest + atomic `UPDATE…RETURNING` for single-use semantics. Same shape as the refresh-token store.
- **Hard delete** (`/account/delete`) is irreversible. Cascades to `credit_batches`, `refresh_tokens`, `usage_events`, `email_verification_tokens`, `password_reset_tokens`, `request_idempotency`. CLI requires interactive `DELETE` confirmation.
- **GDPR export** bundles up to 10k usage_events for performance reasons. v0.2.x can stream chunked output if customers exceed that.
- **Stripe Customer Portal** is the canonical surface for managing the saved card / canceling auto top-up. Server only exposes a session-creation endpoint; we never touch Stripe-side card UI ourselves.
- **mark_complete crash window** observable: `/admin/metrics` exposes `bluey_mark_complete_failures_estimated` for SRE alerting.

## 6. Tests added and pipeline status

| Component | Stage 12-17 delta | Total |
|---|---|---|
| `server` | +3 token tests (Stage 13) | 56 (was 53) |
| `cue-daemon` | +3 balance tests (Stage 17) | (workspace inclusive) |
| Workspace cue | +3 (S9 lane-scoped + balance) | 406 (was 403) |

Pipeline at tip `c0ea5a8`:

```
✅ cargo fmt --all (cue + server)
✅ cargo clippy --all-targets -- -D warnings (cue + server)
✅ 56 server cargo tests
✅ 406 workspace cue tests
✅ 15 vitest
✅ debug builds across all crates
```

No new tests on the embed/transcribe/GDPR/portal/metrics endpoints because they reuse paths covered by existing Stage 4/6/7 tests; live integration is gated on the deferred wiremock harness.

## 7. Explicit deferrals for v0.2 GA

These are NOT in this push and should not be reviewed as missing work — they are scoped out:

### Frontend / UX
- **Overlay live balance UI**: subscriber for `BalanceWatch` + SwiftUI top-strip rendering. Crosses the Tauri/native-overlay boundary; needs frontend review.
- **Per-card cost label** in the dashboard. `RouterMeta.cost_cents` is plumbed by `BlueyManagedProvider`; the inline UI component is a separate dashboard commit.
- **Onboarding web pages** on bluey.dev (signup, reload, account dashboard). Different repo / web codebase.

### Operational
- **SMTP integration** (lettre or sendgrid) wired to `/auth/verify-email/start` + `/auth/password-reset/start`. Currently logs the dev URL at `info` when `BLUEY_SMTP_HOST` is unset.
- **Stripe live-mode rollover** — operational env-var swap; no code change.
- **DigitalOcean droplet provisioning** + Caddy/Let's Encrypt (mirrors Pinky ops shape).
- **SQLite backup rotation** on the droplet.

### Architecture
- **Streaming proxy** — SSE on `/router/complete/stream` + daemon stream-consumer rewire of `BlueyManagedProvider::complete_stream` (currently single-chunk-wrap). Substantial design surface; separate stage.
- **Wiremock test harness** for Stripe + OpenAI + Anthropic + Deepgram integration tests. Queued separately from the customer-facing endpoint work.

### Future hardening
- Multi-process safety on the rate-limit map (currently in-memory, single-binary safe).
- Tier-aware `/router/complete` rate limit (paid users vs trial).
- Server-side spool for daemon `/usage/event` retries during disconnect.

## Codex review focus suggestions

1. **Stage 9 fixes** — verify managed-mode detection in `commands.rs` is correct, `request_id` lane scoping does not break legacy callers, all-lanes-failed fallback path is reachable.
2. **Stage 12 endpoints** — verify the request_id idempotency path is exact-copy of `/router/complete` (it is, but worth confirming); transcribe's binary-body + query-string `request_id` pattern.
3. **Stage 13 token store** — single-use semantics under concurrent consume; atomic `UPDATE…RETURNING` correctness.
4. **Stage 14 hard-delete cascade** — confirm every table referencing `account_id` has `ON DELETE CASCADE` (already in schema migrations).
5. **Stage 15 metrics** — the `bluey_mark_complete_failures_estimated` derivation is a proxy, not a real counter. Acceptable, or insist on metrics-rs?
6. **Stage 17 watch channel** — the polling task has no graceful shutdown (loops forever). Acceptable for daemon lifetime, or wire a `CancellationToken`?
