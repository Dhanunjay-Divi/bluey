# REVIEW: Server Stage 1 — Rust Skeleton

**Commit:** `853be40 feat(server): bluey-server Rust skeleton (R14.9 stage 1)`
**Reviewer:** Codex
**Date:** 2026-05-19

## Per-Task Review

### S1.1 — Crate / Repo Placement

| Field | Value |
|-------|-------|
| Files | `server/Cargo.toml`, `server/src/lib.rs`, `server/src/main.rs` |
| Verdict | 🟢 accept |

**Findings:**
- `server/` as a temporary isolated crate is fine. The `[workspace]` marker keeps it out of the main Cue workspace, and splitting to a separate `bluey-server` repo later is mostly CI/deploy/doc work rather than code architecture.

---

### S1.2 — Route Skeleton / Security Shape

| Field | Value |
|-------|-------|
| Files | `server/src/api/mod.rs`, `server/src/api/admin.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 `GET /admin/customers` is implemented and mounted on the public router with no auth (`server/src/api/mod.rs:51-52`, `server/src/api/admin.rs:33-67`). Once Stage 2 creates accounts, anyone who can reach the server can list customer IDs, emails, and balances. For Stage 1, make this route return 501 like the other protected stubs, or move it behind an auth/admin middleware before real accounts land.

---

### S1.3 — Database Schema

| Field | Value |
|-------|-------|
| Files | `server/src/db/mod.rs` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟢 The table set is a reasonable v0.2 starting point: accounts, refresh tokens, device codes, credit batches, usage events, and Stripe webhook idempotency cover the planned flow.
- 🟡 Before the usage dashboard lands, add an index that supports aggregation by account + task/lane over a time window, or be explicit that `idx_usage_events_account_ts` is enough for first pass and aggregation happens after filtering.
- 🟡 Inline migrations are acceptable at this stage, but once real customer data exists, switch to numbered migration files or a migrator with applied-state tracking. `CREATE TABLE IF NOT EXISTS` cannot safely express column/type/index evolution.

---

### S1.4 — Balance / Credit Accounting

| Field | Value |
|-------|-------|
| Files | `server/src/db/balance.rs` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟢 The account-level deduction query is the right race-free primitive: `UPDATE accounts ... WHERE balance_cents >= ?` serializes concurrent deductions at the SQLite write lock and only one set of updates can spend the same balance.
- 🟡 `deduct()` does not decrement `credit_batches.remaining_cents` (`server/src/db/balance.rs:25-39`). The handoff calls this out, and I agree with the direction: do the FIFO batch consumption in the same transaction as the account deduction before any wallet endpoint consumes this function.
- 🟡 `credit()` and `sweep_expired()` each perform multi-statement balance changes without an explicit transaction (`server/src/db/balance.rs:53-67`, `server/src/db/balance.rs:106-118`). Wrap them in transactions before billing is live so a partial failure cannot desynchronize account balance and batch state.

---

### S1.5 — Pricing

| Field | Value |
|-------|-------|
| Files | `server/src/pricing/mod.rs` |
| Verdict | 🔴 blocker before wallet/router work |

**Findings:**
- 🔴 The known microcent scale bug is real and should be fixed before any `/router/complete`, wallet, estimate, or usage-label implementation is built on top. The constants are off by 100x (`server/src/pricing/mod.rs:24-58`), and the tests deliberately avoid asserting the correct values (`server/src/pricing/mod.rs:133-164`). It is okay that Stage 1 skeleton compiles, but do not defer this past the next server stage that touches spend or pricing.

## Build & Test Verification

```bash
cd /tmp/bluey-stage1-review/server && cargo test                         # ✅ 8 passed
cd /tmp/bluey-stage1-review/server && cargo clippy --all-targets -- -D warnings  # ✅
```

## Overall Verdict

🔴 **REQUEST CHANGES** — The public admin customer route must be removed/protected before this skeleton becomes the base for auth/account work. Pricing and FIFO issues can be folded into the wallet/router stage, but the admin leak should be fixed immediately.

## Follow-ups for Next Batch

- Move `/admin/customers` behind real admin auth or return 501 until middleware exists.
- Fix pricing unit constants and add exact-value tests before implementing metered router calls.
- Make balance mutations transactional when FIFO credit consumption lands.
