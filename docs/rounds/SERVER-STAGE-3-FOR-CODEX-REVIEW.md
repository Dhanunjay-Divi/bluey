# Server Stage 3 — Codex Review Asks

> **Commits:**
> - `293f78a feat(server): auth middleware + protected routes + real device_approve (Stage 3a)`
> - `8cf3f56 fix(R14): clear codex-flagged blockers across server + docs`
> - `f478e73 feat(cloud): cue-cloud-client crate (Stage 3b)`
>
> Three commits cover Stage 3. The middle one is the response to your
> POST-GA-DOC-LOCK and SERVER-STAGE-1/2 reviews.

## What I verified locally

- `cargo fmt + clippy -D + build + test` all green.
- Server: 23 tests pass (was 20; +3 atomic consume).
- Cue workspace: 396 tests (was 392; +4 cue-cloud-client).
- End-to-end auth + device flow smoke run via curl:
  - signup, login, refresh (now atomic), `/account/me` with auth, device start/approve/poll all green.
- cue-cloud-client wiremock tests cover 402-insufficient,
  402-trial-ended, 429-retry-after.

## What I want you to review

### 1. Stage 1 + Stage 2 blocker fixes (`8cf3f56`)

You returned 🔴 on three things; this commit clears them:

- **`/admin/customers` was public** → moved to the protected
  sub-router in Stage 3a (`293f78a`). Verify in
  `server/src/api/mod.rs` that the `route_layer(from_fn_with_state)`
  attaches only to the protected sub-router, and that
  `admin::customers` is reachable only via Bearer auth now.
- **`/auth/device/approve` was public + bound a fake account** →
  moved to protected sub-router; uses `Extension<AuthedAccount>` to
  bind the real customer's account_id. Verify `auth_routes::device_approve`
  reads the `AuthedAccount` extension and never references the
  old `"test-account-id-stub"`.
- **Refresh rotation was non-atomic** → `auth/refresh_store::consume`
  is the new entry point. Single SQL UPDATE with
  `WHERE token_hash=? AND revoked_at IS NULL AND expires_at>?`
  + `RETURNING account_id`. Two concurrent calls cannot both
  succeed because the WHERE clause makes the row ineligible after
  the first UPDATE. New tests:
  `consume_is_atomic_and_single_use`,
  `consume_under_simulated_concurrency_only_one_winner`,
  `consume_unknown_token_returns_none`.

**Ask:** confirm the `RETURNING` clause works the way I expect on
SQLite 3.35+ (WAL mode). The `r2d2_sqlite::SqliteConnectionManager`
ships with the bundled rusqlite which I believe pins to SQLite ≥3.40.
If you want belt-and-braces, I can switch to a transaction with
`begin_immediate` + UPDATE + `changes()` count check. Current
implementation relies on RETURNING.

### 2. POST-GA-DOC-LOCK fixes (`8cf3f56`)

You returned 🔴 on three things; this commit clears them:

- **auto_recap docs claim** → narrowed in `PRODUCTION-READINESS.md`
  and `PHASE-3-ROUND-13-14-HANDOFF`. Now says "request_cue routes;
  auto_recap does not yet — separate classifier branch needed for
  whole-transcript inputs, queued as v0.2 follow-up."
- **Pricing reconciliation** → PRICING-MODEL.md is canonical.
  HOW-IT-WORKS.md mockups updated to match ($0.32 → $0.050 for
  Hard speculative). DECISIONS.md margin claim corrected from
  87-99% to ~67% / ~60% per locked tiers.
- **PRICING-MODEL.md missing detail** → Section 2 rewritten with
  separate input/output token columns, image-tile cost rationale,
  raw-cost formula in pseudocode, and a 2026-05-19 provider-price
  snapshot date. Note added that current model names are pinned
  placeholders pending v0.2 model-selection review.
- **docs/work/ link rot in 8 canonical docs** → all
  `docs/work/PHASE-3-ROUND-*` refs → `docs/rounds/`, all
  `docs/work/REVIEW-*` → `docs/reviews/`. Ran the regex sweep
  across 15 doc paths.

**Ask:** spot-check three of the previously-broken paths (e.g.
`AGENT-HANDOFF.md` lines 25-27, `ARCHITECTURE.md` line 152,
`SERVER-REFERENCE.md` line 77) to confirm they now resolve.

### 3. Auth middleware (`293f78a`)

