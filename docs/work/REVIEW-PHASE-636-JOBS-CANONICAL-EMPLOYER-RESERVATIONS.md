# REVIEW: Phase 636 Jobs canonical employer reservations

Date: 2026-10-28 (America/New_York)
Base: `cd72989dc96cbab66c8fb4717c15200fb16828c0`
Reviewer role: independent read-only source/test review
Verdict: 🟡 conditional accept for bounded local Phase636 correctness; repository warning and external/release holds remain

## Scope reviewed

- `docs/rounds/ROUND-636-JOBS-CANONICAL-EMPLOYER-RESERVATIONS.md`.
- `docs/work/IMPL-PHASE-636-JOBS-CANONICAL-EMPLOYER-RESERVATIONS.md`.
- `docs/work/FIX-806-canonical-employer-reservation-alias.md` through FIX811, the final Jobs gate
  command, Phase636 handoff, completion ledger and changelog closeout.
- Current diff in `server/src/db/jobs/postgres_local_authority_tests.rs`, including the retained
  production-positive source/runtime authorities, approval helper and new PostgreSQL reproduction.
- Existing `reserve_application_attempt` SQLite/PostgreSQL admission sequence and the display-key
  reservation schema/lookup implicated by the test.
- Current correction diff in `server/src/db/jobs/attempt_employer_identity.rs`, `eligibility.rs`,
  `applications.rs`, `workflow_commands.rs`, `execution_authority.rs`,
  `execution_leases.rs`, `ats_certification_authority.rs`, `workspace.rs` and the associated model
  include in `jobs.rs`.
- Test-support corrections in `job_integrity_authority.rs`,
  `production_positive_authority_fixture.rs`, `tests.rs` and the API error mapping in
  `server/src/api/jobs.rs`.

No product, test, migration or harness source was edited by this reviewer. No Cargo command,
PostgreSQL service, provider, production account or external action was started.

## Expected-red fixture review

The original reviewed test blob was
`50ad01bca4d780afa4d1e7c6e97a3c83a9240510`. Two fixture-only corrections were required before a
causal red was obtained; the accepted red fixture blob is
`ef291fceaf70ef0e7bb91c5a822abab96ee04f49`.

- The fixture starts from the existing production-positive source verifier, ATS surface and signed
  job-integrity helpers. It creates two different Greenhouse board/job URLs, two approved Tracks
  and two verified application identities. The display companies normalize differently.
- Both applications are prepared in ordinary `factual`/`review_first` mode and persisted through
  the normal current-approval authority/checksum path. The frozen receipts are required to name
  different application identities rather than merely changing fixture variables.
- After reserving application A, the test re-resolves both composed authorities. Both must remain
  `Verified`, each frozen application must exactly match its current authority, and both signed
  authorities must resolve to the same canonical employer ID and domain. This prevents stale
  source, Track, receipt, ATS or fabricated-integrity rejection from masquerading as the desired
  employer-duplicate denial.
- Application B admission must fail specifically with the existing exact company-duplicate text or
  a typed `company_application_exists`/`canonical_employer_application_exists` reason. An unrelated
  entitlement, daily-limit, source, Track or runner error cannot make the test green.
- Before the losing admission, the test snapshots all attempt reservations, account balance,
  entitlement packet usage, credit remainder, B-specific metering/ledger/generation allowance and
  the complete B application. A passing denial must leave each snapshot unchanged.

The current-product outcome is to reserve B successfully and fail at the explicit
`result.is_err()` assertion because only normalized display `company_key` is checked. Accepted
red3 reached exactly that assertion.

## Failed fixture attempts and causal red resolution

Parent-reported canonical session 45270 exited one, but application A failed at its first
`reserve_application_attempt` with typed reason `current_execution_authority_denied`. No alias
duplicate comparison or B admission occurred. PostgreSQL stopped cleanly; final root cleanup was
verified afterward. This was correctly recorded as an invalid fixture, not an alias reproduction.

