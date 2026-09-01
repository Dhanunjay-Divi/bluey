# Implementation — Phase 626 Jobs Authenticated Workspace Pagination

**Status:** GREEN independent source/static review with P0/P1/P2/P3 all zero; exact-source
Rust/Cargo, portal, configured PostgreSQL, hosted CI, deploy, migration, flag, and production
release gates remain pending

**Branch:** `feat/phase-626-jobs-workspace-pagination`

**Base/working HEAD:** `95696dd0`

## Scope

**Does:** bound authenticated workspace matches to 50 rows; provide a maximum-100 stable
continuation contract; preserve interactive membership semantics; batch page discovery and
account-global source authority; batch positive ATS and integrity authority without weakening
scalar semantics; retain exhaustive export and exact-ID action/detail authority; add paired
indexes, privacy-safe stage/count logs, portal continuation UX, display-only application history
context, and durable background legacy-import repair.

**Does NOT:** change matching scores, silently search unloaded rows, weaken ATS/source/integrity or
effect fences, enable an external action, apply either migration, deploy, or claim hosted evidence.

## Independent-review corrections

The final local review found six additional release-relevant gaps. The candidate now loads and
checks exact current job detail and current preparation eligibility before a portal handoff can
meter a packet or navigate. Independently of the browser, a first packet commit reloads the
tenant-bound application and posting inside the debit transaction, recomposes current
source/ATS/integrity authority beneath the established PostgreSQL fences, and rejects unavailable
or authority-drifted jobs before any new debit or meter. An already-metered exact replay remains
idempotent and performs no fresh side effect.

ATS page reads now restrict canary membership to the exact bounded allowlist hashes referenced by
the page. Quarantine and circuit reads use exact page-derived primary-key identities rather than
account-global history scans. Signed manifest evidence, layout-observation, and runtime-target
counts are projected and checked against the exact loaded child cardinalities in scalar, SQLite
page, and PostgreSQL page paths; missing tails, unexpected contiguous children, invalid ordinals,
or envelope overflow fail closed.

Finally, a failed verified-import worker may release only its still-live lease according to
database time. An expired owner cannot clear even its own unreplaced claim, and the raw-token lease
type intentionally implements neither `Debug` nor `Clone`. That correction set remained yellow at
that stage pending another independent review and exact-source hosted gates.

A subsequent independent review kept the candidate RED on five additional blockers. The
then-unreviewed correction source:

1. loads the current profile, safe preferences, exact relational Track plus policy ledger,
   application-aware attempt reservations, discovery memberships/sources, composed discovery
   evidence, signed source destination, ATS resolution, and integrity resolution in the same first
   packet-commit transaction; it requires current preparation eligibility before allowance,
   debit, ledger, or meter effects and retains the stronger queued auto-submit gate;
2. keeps exact meter replay before every new fence/revalidation, while asserting Track, profile,
   and company-exclusion drift leave account balance, credit remainder, entitlement usage,
   allowance state, overage ledger, and metering unchanged in SQLite and configured PostgreSQL;
3. bounds each exhaustive export authority-composition chunk to 100 and adds 102-row fully
   source/ATS/integrity-positive coverage for both dialects;
4. supplies the missing sixth SQLite verified-import claim parameter and moves the PostgreSQL
   Account deletion/write fence before policy and application/posting children, with first-claim,
   static-order, and configured deletion-race fixtures; and
5. binds portal load-more response installation to the exact installed workspace authority epoch,
   so either a newer refresh or any authority-sensitive mutation rejects the old response even if
   its cursor still matches.

At that stage, author corrections did not clear the RED verdict; independent review and
exact-source execution remained mandatory.

The next independent review reported exactly one remaining P1 and no P0, P2, or P3 finding. Packet
commit still loaded full-account reservation history before locking the entitlement, allowing
application → reservation → entitlement ordering and a stale reservation decision after an
entitlement wait. The then-unreviewed author correction:

- retains the initial exact-meter replay before all locks;
- orders PostgreSQL global fences → parent Account (`FOR NO KEY UPDATE`) → policy → exact
  application/posting → entitlement (`FOR UPDATE`) → relevant reservations, and reuses the locked
  entitlement values for metering;
