# FIX-786: Authenticated workspace exceeded the portal deadline for large histories

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this correction against the current
> Phase 626 worktree and read-only production evidence. No archive fact, production mutation,
> deployment, release flag, provider action, or customer effect was used.

## Issue

Bluey Jobs could not reliably open an authenticated workspace for established accounts. Production
contained 3,954/5,478-posting accounts, and an observed workspace request took 21.448 seconds
against the portal's 20-second read deadline. The accepted implementation fetched 501 rows and
returned an error whenever total history exceeded 500.

## Root Cause

`server/src/db/jobs/workspace.rs` treated an interactive screen as an exhaustive account-history
read. It loaded all account representation inputs and up to 501 postings, rejected totals above
500, and composed current source/ATS/integrity authority per posting. The source resolver also
reread account-global managed-source authority for each posting even when that feature was
inactive, producing at least two SQL calls per posting in the observed state. On a positive source
page, ATS certification was still resolved once per posting after source and integrity reads had
been batched. The portal exposed no continuation contract, so it could neither render a bounded
first page nor continue deterministically. A first-page-only correction also exposed lost
application-history context, cursor leakage through GET access logs, and page-coupled legacy
verified-import repair.

## Fix Summary

- Replaced the total-history interactive load with stable keyset pages: 50 initially, maximum 100.
- Bound each request to one encrypted-document candidate window plus a look-ahead row; sparse
  interactive results can continue with a short or empty page instead of scanning historic rows.
- Added an encrypted version-1 cursor bound to account, page size, key tuple, and a 30-minute TTL.
- Batched page discovery input and resolved account-global managed-source authority once per page;
  inactive authority now exits before per-posting membership/head, ATS, or integrity SQL.
- Bound active source membership by relational `posting.id` while independently revalidating the
  canonical job key, provider job ID, and source identity; a UUID posting ID may differ from its
  canonical key without losing a valid source binding.
- Replaced scalar-per-posting ATS composition with one page snapshot covering trust, both rollout
  heads, canary membership, evidence, layouts, runtimes, quarantine, and circuit state. Policy and
  policy-key revocation are one-row global flags; non-global revocation output is limited to the
  exact page subject set and keyed for bounded lookup. The evaluator retains complete scalar
  semantics and fail-closed identity checks.
- Batched positive-page signed-integrity control, policy, head, attestation/transition, and current
  revocation reads beneath the existing PostgreSQL publication fence; per-record cryptographic and
  source-binding checks remain fail-closed.
- Kept positive source/ATS/integrity truth fail-closed and retained every existing lock/fence.
- Kept exact-ID detail/action revalidation and the separate exhaustive export pager, while
  constraining each export authority-composition chunk to the resolver's 100-row envelope.
- Added tenant-bound, display-only `application_jobs` for application history outside the ranked
  page. Frozen receipt labels are preferred; malformed legacy snapshots use bounded posting
  fallback. Exact detail/effect endpoints remain the only current authority.
- Moved continuation to authenticated, deny-unknown `POST /api/jobs/matches/page` JSON and made
  both workspace and continuation responses `private, no-store`; portal requests also bypass the
  browser cache. Cursors stay out of request URIs and browser storage.
- Kept legacy `GET /api/jobs/matches` exhaustive through 500 rows and return an explicit
  pagination-required conflict above 500 instead of silent truncation.
- Replaced page-1 repair with a 50-row background verified-import migration using durable
  per-account/version ID checkpoints, owner-token-expiry leases claimed before worker spawn,
  monotonic CAS, and partial import-only indexes. Live SQLite leases exit through a read-only fast
  path. Parse, decrypt, and storage failures release the lease but retain the checkpoint;
  relational identity overrides stale JSON.
- Added deterministic portal load-more bound to both the current cursor and exact installed
  workspace authority epoch; a newer refresh or authority-sensitive mutation rejects the old
  response. Loaded-row scope copy remains truthful.
- Added paired query-order indexes and privacy-safe request-correlated stage/count diagnostics.
- Required exact current detail and preparation eligibility before portal handoff metering. The
  first-commit transaction now reloads and recomposes current profile, safe preferences, exact
  Career Track/policy ledger, application-aware reservations, discovery, signed source
  destination, ATS, and integrity authority before allowance, debit, ledger, or meter effects;
  exact replay remains first and side-effect-free.
- Restricted page canary, quarantine, and circuit state to exact bounded page identities aligned
  with their leading primary keys, rather than materializing unrelated authority history.
- Enforced signed evidence/layout/runtime child cardinalities in scalar and page resolvers, and
  required database-time-live ownership for a failed repair worker to release its lease. The
  secret-bearing lease is neither debug-printable nor cloneable.
- Made Command Center freshness explicitly loaded-row scope while a continuation remains; a zero
  first page is no longer presented as zero fresh matches in the account.
