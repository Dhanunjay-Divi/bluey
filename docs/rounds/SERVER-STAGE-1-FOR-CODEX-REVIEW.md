# Server Stage 1 — Codex Review Asks

> **Commit:** `853be40 feat(server): bluey-server Rust skeleton (R14.9 stage 1)`
> **Branch:** `feat/phase-3-round-12`
> **Files:** all under `server/`, plus `.gitignore`.

This is the foundational scaffold for the Layer 3 product server.
Skeleton-only — most handlers return `501 NOT_IMPLEMENTED`.

## What I verified locally

- `cargo build` clean (18.6s clean rebuild).
- `cargo test` passes (4 balance tests + 3 pricing tests, heap-isolated tmp DBs).
- Binary runs: `BLUEY_JWT_SECRET=<32+chars> ./bluey-server` starts, applies
  6 migrations, responds to `GET /admin/health` with
  `{"status":"ok","version":"0.1.0","commit":"unknown"}`, exits cleanly
  on SIGTERM.
- The cue workspace pipeline still passes (392 cargo + 15 vitest).

## What I want you to review

### 1. Crate/repo placement

Server lives at `server/` inside this repo with `[workspace]` marker
to keep it isolated from the cue workspace. Plan: extract to a
separate `bluey-server` repo once the API stabilises (~Stage 4).

**Ask:** is `server/` placement OK as a temporary measure, or should
this be a separate repo from day one? Cost of moving later: rewrite
the deploy/build/CI bits + rewrite docs cross-references; otherwise
clean.

### 2. DB schema (`server/src/db/mod.rs::MIGRATIONS`)

Six migrations land in one shot at startup. All idempotent
(`CREATE TABLE IF NOT EXISTS`).

Tables:
- `accounts` — id (uuid), email (unique), password_hash, balance_cents, trial_seconds_remaining, auto_topup_*, stripe_*
- `credit_batches` — per-reload tracking for FIFO 1-year expiry
- `refresh_tokens` — sha256-hashed refresh tokens, device_label, expires_at, revoked_at
- `device_codes` — OAuth-style device flow opened by first `bluey on`
- `usage_events` — per-request metering (kind, task_type, lane, provider, model, tokens, latency, cost_to_bluey, cost_to_customer, was_speculative, was_fallback)
- `stripe_webhook_events` — webhook idempotency

**Ask:**
- Schema completeness for v0.2: anything missing for the customer
  flow described in `docs/HOW-IT-WORKS.md`?
- Indexes: I added `idx_accounts_email`, `idx_credit_batches_account`,
  `idx_refresh_tokens_account`, `idx_device_codes_user_code`,
  `idx_usage_events_account_ts`, `idx_usage_events_request`. Anything
  missing for the queries we'll actually run (per-account rolling
  aggregation, login lookup, refresh validation, etc.)?
- Should the inline migration approach be replaced with a real
  migration tool (sqlx-migrate, refinery)? Inline is simpler now
  but harder to reason about for `down` paths and one-off data fixes.

### 3. Atomic balance deduction (`server/src/db/balance.rs::deduct`)

```sql
UPDATE accounts SET balance_cents = balance_cents - ?1
 WHERE id = ?2 AND balance_cents >= ?1
```

Returns `Ok(true)` if the UPDATE affected a row, `Ok(false)` if not
(insufficient balance OR concurrent deduction drained it).

**Ask:**
- Is this race-free against concurrent requests for the same
  account? My read: SQLite WAL with default isolation gives us
  serializable behaviour on UPDATE, and the WHERE-balance-check
  makes the deduction atomic. Confirm.
- Tests cover deduct success, deduct failure, credit extends 365d
  expiry, sweep no-op. Anything obvious missing?

### 4. FIFO credit-batch accounting (`server/src/db/balance.rs::credit`, `sweep_expired`)

Each `credit()` call inserts a row into `credit_batches` with
`expires_at = now + 365d` and increments `accounts.balance_cents`.
`sweep_expired()` (intended to run daily as a cron) finds rows
with `expires_at < now AND remaining_cents > 0 AND expired_at IS NULL`,
debits the account by `remaining_cents`, marks the row expired.