- captures one database timestamp only after the entitlement lock, using it for current composed
  authority, posting/discovery freshness, reservation period, packet and allowance timestamps,
  entitlement updates, and any overage ledger entry;
- replaces both dialects' full-account reservation materialization with one indexed active
  same-company row plus at most `daily_limit.clamp(1, 50)` indexed active current-period rows,
  excluding the current application and de-duplicating IDs (51 rows maximum); and
- adds static source proof that no reservation SQL appears between application/posting and
  entitlement plus a configured-PostgreSQL contention fixture. The fixture holds entitlement E,
  observes the real packet commit waiting specifically on E, inserts a relevant R without timing
  out on the Account foreign-key check, releases E, and requires the commit to re-read R and reject
  with balance, credits, entitlement usage, allowance, overage ledger, and meter unchanged.

After correcting one database-time gap and one overly broad error assertion found in its first
bounded review, the same bounded reviewer returned GREEN. The subsequent fresh independent full
line-by-line review of all 50 paths returned **GREEN for source/static review: P0: 0, P1: 0,
P2: 0, P3: 0**. Exact-source Rust/Cargo, portal, configured PostgreSQL, and hosted CI execution
remain pending.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `server/src/db/jobs/workspace.rs` | Modified | Bounded keyset pages, encrypted cursor, page batching, detail/export preservation, diagnostics/tests |
| `server/src/db/jobs/applications.rs`, `eligibility.rs`, `tests.rs` | Modified | Transactional current-effect recomposition, replay/Account/A/E/bounded-R order, database-time eligibility, zero-effect drift proofs |
| `server/src/db/balance.rs` | Modified | Explicit database-time ledger insertion used by packet effects after entitlement serialization |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | Modified | Configured PostgreSQL effect-drift, replay, Account-deletion, and entitlement/reservation contention fixtures |
| `server/src/db/jobs/original_source_verification.rs` | Modified | Resolve account authority once per page and bind membership by posting ID plus canonical identity |
| `server/src/db/jobs/ats_certification_page.rs` | Created | Set-based SQLite/PostgreSQL ATS page reads and scalar-equivalent fail-closed evaluation |
| `server/src/db/jobs/ats_certification_authority.rs` | Modified | Scope/channel identity joins and signed general/canary parity fixtures |
| `server/src/db/jobs/job_integrity_composition.rs` | Modified | Compose one page from batched source, ATS, and integrity projections |
| `server/src/db/jobs/job_integrity_authority.rs` | Modified | Set-based positive-page integrity authority reads |
| `server/src/db/jobs.rs` | Modified | Add `matches_page` and continuation response types |
| `server/src/api/jobs.rs`, `server/src/db/jobs/discovery.rs` | Modified | POST continuation, background repair, complete SQLite claim binding, aggregate metrics/tests |
| `server/src/db/mod.rs` | Modified | Register and parity-test both migrations |
| `infra/sqlite/server-runtime/061_jobs_workspace_pagination.sql` | Created | SQLite page/repair indexes and durable checkpoint |
| `infra/postgres/server-runtime/039_jobs_workspace_pagination.sql` | Created | PostgreSQL page/repair indexes and durable checkpoint |
| `jobs/portal/src/types.ts`, `api.ts`, `api.test.ts` | Modified | Strict page envelope, old-server fallback, API tests |
| `jobs/portal/src/App.tsx`, `App.test.ts` | Modified | Cursor plus workspace-authority-epoch install, de-duplication, invalid/expired recovery |
| `jobs/portal/src/views/MatchesView.tsx`, `styles.css` | Modified | Deterministic load-more and loaded-row scope truth |
| `jobs/portal/src/data/preview.ts` | Modified | Terminal preview page metadata |
| `CHANGELOG.md` | Modified | Unreleased product/security summary |
| Round 626, FIX-786, this IMPL, Phase 626 REVIEW | Created | Scope, cause, evidence, GREEN source/static review state, remaining executable gates |

## Implementation Notes