Session 44231, fixture blob `432502900b82ed251e08235cf61b0f4a8b56d2d0`, added explicit
`full_time w2`, ordinary queue-decision assertions and both full current-execution checks. It exited
one at `career_track_policy_review_required`: adding the second verified identity advanced account
input generation, so the original Track/packet authority was stale. PostgreSQL stopped cleanly and
the root was removed. This also was not an alias reproduction.

Session 81168, frozen fixture blob `ef291fceaf70ef0e7bb91c5a822abab96ee04f49`, used normal review
of both Tracks after final account setup, then factually reprepared and ordinarily approved both
applications. Before reserving A it required both applications to pass complete current-execution
authority under the existing `H -> M -> ATS -> D` lock order. A reserved successfully. Immediately
before B admission, both composed projections were `Verified`, both frozen receipts matched their
current projections, signed canonical employer IDs/domains were equal, display keys were distinct
and the losing-effect snapshots were captured.

Red3 exited one only because B was incorrectly reserved and the assertion
`same signed canonical employer aliases must not reserve twice` failed. PostgreSQL stopped cleanly;
the exact root was absent and its parent empty. This is the accepted causal expected-red evidence
for the P1 product defect. No production gate was bypassed or weakened.

## Schema/parity batch review

Reviewed independently and read-only:

- `infra/sqlite/server-runtime/066_jobs_attempt_employer_identity.sql`.
- `infra/postgres/server-runtime/044_jobs_attempt_employer_identity.sql`.
- The Phase 636 migration registration, SQLite additive-column helper and focused migration tests
  in `server/src/db/mod.rs`.
- The Phase 636 additions to `jobs/scripts/check-jobs-schema-parity.mjs` and
  `jobs/scripts/ci-guards-self-test.mjs`.

The migrations are additive and do not edit committed v3 migration bytes. All three binding fields
are nullable together for historical rows and complete together for bound rows. New active rows
require a complete binding; partial bindings fail. Canonical employer ID and domain each have an
account-scoped partial unique index over `reserved`, `running`, `side_effect_unknown` and
`submitted`, so either signed identity coordinate closes the display-alias race. Bound ID/domain
are immutable. Attestation rotation is limited to `released -> reserved` with unchanged canonical
identity, preserving policy refresh without rewriting attempt identity.

Legacy all-null rows are not guessed, deleted or rewritten. Updates to `side_effect_unknown`,
`submitted` and `released` remain possible, while `reserved`/`running` reentry is denied until a
complete binding is supplied. That preserves terminal recording, reconciliation and safe release
without admitting new unbound work. SQLite replay is idempotent through nullable `ensure_column`
before migration 066; PostgreSQL 044 uses `IF NOT EXISTS` columns and replayable function,
constraint, index and trigger definitions. These were static conclusions at the first schema
checkpoint; the final evidence section records later SQLite and real-PostgreSQL execution.

### Resolved P3 parity-guard gap

The first reviewed parity guard did not structurally pin SQLite's required domain dot or its
text/BLOB byte-length equality, and its Rust negatives did not include a non-localhost single-label
host. The current bytes now require both exact SQLite fragments, mutate each independently in the
CI self-test, and include `intranet` in the migration negatives. Parent-reported static execution
then passed parity for 114 tables/95 indexes and the CI-guard self-test. This resolves the reported
P3 source gap; it is not a substitute for real migration execution.

## Current core static review

- `frozen_attempt_employer_binding` reads only the authenticated application job-integrity receipt.
  Both reservation writers and both cloud-staging writers call it only inside the transaction in
  which complete current signed/frozen source and execution authority has already been resolved.
  Mutable display text and posting email never become canonical employer authority.
- SQLite's immediate transaction and PostgreSQL's existing Account/entitlement serialization enclose
  the relevant-row check and write. PostgreSQL adds no cross-application row lock in the new lookup;
  the two partial unique indexes remain the final concurrent ID/domain fence. The established
  account/entitlement/attempt order is retained.
