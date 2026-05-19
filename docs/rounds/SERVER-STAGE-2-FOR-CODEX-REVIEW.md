# Server Stage 2 — Codex Review Asks

> **Commit:** `07136ed feat(server): real auth - signup, login, refresh, device flow (Stage 2)`
> **Builds on:** Stage 1 (`853be40`).
> **Files:** `server/src/auth/{jwt,password,refresh_store}.rs`,
> `server/src/api/auth.rs`, `server/Cargo.toml`.

Auth endpoints are now real. Daemon-side `cue-cloud-client` and
`bluey login` flow come in Stage 3.

## What I verified locally

- `cargo build` clean.
- `cargo test` passes — 20 server tests (was 7), all green.
- End-to-end curl smoke against the running binary:
  - `POST /auth/signup` → 200 with valid JWTs (access 209 chars, refresh 211 chars), trial_seconds_remaining = 600.
  - `POST /auth/signup` duplicate email → 409 CONFLICT.
  - `POST /auth/login` good password → 200 + new pair.
  - `POST /auth/login` wrong password → 401 UNAUTHORIZED.
  - `POST /auth/refresh` → 200 + new pair.
  - `POST /auth/refresh` with old (rotated) token → 401 UNAUTHORIZED.
  - `POST /auth/device/start` → 200 with user_code (e.g. `PANG-QAAL`) + verification_uri.

## What I want you to review

### 1. JWT design (`server/src/auth/jwt.rs`)

- **Algorithm:** HS256 (symmetric, single secret).
- **Access TTL:** 15 minutes.
- **Refresh TTL:** 30 days.
- **Claims:** `sub` (account_id), `iat`, `exp`, `kind` (access | refresh).
- **Clock skew leeway:** 30s on verify.
- **Single-secret rotation:** not yet implemented. If we rotate
  `BLUEY_JWT_SECRET`, every existing token immediately invalidates.
  Acceptable for v0.2 (forces re-login) but worth flagging.

**Ask:**
- TTLs OK? (15min access / 30d refresh is the OAuth norm; some
  prefer 7-day refresh for tighter compromise window.)
- HS256 OK for v0.2, or want RS256/ES256 with a JWK rotation story
  now? My read: HS256 is fine while the server is single-instance;
  multi-instance or external token verification would need RS256.
- Should `kind` be enforced on every endpoint via middleware (e.g.
  `/router/complete` only accepts kind=access), or accept whichever?
  Currently `/auth/refresh` enforces it; nothing else does (because
  no other endpoint reads tokens yet — Stage 3 work).

### 2. Password hashing (`server/src/auth/password.rs`)

- **bcrypt cost:** 12 (default).
- **Min length:** 8.
- **Max length:** 72 (bcrypt silently truncates after 72 bytes; we
  reject).

**Ask:**
- Cost 12 takes ~250ms on Apple Silicon. Acceptable login latency,
  or prefer cost 11 (~125ms)?
- Min length 8 is industry-standard-low. NIST 800-63B suggests min
  8 + breached-password screening. We don't screen against haveibeenpwned
  yet; flag as a R14.6-class hardening item?
- Reject-at-72 vs silently-truncate: I went with reject so users
  who paste a 100-char passphrase get a clear error rather than
  having only the first 72 bytes count. Confirm.

### 3. Refresh token storage (`server/src/auth/refresh_store.rs`)

- **At rest:** sha256(token) stored in `refresh_tokens` table.
  The token itself never lives in the DB so a DB leak doesn't
  expose live sessions.
- **Lifecycle:** revoked_at column for explicit logout / security
  events; expires_at for natural expiry.
- **Rotation on use:** every successful `/auth/refresh` revokes
  the presented token and issues a new pair.
- **Bulk revoke:** `revoke_all_for_account` for password-change /
  security-event flows.

**Ask:**
- Hashing with raw sha256 (no HMAC, no salt) is OK because the
  source token is high-entropy (HS256 JWT body). Confirm you'd not
  want HMAC-sha256 here.
- Rotation-on-refresh is the OAuth norm but means a flaky network
  can briefly leave a client tokenless. Worth keeping the prior
  refresh valid for ~5s as a grace window? My take: not worth the
  complexity at v0.2 scale; clients should retry from login on
  rotation race.