The continuation tuple is read from relational columns and ordered
`match_score DESC, updated_at_ms DESC, id ASC`. The encrypted cursor contains only that tuple plus
schema/purpose, account hash, page size, and issuance/expiry times. It is never logged or persisted
by the portal. The current Jobs data key authenticates it; rotation intentionally invalidates all
outstanding cursors, after which the portal refreshes the workspace.

Each page reads posting rows and discovery membership rows in bounded batches. The managed-source
release authority is read once. When inactive, every posting gets the existing unavailable,
Review-first composition without additional membership/head/ATS/integrity SQL. When a positive
source binding exists, current ATS and signed-integrity authority remain mandatory. The
ATS trust/head/canary/evidence/layout/runtime/quarantine/circuit reads and the signed-integrity
control/policy/head/attestation/transition/current-revocation reads are batched for the page under
the existing publication and lock order. ATS policy and policy-key revocation use bounded global
flags; non-global revocations are returned only for the exact, capped page subject set and stored
as keyed membership rather than repeatedly scanning a vector. Cryptographic, rollout, target,
canonical-source, and source-match evaluation is still performed for every record. Page authority
is not reusable by any external-effect endpoint.

The ATS loader rejects more than one 100-row scan plus look-ahead before constructing SQLite
placeholders or PostgreSQL arrays. Binding and child queries carry exact result ceilings; evidence,
layout, and runtime children must have contiguous zero-based ordinals and remain within their
signed 64/64/32 envelopes before any revocation query executes.

Source membership storage uses the relational posting UUID. Page composition now uses that UUID
for membership/head lookup and separately revalidates provider job ID and canonical key, matching
scalar authority. Page ATS joins require head, activation channel, and activation scope identity.
General and canary heads may share one manifest; identical manifest policy rows are coalesced while
conflicting policy identity still fails closed.

The continuation endpoint is authenticated POST with `{limit,cursor}` in its deny-unknown body and
returns matches and page metadata only with `private, no-store`. The workspace response is also
`private, no-store`, and both portal requests set `cache: "no-store"`. Continuation intentionally does not repeat
workspace-wide ancillary vectors (facts, applications, evidence, sessions, events, integrations,
identities, source summaries, or entitlement); the browser retains its current workspace snapshot
until its normal refresh. The workspace wire field is additive and older decoders can ignore it,
but its behavioral compatibility tradeoff is explicit: `/api/jobs/workspace` intentionally changes
from exhaustive/fail-above-500 behavior to first-page semantics. An older client sees at most the
first 50 workspace matches and cannot continue the traversal. The separate legacy
`GET /api/jobs/matches` contract does not silently adopt first-page semantics: it remains exhaustive
through 500 and returns an explicit pagination-required conflict above that envelope.

The ordered index cannot inspect encrypted availability inside the posting document. A request
therefore examines one bounded candidate window plus a look-ahead row. If that window is entirely
expired or otherwise non-interactive, the response can be short or empty with a valid continuation
cursor; it never walks an account's entire historical tail to fill a screen.

Workspace now returns the bounded first page. Legacy `GET /api/jobs/matches` remains exhaustive
through its prior 500-row envelope and returns an explicit pagination-required conflict above it;
it never silently truncates. The portal accepts older workspace responses without metadata as one
terminal bounded page.

Application history is not inferred from ranked matches. The first workspace response includes one
deduplicated `ApplicationJobContext` for each application-linked job outside the page. It contains
only strict display fields, prefers the immutable prepared receipt snapshot, and uses bounded
tenant-posting fallback for malformed legacy snapshots. It never supplies current URL or effect
authority. Queueing no longer pre-rejects page absence; handoff/interview paths load current exact
detail before use, and all server actions independently recompose authority.

Legacy verified-import repair is a separate 50-row background migration. A partial import-only
`(account_id,id)` index and durable per-account/version checkpoint provide restart-safe monotonic
progress. The scheduler acquires one owner/token/expiry lease before spawning work; a live SQLite
lease is rejected by a read-only fast path, while replacement after expiry is fenced by the secret
token digest. A zombie owner cannot reread the batch, advance the cursor, or release its
replacement. Compare-and-swap prevents cursor regression; retryable parse/decrypt/storage failures
release the lease without advancing, and the tenant relational row overrides stale embedded
posting identity. A completed checkpoint is constrained to have no lease fields.