- First admission is not circular: current authority may resolve before an owned reservation exists.
  Once an active owned reservation exists, current execution and packet validation require its full
  ID/domain/attestation binding to equal the frozen receipt. Released history may refresh only the
  attestation for the same canonical ID/domain; an active policy/trust rotation fails closed until
  safe release and readmission.
- Packet validation reads the owned row, display aliases, canonical ID/domain matches, mapping
  conflicts and any active unbound legacy row before a new metering effect. An already committed
  metering replay remains read-only. Cloud staging performs employer admission before packet
  metering in the same transaction.
- Local/cloud execution authority inherits the owned-binding check. The final ATS Phase B context
  separately requires the exact stored binding before deriving its unchanged legacy metering hash.
  No new binding field was added to that historical hash contract.
- Terminal `click_started` replay, checkpoint reconciliation and status-only terminal/release paths
  were not placed behind a fresh-current-authority check. Legacy null rows can still record
  `side_effect_unknown`, `submitted` or `released`, while they cannot re-enter `reserved`/`running`.
  This preserves outcome recording without admitting new unsigned work.
- PostgreSQL posting-representation drift detection now includes all three binding values in its
  mutable-input fingerprint. All changed row readers reviewed use the same 11-column projection.
- A repository search found no frozen receipt, checkpoint, request HMAC or protocol hash that
  serializes the whole `AttemptReservation`. Its only general serialization surface is the additive
  account export/workspace model; `serde(default)` keeps legacy JSON readable, while new exports
  expose the three nullable/current fields. ATS metering continues to hash only its established
  six-field reservation projection, with the exact binding checked separately first.

No product-authority bypass was found in this static pass. Concurrency, live migration behavior,
actual Phase B consumption, binding drift, released rotation and legacy terminal availability were
then executable-proof requirements; the final evidence section records their current qualification.

## Resolved P2 test-gate finding: direct active-row fixtures

The first correction snapshot left five pre-existing tests inserting `reserved`/`running` rows
without the three new binding columns. SQLite066/PostgreSQL044 deliberately reject those inserts
with `active attempt requires employer identity binding`, so that snapshot could not support a
full-suite claim:

- `server/src/db/jobs/postgres_local_authority_tests.rs:984-997`;
- `server/src/db/jobs/tests.rs:14953-14965`;
- `server/src/db/jobs/workspace.rs:3537-3542`;
- `server/src/db/jobs/ats_certification_authority.rs:15318-15331` for invocations using an active
  status; and
- `server/src/db/jobs/ats_certification_authority.rs:20394-20400`.

The current source resolves this finding without weakening the production trigger. The packet-race
and cloud-context fixtures now use the exact signed/frozen application binding; the pure quota,
workspace ABA and ATS-capacity fixtures use explicit well-shaped synthetic bindings scoped to the
authority each test actually exercises. Historical unbound-state coverage remains isolated to
pre-066/replay fixtures, and the migration negatives still prove that a new unbound active insert
is rejected. This was a test-evidence blocker, not a production bypass; it was statically resolved
there and subsequently exercised by the final-source gates below.

The first post-fix focused session 81433 exited one at compile discovery with Rust E0753 because
the newly included file began with inner `//!` comments. It executed zero tests. PostgreSQL stopped
cleanly, the exact root was removed and all frozen source hashes matched. Parent changed only those
three introductory lines to ordinary comments after terminal.

Parent-reported replacement session 31836 then exited zero on its unchanged 19-file snapshot:
private PostgreSQL alias1/1, SQLite/core8/8, migration3/3, registration1/1 and strict all-target
Clippy passed; PostgreSQL stopped cleanly and the owned root was removed. That is valid preliminary
evidence for that snapshot. It predates the later irreversible-state guards, corrected fixtures,
same-job terminal-history fix and their new tests, so it is not final-source green.

