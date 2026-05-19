# REVIEW: Server Stage 2 — Auth

**Commit:** `07136ed feat(server): real auth - signup, login, refresh, device flow (Stage 2)`
**Builds on:** `853be40`
**Reviewer:** Codex
**Date:** 2026-05-19

## Per-Task Review

### S2.1 — JWT / Password Basics

| Field | Value |
|-------|-------|
| Files | `server/src/auth/jwt.rs`, `server/src/auth/password.rs` |
| Verdict | 🟢 accept |

**Findings:**
- HS256 with one server-held secret is acceptable for a single-instance v0.2 server where no third party verifies tokens.
- 15-minute access tokens and 30-day rotating refresh tokens are a sane default.
- Bcrypt cost 12, min length 8, and reject-over-72-byte behavior are all reasonable. Add breached-password screening later with rate limiting, not as a blocker for this stage.

---

### S2.2 — Public Route Table / Auth Boundary

| Field | Value |
|-------|-------|
| Files | `server/src/api/mod.rs`, `server/src/api/admin.rs`, `server/src/api/auth.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 Stage 2 still mounts every route publicly (`server/src/api/mod.rs:29-55`). That includes `GET /admin/customers`, which returns customer IDs, emails, and balances (`server/src/api/admin.rs:33-67`), and `POST /auth/device/approve`, which mutates device-code state (`server/src/api/auth.rs:285-310`). Even if most product endpoints return 501, these two are live and should not be public.
- 🔴 `device_approve` is not a safe placeholder. It writes `account_id = "test-account-id-stub"` for any submitted user code (`server/src/api/auth.rs:299-304`), so a public caller can approve a device code into a broken state; subsequent poll tries to fetch that fake account and fails. Either keep approve unimplemented until middleware exists, or implement access-token auth and bind the real account now.

---

### S2.3 — Refresh Token Rotation

| Field | Value |
|-------|-------|
| Files | `server/src/api/auth.rs`, `server/src/auth/refresh_store.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 Refresh rotation is not single-use under concurrency. `/auth/refresh` validates the old token, then later revokes it in a separate DB operation (`server/src/api/auth.rs:144-159`). Two concurrent refreshes can both pass `validate_and_touch()` before either `revoke()` runs, and both can mint a new pair. Replace this with an atomic consume operation, e.g. one transaction or `UPDATE refresh_tokens SET revoked_at = now WHERE token_hash = ? AND revoked_at IS NULL AND expires_at > now`, then only issue the replacement if exactly one row was updated.
- 🟡 `validate_and_touch()` maps query errors to `None` via `.ok()` (`server/src/auth/refresh_store.rs:41-48`). For auth failures that is fine, but DB/schema errors should not be silently treated as invalid tokens. Return errors for DB failures and `Ok(None)` only for a real not-found row.

---

### S2.4 — Signup / Login Endpoint Behavior

| Field | Value |
|-------|-------|
| Files | `server/src/api/auth.rs`, `server/src/db/accounts.rs` |
| Verdict | 🟡 minor nit |

**Findings:**
- 🟢 Login returns the same 401 for unknown email and bad password, which is the right shape before rate limiting.
- 🟡 Signup has a duplicate-check race: it pre-checks email (`server/src/api/auth.rs:91-98`) then inserts (`server/src/api/auth.rs:103-104`). A concurrent duplicate can still hit the UNIQUE constraint and return 500. Map SQLite unique violations to 409 at the insert boundary too.
- 🟡 Add max email length / normalized email tests before public signup. Lowercase+trim+contains-`@` is adequate for internal testing but too loose for a paid account surface.

---

### S2.5 — Device Flow

| Field | Value |
|-------|-------|
| Files | `server/src/api/auth.rs`, `server/src/db/mod.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The committed Stage 2 device flow is not end-to-end real despite the stage title. `device_start` and `device_poll` exist, but `device_approve` is public and binds a fake account, so a normal start -> approve -> poll flow cannot produce valid tokens unless a fake account happens to exist.
- 🟡 Keep 8-character user codes and 10-minute TTL; that is enough once `/auth/device/approve` and `/auth/device/poll` are rate-limited.
- 🟡 Consider marking consumed device rows rather than deleting them if support/audit matters. Delete-on-success is acceptable for the first product cut.

## Follow-up Commit Note

A follow-up commit landed during review: `293f78a feat(server): auth middleware + protected routes + real device_approve (Stage 3a)`. It starts addressing the public-route and device-approval blockers above, but it is not part of the reviewed Stage 2 commit. As of this review, the branch-tip `server` crate passes:

```bash
cd /Users/uno/Downloads/cue/server && cargo test                         # ✅ 20 passed
cd /Users/uno/Downloads/cue/server && cargo clippy --all-targets -- -D warnings  # ✅
```

That follow-up should be reviewed separately as Stage 3a or as a Stage 2 fix wave. The Stage 2 verdict remains based on commit `07136ed`.

## Build & Test Verification

Committed Stage 2 snapshot, reviewed in a detached worktree:

```bash
cd /tmp/bluey-stage2-review/server && cargo test                         # ✅ 20 passed
cd /tmp/bluey-stage2-review/server && cargo clippy --all-targets -- -D warnings  # ✅
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Signup/login/JWT/password pieces are sound, but public admin/customer data, public fake device approval, and non-atomic refresh rotation must be fixed before Stage 3 builds cloud-client or auth middleware on top.

## Follow-ups for Next Batch

- Add real auth middleware and apply it to account, router, billing checkout, usage ingestion, device approve, and admin routes.
- Require admin role for `/admin/customers`.
- Replace refresh validate+revoke with atomic consume-and-rotate.
- Make `device_approve` bind the authenticated account or return 501 until browser session auth exists.
- Add endpoint-level integration tests for signup/login/refresh rotation replay, protected route unauthorized/authorized, and device start -> approve -> poll.