## Build & Test

Observed locally on 2026-08-31:

```text
cargo +1.95.0 fmt --manifest-path server/Cargo.toml --all -- --check
  PASS

git diff --check
  PASS

isolated CARGO_TARGET_DIR + isolated TMPDIR
cargo +1.95.0 check --manifest-path server/Cargo.toml --lib
  PASS in 1m 01s

cargo test --manifest-path server/Cargo.toml --lib --offline \
  'db::jobs::ats_certification_page_tests::' -- --nocapture
  PASS: 6 passed, 0 failed; 50-target ATS page = 10 SELECTs;
  50-row fully positive source/ATS/integrity page = 19 SELECTs

cargo test --manifest-path server/Cargo.toml --lib --offline \
  sqlite_page_projection_binds_active_membership_by_posting_id_and_canonical_key
  PASS: 1 passed, 0 failed

cargo test --manifest-path server/Cargo.toml --lib --offline \
  public_lifecycles_resolve_one_production_positive_composition
  PASS: 1 passed, 0 failed

cargo test --manifest-path server/Cargo.toml --lib --offline \
  workspace_keeps_501_application_jobs_available_without_ranked_list_inflation
  PASS: 1 passed, 0 failed

cargo test --manifest-path server/Cargo.toml --lib --offline \
  durable_verified_import_repair_reaches_row_51_and_retains_failed_checkpoint
  PASS: 1 passed, 0 failed

cargo test --manifest-path server/Cargo.toml --lib --offline \
  verified_import_checkpoint_rejects_non_monotonic_transitions
  PASS: 1 passed, 0 failed

cargo test --manifest-path server/Cargo.toml --lib --offline \
  jobs_workspace_pagination_index_is_paired_and_runtime_ledgered
  PASS: 1 passed, 0 failed

npm run typecheck in jobs/portal
  PASS

npm test in jobs/portal
  PASS: 32 files, 382 tests
```

That evidence predates the final repair lease, exact-subject ATS revocation, workspace no-store,
the five earlier RED-blocker corrections, and the later entitlement/reservation correction. Per
the storage stop, no Rust command was started for the corrected source. Portal test and typecheck
executables remain absent from the retained `node_modules` tree, so no portal suite was started;
no dependency installation or network access was used. The post-correction Bluey ops docs, Jobs
client/server boundary, edge/crawler policy, and diff-hygiene guards passed. The exact manifest is
recorded in the handoff evidence, but exact corrected-source Rust, portal, and PostgreSQL execution
remain hosted/integrated gates.

Source fixtures added for hosted CI cover:

- 5,500 rows return 50 initially and traverse all 5,499 interactive rows without gaps/duplicates;
- tied score/time ordering and relational-ID authority;
- malformed, tampered, expired, foreign-account, and changed-page-size cursors;
- maximum page size, tenant isolation, terminal metadata, and SQLite index query plan;
- expired detail plus exhaustive 5,500-row export;
- one managed-source account authority read per page and independent action/detail composition;
- one bounded source-membership read and one bounded source-head read for an active page, with no
  per-posting source membership/head loop;
- complete scalar/page ATS JSON parity for signed general and canary authority, general-first and
  shared-manifest canary fallback, activation/trust expiry, wrong enrollment, revocation,
  quarantine, circuit, malformed targets, evidence/layout/runtime loss, and scope drift;
- exactly 10 ATS reads for 50 distinct fully positive targets and 19 total reads for 50 fully
  positive source-to-ATS-to-integrity page compositions;
- a rollback-only PostgreSQL first/deep `EXPLAIN` fixture for 5,500 tied score/time rows. Hosted
  review must retain plans proving the expression-order index range seek without a Sort or
  sequential scan;
- portal envelope strictness, old-server compatibility, stale-response rejection, and de-duplication.
- 501 application contexts without ranked-list inflation, frozen expired labels, malformed receipt
  fallback, continuation zero-context diagnostics, all affected portal labels, and exact action
  revalidation;
- durable import repair across rows 1-50 and 51, stale embedded-ID recovery, parse-failure
  checkpoint retention, monotonic CAS, single-flight owner/token/expiry fencing, live-lease
  read-only admission, and the partial-index query plan;