## Resolved P1 static finding: same-job terminal history could be erased

The first terminal-history implementation excluded the current application ID in
`prepare_application_inner`. Eligibility likewise excludes the owned reservation. The same legacy
`submitted` or `side_effect_unknown` application could therefore be reused, reset to `preparing`
and CAS-updated to a new prepared packet when it had no reservation or only a released slot. That
would erase the retained terminal evidence and permit a later second reservation for the same
employer. This was a product safety blocker, not only a missing test.

The current source closes the path at both phases of preparation:

- before tailoring, parsed `submitted`/`side_effect_unknown` state and any retained
  `submitted_at_ms` are rejected with typed `legacy_company_identity_review_required`;
- new preparation calls the complete-terminal-history query with no current-row exclusion;
- finalization rechecks the database-column `ExpectedApplicationRevision.state` before inserting
  any evidence/resume rows, while the existing exact state/timestamp/payload CAS rejects a
  concurrent terminal transition and rolls back the transaction.

SQLite tests retain ordinary nonterminal reprepare and snapshot application, resumes, attempts and
packet effects for both terminal states. PostgreSQL adds the same positive and negative coverage
for both an absent attempt and a retained released slot. The helper's current-ID exclusion remains
available to reconciliation/current-execution callers that are recording an outcome; preparation
no longer uses that exception. This P1 was statically resolved and the final evidence below records
execution of the corrected source and regressions.

## Irreversible state and reconciliation review

The refreshed SQLite update trigger and PostgreSQL trigger make `submitted` reservation state
irreversible and permit `side_effect_unknown` only to remain unknown or reconcile to `submitted` or
`released`. They still permit status-independent metadata updates and account/application cascade
deletion. A legacy unbound running row may record unknown/submitted/released, but cannot re-enter
reserved/running; an unknown row may be safely released and a released row may renew only with a
complete binding. The Rust status writer mirrors these transitions before issuing SQL.

The existing no-submission reconciliation moves only `side_effect_unknown -> released`; trusted
submission recording moves unknown/running to submitted; local failure and packet-revision cleanup
operate on running/reserved rows. No reviewed legitimate reconciliation path requires
`submitted -> released`. The two-connection SQLite race and state-transition tests are structurally
meaningful; the final evidence below records their current-source execution together with
PostgreSQL044 and the expanded regressions.

## Correction invariants to preserve

- Keep the existing display-name guard and add signed canonical employer ID/domain as independent
  reservation authority; never infer identity from mutable display text, email or receipt JSON.
- New admission and packet/pre-click execution must fail closed when the binding is absent, stale or
  mismatched. Own-reservation replay must prove exact current binding rather than bypassing the
  check merely because the application ID matches.
- Legacy unresolved rows remain intact. Terminal/submitted/side-effect-unknown recording,
  reconciliation and explicitly authorized safe release must remain available; a new admission
  fence must not strand irreversible outcomes.
- Additive SQLite 066/PostgreSQL 044 migrations must preserve committed historical migration bytes,
  existing v3 release/schema/metering hashes and old release/head authority. Concurrency must be
  enforced by database constraints in both backends, not only a pre-insert query.
- Green verification still needs distinct-employer allowance, same-name display protection,
  policy/trust rotation, canonical binding drift, exact replay, legacy migration/replay and real
  PostgreSQL concurrent admission tests.

## Executable evidence required at the static checkpoint

- Obtain final-source focused SQLite/PostgreSQL green evidence for migrations, both backend races,
  binding drift, same-job terminal preservation, API error mapping and reconciliation transitions.
- Full formatting, strict Clippy, all-target Rust, cleanup/self-test/privacy and final source review.

### Resolved P2 test-evidence finding: signed quota fallback is caller-explicit