- Added a rollback-only PostgreSQL first/deep keyset `EXPLAIN` fixture for 5,500 tied rows. Its
  expression order makes the continuation predicate an index range seek.
- Fixed the SQLite repair claimant's missing sixth liveness binding and moved PostgreSQL packet
  locking to exact replay → global authority fences → parent Account (`FOR NO KEY UPDATE`) →
  policy → exact application/posting → entitlement (`FOR UPDATE`) → relevant reservations. The
  entitlement row is reused for metering, while one post-lock database timestamp governs current
  composition, posting/discovery freshness, reservation period, packet/allowance timestamps,
  entitlement updates, and any overage ledger entry. Both dialects read at most one indexed active
  same-company row plus 50 indexed active current-period rows, excluding the current application
  and de-duplicating IDs. First-claim, static-order, exact-replay, zero-effect drift, configured
  Account-deletion, and entitlement/reservation contention fixtures preserve the boundaries.
- Added source-positive 102-row exhaustive-export fixtures for SQLite and configured PostgreSQL,
  proving full ordered traversal and authority projection across the 100/2 chunk boundary.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/workspace.rs` | Keyset/cursor contract, bounded selection, 100-row export chunks, diagnostics, exhaustive/detail/action regressions |
| `server/src/db/jobs/applications.rs`, `eligibility.rs`, `tests.rs` | Current-effect recomposition, replay/Account/A/E/bounded-R order, database-time eligibility, zero-effect SQLite drift regressions |
| `server/src/db/balance.rs` | Explicit database-time ledger insertion for serialized packet effects |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | PostgreSQL drift/replay/Account-deletion and entitlement/reservation contention fixtures |
| `server/src/db/jobs/original_source_verification.rs` | One account-global source-authority read per page; posting-ID/canonical-key binding and explicit errors |
| `server/src/db/jobs/ats_certification_page.rs` | SQLite/PostgreSQL page ATS snapshots, scalar-parity evaluator, and statement-budget regressions |
| `server/src/db/jobs/ats_certification_authority.rs` | Scalar/page scope-channel identity joins and signed parity fixtures |
| `server/src/db/jobs/job_integrity_composition.rs` | Page composition over batched source, ATS, and integrity authority |
| `server/src/db/jobs/job_integrity_authority.rs` | Bounded page reads for signed-integrity authority |
| `server/src/db/jobs.rs` | Additive workspace/page wire types |
| `server/src/api/jobs.rs`, `server/src/db/jobs/discovery.rs` | POST continuation, background repair, complete SQLite claim binding, aggregate logs/tests |
| `infra/sqlite/server-runtime/061_jobs_workspace_pagination.sql` | Page/repair indexes and checkpoint |
| `infra/postgres/server-runtime/039_jobs_workspace_pagination.sql` | Page/repair indexes and checkpoint |
| `server/src/db/mod.rs` | Paired runtime migration registration and parity guard |
| Portal API/types/app/views/components/libs | Strict pages, application context, exact actions, loaded scope |
| Portal/server tests and preview fixture | Cursor plus authority epoch, pagination, compatibility, and UI state coverage |
| `CHANGELOG.md`, Round 626, FIX-786, Phase 626 IMPL | Auditable scope and evidence |

## Edge Cases Handled

- Equal score and update time use relational ID as the deterministic tiebreaker.
- Relational ID/order fields override stale encrypted document fields.
- Closed/expired postings are excluded interactively but detail/export remain reachable.
- Manual/untracked, application-linked, and current healthy/waiting source memberships remain
  represented under existing semantics.
- Cursor tamper, truncation, unknown fields, account reuse, page-size change, expiry, clock skew,
  and data-key rotation return one non-oracular invalid-or-expired response.
- Concurrent/stale portal responses cannot overwrite a newer cursor or cross a workspace refresh/
  mutation authority epoch; duplicate IDs are ignored.
- Search/filter/sort/count scope is identified as loaded-only while `has_more` is true.
- General and canary heads may share one signed manifest: identical policy rows are coalesced,
  general wins while active, and enrolled canary fallback matches the scalar resolver after expiry.
- Corrupted head/activation channel or scope identity fails closed in scalar and page paths.
- Oversized pages fail before placeholder/array construction; child result sets are capped from
  exact manifest count and reject non-contiguous or 65th evidence/layout and 33rd runtime rows
  before revocation lookup.
- No cursor or user/job payload is logged or stored in browser storage.
- Persisted tie-breakers are not trimmed or capped at 240 characters; cursor generation enforces
  the same 2,048-byte ceiling as decoding and never emits an unusable continuation.
- 501 distinct application jobs remain available without ranked-list inflation or full authority
  composition.
- A 102-row fully positive export crosses the 100-row authority resolver envelope without
  truncation, oversize rejection, or tenant/source/ATS/integrity projection loss.
- Current Track activation, profile truth, or preference inclusion drift rejects the first packet
  effect with balance, credits, entitlement usage, allowance, overage ledger, and meter unchanged;
  an exact prior meter remains replayable before those current-state checks.
- PostgreSQL packet commit cannot lock an application/posting child before its parent Account or
  read a reservation before locking the entitlement. Account deletion can fence/cascade without
  the former inversion, while an entitlement owner can insert a relevant reservation without an
  Account foreign-key timeout; packet commit then re-reads it and fails before any packet effect.

## How to Test

```bash
# Run Rust commands only after integration into a successor that contains the canonical launcher.
bash scripts/run-bluey-tests.sh -- \
  cargo +1.95.0 fmt --manifest-path server/Cargo.toml --all -- --check