- high-cardinality irrelevant ATS revocation/trust-key history with zero non-page rows returned,
  bounded global flags, exact page-subject membership, and SQLite/PostgreSQL query-shape parity;
- private/no-store workspace response and browser request behavior.
- 102 fully source/ATS/integrity-positive export rows in SQLite and configured PostgreSQL, proving
  exhaustive traversal across the fixed 100/2 authority-composition boundary;
- first-owner SQLite repair claim binding plus the existing burst single-flight lease fixture;
- current Track, profile, and company-exclusion drift with zero balance, credit, allowance,
  entitlement, overage-ledger, or meter effect in both dialects;
- exact packet-meter replay before current policy revalidation; and
- PostgreSQL replay → global → Account → application/posting → entitlement → bounded-reservation
  static order, the configured Account-deletion race, and the configured entitlement-wait race
  that inserts a relevant reservation before wake-up and proves zero packet effects on rejection;
- portal continuation rejection across refresh and mutation authority epochs.

The focused 5,500-row runtime test, all newly added correction fixtures, configured PostgreSQL
tests, portal typecheck/Vitest, strict Clippy, and full exact-tip suites remain hosted review gates.
Per release-owner direction, a second uncached local build/test was not started on the constrained
disk.

## Deviations From Plan

| Deviation | Rationale |
|-----------|-----------|
| Portal dependencies installed only for focused verification | TypeScript and all Vitest tests ran locally; generated dependency state is removed before handoff. |
| Further Rust execution stopped after focused correction tests | The isolated Phase 626 target grew to approximately 14 GiB physical (28.1 GiB apparent). It was proven inactive and cleaned exactly with `cargo clean --target-dir /private/tmp/bluey-phase626-ats-check.rJ8Soi/target`. The measured residuals were 277 MiB in the temporary root and 172 KiB in its cargo-home, not empty; the 277 MiB temporary root was then moved to Trash. This base has no canonical launcher, so future Rust evidence must wait for integration into a successor with `scripts/run-bluey-tests.sh` (or an equivalently reviewed auto-cleanup launcher). |
| No local PostgreSQL execution | No isolated `BLUEY_TEST_POSTGRES_URL` was provided. Migration/source parity is covered statically; runtime parity remains mandatory. |

## Known Follow-Ups

- Hosted focused 5,500-row SQLite test plus configured PostgreSQL parity.
- Hosted execution of `server/tests/fixtures/jobs_workspace_postgres_explain.sql` against an
  isolated migrated PostgreSQL database, with retained first/deep plans and explicit rejection of
  sequential-scan or Sort nodes.
- Exact-tip server check, strict Clippy, focused/full tests, Jobs CI/privacy/schema guards, and
  production-shaped latency/query-count observation.
- Exact-source SQLite/configured-PostgreSQL 102-row positive export, packet current-effect drift,
  exact replay, repair first-claim, parent-Account deletion race, and entitlement/reservation
  contention-race execution.
- Reviewed migration plan, rollback evidence, deployment authorization, and live read-back before
  any limited-audience release.

## Review Checklist

- [ ] Cursor cryptography, TTL, tenant/page-size binding, and generic failure semantics reviewed
- [ ] Interactive inclusion/exclusion semantics match current product policy
- [ ] Search/filter/sort/count scope is never presented as exhaustive while more pages exist
- [ ] Detail, export, approval, queue, runner, and final-effect paths do not trust page state
- [ ] First packet effect recomposes current Track/profile/preferences/discovery and preserves
      replay-first, zero-effect denial in both dialects
- [ ] Exhaustive export crosses 101 rows through fixed-size authority-composition chunks
- [ ] PostgreSQL packet lock order is replay → globals → Account → application/posting →
      entitlement → bounded reservations, with no reservation read between application and
      entitlement
- [ ] Load-more installation requires both the current cursor and workspace authority epoch
- [ ] SQLite/PostgreSQL index and query shape reviewed against production plans
- [ ] Diagnostics contain counts/timings only and no user/job/cursor payload
- [ ] Hosted test gates above attached to the exact reviewed source
- [ ] No unrelated worktree, production state, flag, credential, or external effect changed