The repaired daily-quota fixture now publishes ordinary positive source, integrity and ATS
authorities and uses `full_time w2`, distinct signed employer domains and the exact frozen binding
when the application contains a job-integrity receipt. Its reservation count and duplicate-company
assertions remain meaningful. In the first repair snapshot, however,
`seed_active_reservation_for_quota_eligibility` selects its synthetic name-only binding whenever
`application_job_integrity_receipt` returns `None`; the caller does not explicitly authorize that
fallback. The same helper serves both the old unsigned name-guard case and the new signed quota
case. Consequently, a regression that causes the signed quota preparation to omit its integrity
receipt can silently seed synthetic identity and still reach/pass the daily-limit assertion.

The current source resolves the gap with an explicit `allow_unsigned_name_only` argument. The sole
legacy name-guard caller passes `true` and first asserts that its receipt is absent. Both signed
quota seed paths pass `false` and unconditionally require `frozen_attempt_employer_binding`, so an
absent, malformed or invalid signed receipt fails instead of synthesizing identity. Repository
search found no fourth caller. This statically resolved the test false-green risk without changing
production authority behavior; later current-source execution is recorded below.

The first combined run on this repair, session21814, was deliberately stopped by the parent during
compile/discovery before tests executed so this review finding could be corrected. It exited143;
PostgreSQL stopped cleanly, the exact root and owned processes were absent, and all23 frozen hashes
were unchanged. It is cleanup evidence only, not a pass or failure of the repaired tests.

Parent-reported replacement session87312 then executed all38 selected private-PostgreSQL cases and
20 focused core/SQLite/API cases successfully, but exited one when the daily-quota fixture reached
its distinct-company loop. The signed integrity test constructor still hard-coded
`employer-acme`, so varying only the domain correctly produced
`canonical_employer_mapping_conflict`. PostgreSQL stopped cleanly, the exact root and parent were
empty, no owned process remained and all23 hashes were unchanged. Strict Clippy and the full suite
did not run; this is useful causal failure evidence, not a green gate.

The current test-only correction passes canonical employer ID explicitly into the signed
attestation constructor. Existing fixture entry points still supply the exact old
`employer-acme` value; a new `_for_employer` wrapper is used only by the distinct-company quota
loop and final Globex posting. The loop IDs/domains are valid and mutually distinct, while the two
Acme postings retain the same default ID/domain and continue to prove company duplication. The
verified-source helper has exactly its two controlled callers, and the low-level signed installer
has one call site, so no prior fixture silently changes identity. The reservation helper continues
to require the frozen signed receipt for these paths. This was statically coherent, test-only and
did not weaken production or admit unsigned authority; later execution is recorded below.

Parent-reported session39121 subsequently passed all38 selected private-PostgreSQL cases and20
focused core/SQLite/API cases, then exited one later in the quota fixture because twelve distinct
Greenhouse boards exceeded the independent Career-Track source-enrollment cap. PostgreSQL stopped
cleanly, the exact root and parent were empty, no owned process remained and all25 hashes were
unchanged. This again is a causal fixture failure; full Rust and Clippy did not run.

The current quota fixture uses one explicit synthetic multi-employer Greenhouse board with twelve
distinct exact job paths, external IDs and canonical job keys. Discovery therefore owns one source
without collapsing job subjects. The same managed runtime/source authority is correctly reused,
while every exact job still receives ordinary source, ATS and integrity publication plus its
explicit signed canonical employer ID/domain. The two Acme jobs continue to share employer
identity, the nine additional stored attempts remain distinct, and Globex remains the next
distinct-employer candidate that must be denied by the ten-per-day limit. This statically removes
only the unrelated source-cap collision; source limits and production authority are unchanged. The
corrected quota and full-suite results are recorded below.

### Full-suite session16096 and resolved source-contract/cloud-fixture failures