- `validate_and_touch` updates `last_used_at` on every refresh.
  Useful for forensics; cheap. Confirm.

### 4. Endpoint behavior (`server/src/api/auth.rs`)

- **Email normalisation:** lower-case + trim. Rejected if empty or
  no `@`.
- **Email enumeration defense:** `/auth/login` returns the same 401
  whether the email is unknown OR the password is wrong. No timing
  attack mitigation (bcrypt verify is constant-time-ish; no
  artificial delay).
- **Duplicate signup:** 409 CONFLICT.
- **Refresh JWT-vs-DB cross-check:** the JWT's `sub` claim must
  match the DB row's `account_id` for the rotation to succeed.
  Defense in depth — if the JWT was somehow forged with a different
  signature secret, the DB check still catches.

**Ask:**
- Email enumeration: should `/auth/signup` also be rate-limited (a
  bot could enumerate which emails are registered via the 409 vs
  201 split)? Pinky has rate limiting; I don't yet. Flag as Stage
  3.5 hardening?
- The `device_approve` endpoint is currently a STUB — it accepts a
  user_code without auth and stamps `account_id = "test-account-id-stub"`.
  This is broken for production; needs the Stage 3 auth middleware
  + a logged-in customer in the browser. Flagging that I know it's
  a stub; will fix in Stage 3.

### 5. Device flow (`server/src/api/auth.rs::device_*`)

- **device_code:** 32 bytes from `getrandom`, hex-encoded (64 chars).
- **user_code:** 8 chars from a base32-style alphabet (no 0/O/1/I/l),
  hyphenated `XXXX-XXXX`. Generated via `getrandom` with bias
  (modulo of 256 against 32-char alphabet → slight 6.25% bias on
  some letters). Acceptable for an 8-char human code; not for
  cryptographic material.
- **TTL:** 10 minutes.
- **Poll interval suggestion:** 5 seconds.
- **Approval state:** `device_codes.approved` flag flipped by
  `/auth/device/approve` (currently stubbed); poll then issues the
  AuthResponse and deletes the device_code (single-use).

**Ask:**
- 8-char user_code: enough entropy? `32^8 ≈ 1e12` so brute-force
  during the 10-min window would need ~1.6M req/s. Sufficient with
  rate-limiting on /auth/device/poll. Confirm.
- The 6.25% modulo bias on user_code chars: bother fixing now or
  defer? My take: defer — 8-char codes are about UX more than
  security.
- Single-use device_code on successful poll: device row is deleted.
  Should we instead mark it consumed + keep for forensics? Pinky
  deletes; I followed.

### 6. Cargo deps

Added `sha2 0.10`. No other changes.

## What's NOT in this stage

- **Auth middleware** — no endpoint other than `/auth/refresh`
  actually reads JWTs yet. Middleware lives in Stage 3 alongside
  the daemon-side `cue-cloud-client`.
- **`/auth/device/approve` real impl** — needs the auth middleware
  (knows the logged-in customer) + a browser session cookie. Stage 3.
- **Rate limiting** — Pinky has middleware-level rate limits on
  /auth/login etc. Not in Stage 2; Stage 3.5 hardening.
- **Email verification flow** — `email_verified_at` column exists in
  schema; the verify endpoint isn't built. Stage 3.5 or 4 — not
  blocking for the daemon to log in (we'll allow unverified accounts
  to use Bluey, just gate billing reload on verification).
- **Password reset flow** — same; schema-ready, endpoint not built.
- **Pricing microcent unit-conversion bug** — flagged in Stage 1;
  still flagged. Will fix in Stage 4 alongside the real
  `/router/complete` impl.

## Suggested verdict shapes

- 🟢 **ACCEPT:** auth design + impl looks right; proceed to Stage 3
  (cue-cloud-client + auth middleware + real device_approve).
- 🟡 **ACCEPT WITH NITS:** flag specific items (TTL choice, bcrypt
  cost, rate-limit timing, email enumeration mitigation) — I
  address inline on Stage 3.
- 🔴 **REQUEST CHANGES:** structural issues with JWT, refresh
  rotation, or device flow; fix before Stage 3 lands.