bash scripts/run-bluey-tests.sh -- \
  cargo +1.95.0 check --manifest-path server/Cargo.toml --lib

# Hosted/existing dependency cache required before release.
npm --workspace @bluey/jobs-portal run typecheck
npm --workspace @bluey/jobs-portal test

scripts/check-bluey-ops-docs.sh
git diff --check
```

This Phase 626 base does not contain `scripts/run-bluey-tests.sh`. Its prior isolated target grew to
approximately 14 GiB physical (28.1 GiB apparent). After its command was proven inactive, the
target was cleaned exactly with `cargo clean --target-dir
/private/tmp/bluey-phase626-ats-check.rJ8Soi/target`. The measured residuals were 277 MiB in the
temporary root and 172 KiB in its cargo-home, not empty; the 277 MiB temporary root was then moved
to Trash. No further Rust command is authorized in this worktree. Future Rust evidence is blocked
until the change is integrated into a successor with the canonical auto-cleanup launcher (or an
equivalently reviewed disposable launcher).

Focused isolated SQLite correction evidence executed on 2026-08-31:

- six ATS scalar/page full-JSON parity and budget tests passed, covering signed general/canary,
  general-first and shared-manifest canary fallback, expiry, wrong enrollment, malformed targets,
  evidence/layout/runtime loss, revocation, quarantine, circuit, and scope corruption;
- 50 distinct positive ATS targets used exactly 10 `SELECT` statements;
- 50 fully positive source-to-ATS-to-integrity compositions used 19 `SELECT` statements;
- the positive UUID-posting-ID/canonical-key membership regression and existing public-lifecycle
  production-positive composition regression passed;
- the 501-application context, frozen-expired label, malformed-snapshot fallback, row-51 durable
  repair/restart, parse-failure retention, CAS monotonicity, and partial-index regressions passed;
- portal TypeScript and all 382 Vitest cases passed with disposable local dependencies.

Those passes predate the final single-flight repair lease, exact-subject ATS revocation, workspace
no-store, the five earlier RED-blocker corrections, and the later entitlement/reservation
correction. No Rust command was started after the storage stop. The retained dependency tree has
neither `vitest` nor `tsc`, so no corrected portal suite was started; no install or network access
was used. Post-correction safe guard and manifest evidence records passing Bluey ops docs, Jobs
client/server boundary, edge/crawler policy, and diff hygiene. The fresh independent full
line-by-line source/static review is GREEN with P0/P1/P2/P3 all zero. Exact corrected-source
Rust/Cargo, portal, configured PostgreSQL, and hosted CI evidence remains mandatory.

Hosted PostgreSQL plan gate (only with an isolated, migrated test database): run
`server/tests/fixtures/jobs_workspace_postgres_explain.sql` and retain both plans. Each must use
`idx_jobs_postings_account_workspace_page` without a sequential scan or Sort; do not run this
fixture against any shared or production database.

The exact-source successor gate must also execute the 102-row positive export fixtures, SQLite
first-claim regression, both-dialect packet current-effect drift and exact-replay regressions,
configured PostgreSQL parent-Account deletion and entitlement/reservation contention races, and
portal workspace-authority continuation tests. Their presence in source is not a pass.

## Known Limitations

- The formal source/static verdict is GREEN with P0/P1/P2/P3 all zero. Exact-source Rust/Cargo,
  portal typecheck/Vitest, configured PostgreSQL, and hosted CI execution remain pending; this is
  not yet an accepted release candidate.
- Configured PostgreSQL execution and the full exact-tip Rust/Jobs matrices remain release gates.
- The continuation response is page-only: it does not reload workspace-wide facts, applications,
  evidence, sessions, events, integrations, or discovery summaries. Existing workspace consumers
  keep their original response shape; their next full refresh remains authoritative.
- `/api/jobs/workspace` intentionally changes to first-page semantics; older clients that ignore
  `matches_page` see at most 50 matches. Legacy `GET /api/jobs/matches` remains exhaustive through
  500 and returns an explicit pagination-required conflict above that envelope.
- A keyset traversal is deterministic for stable ordering values. A posting updated between page
  requests is current state, not a historical snapshot; the 30-minute cursor TTL bounds drift.
