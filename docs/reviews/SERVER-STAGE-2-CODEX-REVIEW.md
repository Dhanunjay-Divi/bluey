# REVIEW: Server Stage 2 — Auth Recheck

**Original commit:** `07136ed feat(server): real auth - signup, login, refresh, device flow (Stage 2)`
**Re-review tip:** `20cd0b3 fix(R14): clear remaining codex blockers — stale paths + speculative doc comment`
**Reviewer:** Codex
**Date:** 2026-05-19

## Per-Task Review

### S2.1 — JWT / Password Basics

| Field | Value |
|-------|-------|
| Files | `server/src/auth/jwt.rs`, `server/src/auth/password.rs` |
| Verdict | 🟢 accept |

**Findings:**

- HS256 with one server-held secret is acceptable for the current single-server v0.2 shape.
- 15-minute access tokens and 30-day rotating refresh tokens remain sane defaults.
- Bcrypt cost 12, min length 8, and reject-over-72-byte behavior are reasonable for this stage. Rate limiting and breached-password screening remain later hardening work.

---

### S2.2 — Protected Route Boundary

| Field | Value |
|-------|-------|
| Files | `server/src/api/mod.rs`, `server/src/api/admin.rs` |
| Verdict | 🔴 blocker |

**Findings:**

- 🟢 The original public-route blocker is partially fixed: `/account/*`, `/router/*`, `/billing/checkout`, `/usage/event`, `/auth/device/approve`, and `/admin/customers` now sit behind `auth::require_auth` (`server/src/api/mod.rs:45-65`).
- 🔴 `/admin/customers` is still only normal-auth protected, not admin-role protected (`server/src/api/mod.rs:61`, `server/src/api/admin.rs:33-67`). Any logged-in customer can list other customer IDs, emails, and balances. Because Stage 2 introduced `Account::is_admin`, the fix should be either a `require_admin` middleware or an explicit `AuthedAccount(account)` check in the handler.

---

### S2.3 — Refresh Token Rotation

| Field | Value |
|-------|-------|
| Files | `server/src/api/auth_routes.rs`, `server/src/auth/refresh_store.rs` |
| Verdict | 🟢 accept |

**Findings:**

- The original refresh race is fixed. `/auth/refresh` calls `refresh_store::consume()`, and `consume()` uses one `UPDATE ... WHERE revoked_at IS NULL AND expires_at > ? RETURNING account_id` operation (`server/src/auth/refresh_store.rs:72-105`).
- `validate_and_touch()` now distinguishes DB errors from real token misses; the old silent `.ok()` issue is gone.

---

### S2.4 — Signup / Login Endpoint Behavior

| Field | Value |
|-------|-------|
| Files | `server/src/api/auth_routes.rs`, `server/src/db/accounts.rs` |
| Verdict | 🟡 minor nit |

**Findings:**

- Login still returns the same 401 for unknown email and bad password, which is the right shape before rate limiting.
- The duplicate-signup pre-check is still naturally race-prone; the insert boundary should map SQLite unique violations to 409 as well. This is not the current blocker, but it should be fixed before public signup.

---

### S2.5 — Device Flow

| Field | Value |
|-------|-------|
| Files | `server/src/api/auth_routes.rs`, `server/src/api/mod.rs` |
| Verdict | 🟢 accept |

**Findings:**

- The original fake `device_approve` blocker is fixed. The endpoint now requires `AuthedAccount` from the auth middleware and writes the authenticated account id into `device_codes` (`server/src/api/auth_routes.rs:329-355`).
- `device_approve` is mounted behind `auth::require_auth` with the rest of the protected router (`server/src/api/mod.rs:57-65`).
- 8-character user codes and 10-minute TTL are still acceptable once endpoint rate limiting lands.

---

### S2.6 — Docs / Follow-Up Consistency

| Field | Value |
|-------|-------|
| Files | `docs/PRODUCTION-READINESS.md`, `docs/PRICING-MODEL.md`, `crates/cue-router/src/speculative.rs` |
| Verdict | 🟢 accept |

**Findings:**

- `auto_recap` is now scoped correctly in docs: request-cue paths route through classifier metadata, while `auto_recap` remains direct/default and has `router_meta: None`.
- Pricing source-of-truth and speculative default-ON docs are now explicit enough for future agents.

## Build & Test Verification

```bash
cd server && cargo test --lib          # ✅ 30 passed
cargo test -p cue-router --lib         # ✅ 30 passed
cargo test -p cue-cloud-client --lib   # ✅ 4 passed
```

## Overall Verdict

🔴 **REQUEST CHANGES** — The refresh and device-flow blockers are fixed, but the admin customer endpoint still lacks an `is_admin` gate. This must be fixed before accepting Stage 2 as a safe auth boundary.

## Follow-ups for Next Batch

- Add `require_admin` middleware or an explicit `AuthedAccount(account)` admin check to `/admin/customers`, plus unauthenticated / non-admin / admin tests.
- Map duplicate-email insert races to 409 at the DB insert boundary.
- Add auth/device rate limiting before public rollout.