Parent-reported session16096 reached terminal exit one with1726 passes and six failures in
2502.74seconds; no later target remained. One failure was the source-contract assertion that still
required the pre-Phase636 `LIMIT 1`. The other five all failed during fixture setup with conflicting
employer-identity records: the generic cloud lease fixture signed every suffix as
`employer-acme` while assigning a different suffix-derived canonical domain. The affected tests
were the shared-profile lease exclusion, exact cloud/local final-submission authority, stale and
terminal receipt authority, safe terminal protected-capacity release and submitted/unknown
protected-capacity retention. These failures occurred before the intended lease/receipt assertions.
All25 source hashes and HEAD remained unchanged; the exact root and parent were empty and the
observed launcher/PostgreSQL PIDs were absent before corrections.

The current source-contract test now pins the actual bounded query invariants in both dialects:
the eleven-column reservation projection, owned-application inclusion, active blocking peer,
canonical ID/domain and legacy-null predicates, owned-first `LIMIT 2`, distinct daily-period query,
daily-limit clamp and both PostgreSQL `FOR SHARE` bounds. It retains the full-account-query
negatives. It no longer requires obsolete SQL shape and does not relax production behavior.

The current test-only fixture correction adds a runner/employer wrapper while keeping the runtime
surface builder private. Every existing runner wrapper and caller retains the exact
`employer-acme` default. Only the generic cloud execution-lease fixture supplies a stable distinct
ID derived from its already-distinct Greenhouse tenant. The local authority helper remains
`employer-acme`/`acme.com`, the runner-volume-purge caller remains on the default, and browser
profile identity remains shared because it derives from account/application identity rather than
employer identity. The affected tests retain their exact conflict, release, higher-fence,
receipt-authority and capacity assertions. Their six exact filters are sufficient targeted
regressions before the planned combined/full gate; no extra narrow case is required statically.

### Focused session 78617: pre-existing ATS serializable retry defect

Parent-reported focused session 78617 exited one after seven of eight selected private-PostgreSQL
cases passed, including all five new canonical-employer cases, the existing entitlement reread race
and the PostgreSQL workspace snapshot. The existing
`postgres_concurrent_canary_phase_b_enforces_signed_concurrency_cap_atomically` case failed because
the losing transaction returned `Storage` containing genuine PostgreSQL SQLSTATE `40001`
serialization-pivot evidence instead of rereading the winner and returning typed
`CapacityUnavailable`. PostgreSQL then stopped cleanly; the exact root and owned process were gone.

This is a latent transaction-wrapper defect, not evidence that 044's canonical indexes weakened the
capacity rule. The standalone pool wrapper starts `SERIALIZABLE` and then calls the transaction
advisory-lock `SELECT`. A waiter can establish its serializable snapshot before blocking on the
winner, retain a pre-winner capacity snapshot after acquiring the lock, and be aborted by SSI. The
Phase 636 diff does not change that wrapper or the capacity query. Its new attempt indexes are on
canonical employer ID/domain, cannot serve the capacity join on attempt primary key, and Phase B
does not write attempt rows; they may change timing, but do not create this stale-snapshot pattern.

The implemented retry uses the narrow correct boundary: the standalone pool-owning wrapper, not
the caller-owned `_tx_after_prelock` primitive. It preserves `SERIALIZABLE` and retries at most
three complete local database transactions only when `Storage` contains a real
`postgres::Error` whose code is `T_R_SERIALIZATION_FAILURE`. Each closure invocation starts a new
transaction, reacquires the ATS transaction advisory lock, executes the complete Phase B body,
commits and converts the outcome. An error-returning invocation drops the transaction before the
next invocation. The retry therefore cannot reuse the aborted snapshot. Text containing `40001`,
another SQLSTATE, a domain denial and an ambiguous connection/commit text error are not retried.
The caller-owned context/after-prelock APIs remain unchanged. This statically resolves the product
retry-boundary finding without broadening retries to an external/provider/object effect.