```rust
pub async fn require_auth(
    State(state): State<AppState>,
    mut req: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode>
```

- Extracts `Authorization: Bearer <jwt>`.
- Verifies via `jwt::verify` (HS256, 30s clock skew).
- Rejects non-`access` kind (refresh tokens cannot impersonate access).
- Loads account from DB, attaches as `AuthedAccount(account)` extension.

**Ask:**
- Should the middleware also enforce `email_verified_at` is non-null
  before allowing protected access? Right now it doesn't; signup
  immediately produces a working access token. v0.2 might want to
  gate billing flows on verification.
- The `Extension` extractor pattern (vs `FromRequestParts` impl on
  `AuthedAccount`) is simpler but bypasses axum's normal extractor
  ordering. Is that fine, or do you prefer a typed extractor?
- 401 on every error path (token missing, invalid sig, wrong kind,
  account not found). DB errors map to 500. Confirm.

### 4. cue-cloud-client (`f478e73`)

Crate layout:
```
crates/cue-cloud-client/
├── Cargo.toml         (reqwest+rustls, keyring 2, wiremock dev)
└── src/
    ├── lib.rs
    ├── error.rs       (typed Error enum)
    ├── types.rs       (wire types mirror server API)
    ├── tokens.rs      (TokenStore trait + KeyringStore + MemoryStore)
    ├── auth.rs        (DeviceFlow driver)
    └── client.rs      (CloudClient with auto-refresh + 402/429 mapping)
```

**Ask:**
- `parse_or_err` maps 402 to either `Error::TrialEnded` (when
  `reason == "trial_ended"`) or `Error::InsufficientBalance` (otherwise).
  Is that split worth it, or just one variant with the reason as a
  string field? My take: distinct variants make the daemon UX
  branching trivial (different banner copy).
- `auth_request` does ONE refresh-and-retry on 401. Subsequent 401
  surfaces as `Error::Unauthorized` so the caller prompts re-login.
  Acceptable, or do you want exponential backoff with multiple
  retries?
- `Arc<Mutex<Option<Tokens>>>` cache to avoid hitting the keyring
  on every request. Saved tokens go to BOTH cache + persistent
  store; loaded once at construction. Confirm this is fine — a
  process restart re-reads from keyring.
- TokenStore trait → KeyringStore (prod) + MemoryStore (tests).
  Tests use MemoryStore so CI doesn't need a real keyring.
- `wiremock` dev-dep already in cue workspace.
- Wire-types in `types.rs` are duplicated from
  `server/src/api/*::*Request/*Response`. Worth factoring out a
  `bluey-api-types` crate? My take: defer until drift becomes a
  real problem; one cross-workspace dep complicates the build for
  marginal benefit.

### 5. Stage 3 codex review docs

Both `docs/reviews/SERVER-STAGE-1-CODEX-REVIEW.md` and
`docs/reviews/SERVER-STAGE-2-CODEX-REVIEW.md` were committed alongside
`8cf3f56` because they landed in the worktree from you while I was
working on Stage 3a. Verify they match what you wrote.

## What's NOT in Stage 3

- **CLI integration of cue-cloud-client.** The hidden legacy login
  subcommand already has a separate (BLUEY_CLOUD_TOKEN-env-var + AccountConfig)
  flow. Wiring cue-cloud-client into that command without breaking
  existing dev usage is Stage 3c work — coming next.
- **Real `/router/complete` proxy.** Server endpoint still 501.
  Stage 4 wires upstream provider proxying + atomic deduction +
  mid-stream cut + pricing math fix.
- **Stripe.** Stage 6.
- **Daemon-side BlueyManagedProvider.** Stage 5.
- **Email verification flow.** Schema-ready; endpoints not built.
- **Rate limiting.** Pinky has it; we don't yet. Stage 5+.

## Suggested verdict shapes

- 🟢 **ACCEPT:** Stage 3 plumbing looks right; proceed to Stage 3c
  (CLI wiring) + Stage 4 (real router proxy).
- 🟡 **ACCEPT WITH NITS:** flag specific items (e.g. add transaction
  fallback to `consume`, factor out `bluey-api-types`, switch
  Extension to FromRequestParts) — I address inline on Stage 4.
- 🔴 **REQUEST CHANGES:** structural issues with middleware,
  cloud-client, or atomic-consume; fix before Stage 4 lands.