**Ask:**
- The model is "balance is a denormalised sum of remaining_cents
  across all live batches; sweep keeps them in sync." That's
  simple but has a known gap: when a customer SPENDS, we deduct
  from `accounts.balance_cents` but we DON'T decrement the oldest
  `credit_batches.remaining_cents`. So sweep will over-debit on
  expiry. **Need:** a FIFO consumption update — every successful
  `deduct()` should also decrement remaining_cents from oldest
  unexpired batch first. Will fix in Stage 4 when wallet logic
  becomes real; flagging here to confirm the design intent.
- Should sweep run as a tokio interval inside the server, or as a
  separate cron? I prefer separate cron (simpler ops, restart
  resilient), Pinky's pattern.

### 5. Pricing math known bug (`server/src/pricing/mod.rs`)

The `compute_cost` function has a microcent unit-conversion bug
flagged in the test (`cost_computation_medium_code`). The constant
table uses `upstream_in_microcents_per_1m: 30_000` for claude-3-5
($3/1M tokens), which gives ~24 microcents for 800 input tokens
when the actual upstream cost is ~240 microcents (~0.24¢).

The fix: change unit semantics so `upstream_in_microcents_per_1m`
holds microcents directly (e.g. `3_000_000` for $3/1M tokens), OR
rename and adjust to be cents-per-million-tokens.

**Ask:** flagged for Stage 4 (when wallet logic actually consumes
this). Confirm you'd rather see it fixed in Stage 4 alongside the
real `/router/complete` impl rather than as a standalone fix
commit.

### 6. Auth surface (`server/src/api/auth.rs`)

Stub-only. Routes wired:
- `POST /auth/{signup,login,refresh}` — standard email/password.
- `POST /auth/device/{start,poll,approve}` — OAuth-style device
  flow opened by first `bluey on` from the daemon.

**Ask:**
- Device flow is the one I'm less sure about. Targeting the same
  shape as Pinky's implementation. Is the 3-endpoint split the
  right granularity, or should approve+poll fold into one
  `/auth/device/exchange` endpoint?
- Refresh tokens stored as sha256 hash in DB, raw token in JWT
  body, signed with HS256. Acceptable for v0.2, or do you want
  ed25519 + JWK rotation now?

### 7. JWT algorithm + claims (`server/src/auth/mod.rs`)

```rust
pub struct Claims {
    pub sub: String,      // account_id
    pub iat: i64,
    pub exp: i64,
    pub kind: String,     // "access" | "refresh"
}
```

Stub. Real impl in Stage 2.

**Ask:** access token TTL — 15 minutes? 1 hour? 24 hours?
Refresh token TTL — 30 days? 90 days? Want your call before
Stage 2 lands.

### 8. Cargo dependencies (`server/Cargo.toml`)

```
axum 0.7, tokio "full", tower-http (trace+cors+gzip)
rusqlite + r2d2 + r2d2_sqlite (bundled SQLite)
jsonwebtoken 9, bcrypt 0.15
reqwest (rustls-tls, stream)
clap (env)
chrono (serde)
async-stream, futures-util, bytes (for streaming proxy in Stage 4)
```

**Ask:** anything missing? Anything you'd swap?
- Considered `axum-extra` for typed-headers but axum 0.7 inline
  extractors look sufficient.
- `rustls-tls` over `native-tls` matches Bluey daemon convention.
- No telemetry/metrics dep yet (R14.6 deferred).

## What's NOT in this stage

- No real auth (Stage 2).
- No real `/router/complete` proxying (Stage 4).
- No Stripe (Stage 8).
- No CI workflow for the server (separate concern; mirror Pinky).
- No deploy artifacts (Dockerfile, systemd unit) — Stage 9.
- No client-side wiring in Bluey daemon (Stage 5+).

## Suggested verdict shapes

- 🟢 **ACCEPT:** schema + skeleton looks right; proceed to Stage 2.
- 🟡 **ACCEPT WITH NITS:** flag specific items (e.g. fix the
  pricing microcent bug now, choose JWT TTLs) to address before
  Stage 2 lands.
- 🔴 **REQUEST CHANGES:** structural issues with the layout, schema,
  or deduction logic; fix before more code lands on top.

I'll wait for your verdict before committing Stage 2 if it's 🔴.
For 🟢/🟡 I'll continue with Stage 2 (real auth) and address nits
inline.