The existing concurrent test retains one authorization, one exact typed capacity loser, one stored
reservation and unchanged losing-binding assertions. The new generated-driver-error test proves
that a real `40001` is retried exactly three times and that `23505` is returned after one attempt;
the pure negative similarly proves that error text and domain errors do not retry. The generated
`40001` test's second-attempt branch directly returns `CapacityUnavailable`; by itself it does not
invoke the production capacity query. The current concurrent canary test closes that timing gap
structurally. A separate transaction holds the real ATS transaction advisory lock, both
serializable contenders are started, and an observer requires two distinct ungranted backend PIDs
waiting on the exact held advisory-lock identity before releasing the blocker. Consequently both
contenders' first snapshots predate either contender's write. The blocker is committed before
joining either thread, so the test does not deadlock on its own lock; the ten-second observation
bound releases the lock even when the proof condition is not met and then fails the assertion. One
contender can return the required typed capacity denial only after the genuine `40001` retry gets a
fresh snapshot. The existing winner, loser, stored-reservation and unchanged-losing-binding
assertions remain intact. This statically closed the prior P3 timing gap, and session88910 later
executed both the deterministic race and typed retry cases. The direct generated-error test should
still not be described as independently performing the capacity reread. Session78617 remains the
historical pre-fix failure.

The current PostgreSQL044 test also executes the actual migration body twice inside a generated
transaction-local schema. Its generated schema name is identifier-safe, `SET LOCAL search_path`
cannot survive the transaction, and rollback removes the schema even though the pooled connection
is reused. It covers nullable legacy submitted/unknown/reserved rows, denied legacy reentry,
allowed unknown reconciliation, rejection of new unbound active rows, bound identity
immutability, released attestation refresh and byte-for-byte replay stability. The PostgreSQL
terminal-history test now attempts both the same job and the different canonical alias job; the
latter forces the cross-application terminal-history query instead of passing only through the
same-application state guard. These additions were statically sound in that snapshot and were
subsequently exercised by the final private-PostgreSQL/current-source gates below.

## Final source qualification and closeout review

Runtime results in this section are parent-reported evidence inspected against the current source,
controllers, IMPL, ledger and handoff. This reviewer did not independently rerun Cargo, PostgreSQL,
Node, Chromium or the privacy scanner.

- Session88910 exited zero on the final product/test source:38 actual private-PostgreSQL cases in
  fresh databases,30 focused cases, formatting and strict all-target Clippy passed. The all-target
  Rust result was1872 passed,0 failed/ignored:1732 library,1 binary,134 integration and5 guarded
  examples. The25-file controller manifest, HEAD and scoped status matched internally and in a
  separate parent check; PostgreSQL stopped, the exact root/parent were clean and observed PIDs
  were absent. Configured SQLite early returns are not counted as additional actual-PG evidence.
- The six exact FIX808/809 regressions and the quota preflight passed before the broader gates. The
  deterministic ATS advisory-waiter race and typed40001 retry were included in the actual-PG set.
  This supplies executable coverage for the previously static migration/replay, alias race,
  binding drift, immutable history, legacy reconciliation, packet/execution and retry findings.
- Timeout qualification48144 observed the intentionally failing inner controller exit137 within
  8352ms, then confirmed controller/child/grandchild PIDs and the exact root absent. Fresh launcher
  self-test26934 also passed its intentional negative and residue cases.
- Workspace14411 actually passed all2071 tests but failed only its TAP-only summary parser; it is
  retained as a controller failure, not a workspace green. FIX811 added strict TAP/spec parsing.
  Successor55179 exited zero with Node8/7/17 and workspace804/219/308/321/387 tests,2071 total,
  zero skips, all five typechecks/builds, schema parity114 tables/95 indexes and CI-guard self-tests.
  Portal output stayed private with config/dotenv discovery disabled. Before cleanup, exact private
  Chromium process and open-file inventories were zero. All760 scoped tracked/nonignored-untracked
  hashes, HEAD and status matched internally and afterward; root/parent/process cleanup passed.
- Privacy73988 passed2871 indexed paths/2595 text files with all17 owned new files, inherited
  REVIEW628 excluded, the real index unchanged and its private scratch root removed. Because the
  closeout documents and this review changed afterward, a final rescan is still required before
  staging/commit; the dated pass must not be relabeled as post-review evidence.

Late prospective-index inspection found one extra blank EOF line in each new migration, SQLite066
and PostgreSQL044. The checkpoint was aborted before commit. Parent removed exactly one trailing
newline from each file and verified the transformation against the staged pre-format bytes; no SQL
token or historical migration changed. The resulting parent-reported SHA-256 values are
`eb40814a4b6c720d0c17b19ed41d03ec13626f4f117a4a292566fc43560ba3ea` for SQLite066 and
`595f528a06198f6efe8378cca7a7fbbb0685c87e3e9f720c99029903bd07c830` for PostgreSQL044. Fresh
parity114/95 and CI guards passed, but sessions88910 and55179 qualify the pre-format snapshots, not
these exact current bytes. A fresh final-source gate after the planned offline-font/bundle-warning
corrections, plus prospective-index whitespace and privacy checks covering all18 owned new files,
is therefore mandatory before any checkpoint or stronger verdict.

No P0-P3 source-correctness or security finding remains in the Phase636 canonical-employer/ATS
retry diff. The source changes preserve the existing display guard, add signed canonical ID/domain
uniqueness in both backends, keep terminal/reconciliation recording available, fail closed on
legacy or drifted bindings and do not change frozen v3 release/metering bytes.

## Separate warning and release holds

- **P2 release-quality hold — PDF.js offline standard fonts.** Parent-observed Jobs output warns on
  the standard-font path used by production `assertUsablePdf` at
  `jobs/automation/src/documents.ts:533` and independently by
  `jobs/automation/tests/documents.test.ts:421`. Passing extraction does not prove offline standard
  font fidelity. Supply a package-owned, bounded `standardFontDataUrl`/asset path and a no-network
  regression that exercises a representative standard-font PDF. Do not suppress the warning.
- **P3 performance hold — portal chunk ownership unmeasured.** The private portal build emitted a
  503.99kB minified/131.61kB-gzip chunk plus a346.21kB/99.74kB-gzip index chunk. The current record
  does not prove which importer owns the larger chunk or that it is initial-load work. Map the
  importer/route and measure before choosing a split or claiming latency improvement.

These warnings predate or sit outside the canonical-employer correction, so they do not invalidate
its bounded correctness evidence. They do prevent an unconditional warning-clean verdict under the
repository rules and remain release holds. No manual UI QA, independent runtime rerun, hosted TLS,
provider/storage/model/OAuth, real ATS/employer submission, native Linux/x86_64 runner, signing,
capacity/SLO or production deployment was proven by Phase636.

## Current verdict and limits

🟡 **CONDITIONAL ACCEPT — BOUNDED LOCAL PHASE636 CORRECTNESS ACCEPTED; WARNING/RELEASE HOLDS
REMAIN.** The causal red, dual-backend correction, irreversible-history fix and bounded ATS retry
are supported by static review plus the parent-reported pre-format Rust, actual-PG, workspace,
cleanup and source-identity evidence above. No Phase636 logic blocker is identified, but the exact
current bytes still require the disclosed fresh final-source gate. The post-review privacy and
prospective-index checks remain procedural acceptance work, and the PDF.js/font warning keeps this
from being an unconditional warning-clean repository verdict; the unattributed portal chunk remains
a P3 performance follow-up.

This is not manual QA, independent rerun evidence, production readiness, merge/release approval or
completion of the broader Jobs objective. The historical failed sessions remain diagnostic record,
not green evidence.

This does not authorize production submission, deployment, provider traffic, real employer action
or any relaxation of source, ATS, receipt, reconciliation, migration or release authority.
