# IMPL: PHASE-614 — Jobs Original-Source Verification Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against the current
> repository, Round 614, and the reviewed Phase 613 tip. The SSD archive was not used.

**Status:** Focused local source accepted; final aggregate and external evidence remain conditional

**Base commit:** `825947dc97feaf5c3b2698e7931cb301b9a3bda4`

**Branch:** `feat/phase-614-jobs-original-source-verification-authority`

## Scope

**Does:**

- implement the closed `original_source_verifier` worker entrypoint, private API, exact worker
  scope, and release/runtime identity;
- add paired fenced, database-time assignment and heartbeat authority with immutable attempts,
  events, observations, receipts, transitions, and a monotonic compare-and-swap current head;
- perform anonymous, semantically read-only verification for Greenhouse, Lever, Ashby,
  SmartRecruiters, and Workday under one closed provider/subject protocol;
- derive positive compatibility projection only from the current immutable receipt/head and a
  still-consumable assignment;
- reconcile one fail-closed verifier status/result vocabulary across TypeScript, Rust, API, and
  paired storage constraints;
- add versioned managed-release v2 authority for the exact verifier capability, protocol,
  entrypoint, runtime identity, and readiness role while preserving Phase 611 v1 rejection;
- recheck current source and existing managed/ATS/Track/identity/resume/risk/hold/entitlement
  authority at execution-capable preparation, transactional queue/running admission, and final
  effect boundaries; and
- impose the PostgreSQL cross-authority lock order `H -> M -> ATS -> D` on application save/queue
  and final-effect paths.

**Does NOT:**

- enable a current activation, direct/global discovery, Browser distribution, workflow dispatch,
  model generation, mailbox, communication, or any provider write;
- use credentials, cookies, OAuth, login, CAPTCHA bypass, private sessions, form filling, or
  authenticated provider actions;
- manufacture independent employer-identity or scam-risk clearance from provider presence;
- deploy, run live provider canaries, change a customer cohort, or claim production readiness; or
- implement Round 615 source enrollment, rights, cadence, budgets, scheduling fleet, SLOs, broad
  retention, or operational rollout.

## Implemented Authority Decisions

- Phase 611 release v1 stays fail closed for source verification. Version 2 is a separate closed
  contract and can be represented only by exact signed local fixtures in this batch.
- `original_source_verifier` is bound to `jobs-workflows`, its measured
  `original-source-verifier.js` entrypoint, protocol set, runtime grant/instance/heartbeat chain,
  and account-scoped current managed authority.
- Assignment lifecycle and receipt verdict remain separate. Lease acquisition, heartbeat,
  retry timing, and publication use database time; provider I/O happens outside database locks.
- First terminal publication rechecks assignment, fence, current release/runtime/source/hold
  authority, provider subject, observation tuple, and replay identity before accepting immutable
  evidence.
- Authenticated byte-identical response-loss replay returns the already committed result even after
  later authority loss, without reminting freshness or advancing the head. Changed-byte replay
  appends a quarantine event and makes the assignment non-consumable; projection then rejects any
  old positive head because a positive projection requires the assignment to remain `idle`.
- Only `verified_open` is potentially positive. Every closed, mismatch, changed, unsupported,
  unreachable, rate-limited, authentication/challenge, parser, or unknown result is non-authority.
- Production callers cannot use the positive convenience constructor. Explicit positive setup is
  confined to `cfg(test)` fixtures, while mutable/imported positive-looking fields are sanitized
  and cannot mint receipt authority.
- A hosted-ATS snapshot remains sufficient only for Review-first preparation. Phase 614 contains no
  relational authority that independently clears employer identity or scam risk, so the complete
  current path intentionally denies approval, queueing, and effects until a successor supplies
  that separate authority.
- PostgreSQL application save/queue and final-effect paths acquire operational holds, managed
  release, ATS certification, then discovery/application locks (`H -> M -> ATS -> D`).
  Managed-registry readers take a shared fence; writers remain exclusive.
- Reservation/running-status transitions currently use `H -> M -> D` and recheck
  original-source/discovery authority; independent ATS/integrity composition is parked for Phase
  614B and is not claimed by the reservation regression. Later claim/final-effect gates prevent an
  external effect, but a stale reservation may still consume capacity.
- Managed-cloud claim/final Submit begin with the combined
  `H -> exclusive M -> ATS -> fleet` prelock; unmanaged paths use
  `H -> shared M -> ATS`; both branches acquire account `D` afterward.
- Certified ATS fixtures use provider-verification scope, and provider evidence remains
  preparation-only without independent Phase 614B employer/risk authority.
- Auto-submit drafts persist as `awaiting_review` with pending approval and can become `queued` only
  in the transaction that attaches the exact `approved_execution` snapshot and revalidates current
  authority.

## Exact Changed-File Scope At This Checkpoint

This is the complete 57-file Phase 614 source/documentation scope at the late-fix
checkpoint.

### Schema, Migration Registration, And Guards

- `infra/sqlite/server-runtime/057_jobs_original_source_verification_authority.sql`
- `infra/postgres/server-runtime/035_jobs_original_source_verification_authority.sql`
- `server/src/db/mod.rs`
- `jobs/scripts/check-jobs-schema-parity.mjs`
- `jobs/scripts/ci-guards-self-test.mjs`

### Rust Verifier, API, Projection, And Effect Boundaries

- `server/src/api/jobs_original_source_verifications.rs`
- `server/src/api/jobs_operations.rs`
- `server/src/api/jobs_worker_auth.rs`
- `server/src/api/mod.rs`
- `server/src/db/jobs/original_source_verification.rs`
- `server/src/db/jobs.rs`
- `server/src/db/jobs/applications.rs`
- `server/src/db/jobs/discovery.rs`
- `server/src/db/jobs/eligibility.rs`
- `server/src/db/jobs/execution_authority.rs`
- `server/src/db/jobs/execution_leases.rs`
- `server/src/db/jobs/local_runner.rs`
- `server/src/db/jobs/managed_cloud_release_authority.rs`
- `server/src/db/jobs/operational_holds.rs`
- `server/src/db/jobs/runner_volume_purge.rs`
- `server/src/db/jobs/tests.rs`
- `server/tests/integration_e2e.rs`
- `server/tests/jobs_runner_plan_matrix.rs`

### Automation, Workflow Runtime, And Managed Release

- `jobs/automation/package.json`
- `jobs/automation/src/index.ts`
- `jobs/automation/src/managed-cloud-runtime.ts`
- `jobs/automation/src/original-source-verification.ts`
- `jobs/automation/src/public-ats.ts`
- `jobs/automation/src/worker-auth.ts`
- `jobs/automation/tests/managed-cloud-runtime.test.ts`
- `jobs/automation/tests/original-source-verification.test.ts`
- `jobs/automation/tests/worker-auth.test.ts`
- `jobs/workflows/Dockerfile`
- `jobs/workflows/package.json`
- `jobs/workflows/src/original-source-verification-api.ts`
- `jobs/workflows/src/original-source-verification-runtime.ts`
- `jobs/workflows/src/original-source-verifier.ts`
- `jobs/workflows/tests/original-source-verification-runtime.test.ts`
- `jobs/scripts/managed-cloud-release-gate.mjs`
- `jobs/scripts/managed-cloud-release-gate.test.mjs`

### Audit Record

- `docs/rounds/ROUND-614-JOBS-ORIGINAL-SOURCE-VERIFICATION-AUTHORITY.md`
- `docs/work/IMPL-PHASE-614-JOBS-ORIGINAL-SOURCE-VERIFICATION-AUTHORITY.md`
- `docs/work/REVIEW-PHASE-614-JOBS-ORIGINAL-SOURCE-VERIFICATION-AUTHORITY.md`
- `docs/work/FIX-712-jobs-original-source-evidence-self-minted-authority.md`
- `docs/work/FIX-713-jobs-original-source-verification-status-parity.md`
- `docs/work/FIX-714-jobs-postgres-cross-authority-lock-order.md`
- `docs/work/FIX-715-jobs-original-source-publication-replay-authority.md`
- `docs/work/FIX-716-jobs-original-source-provider-protocol-parity.md`
- `docs/work/FIX-717-jobs-original-source-operational-hold-migration-parity.md`
- `docs/work/FIX-718-jobs-application-transactional-queue-admission.md`
- `docs/work/FIX-719-jobs-original-source-verifier-lease-starvation.md`
- `docs/work/FIX-720-jobs-original-source-readiness-hold-capability-coverage.md`
- `docs/work/FIX-721-jobs-original-source-managed-authority-error-classification.md`
- `docs/work/FIX-722-jobs-auto-submit-approval-state-regression.md`
- `docs/work/FIX-723-jobs-managed-cloud-admission-prelock-order.md`
- `docs/work/FIX-724-jobs-certified-fixture-scope-review-first-expectations.md`
- `CHANGELOG.md`

No file under `docs/reviews/` is in scope.

## Implementation Checklist

- [x] Paired SQLite 057/PostgreSQL 035 schema with 14 Phase 614 parity tables and five required
      indexes is registered in both normal migration paths.
- [x] Immutable attempt/event/observation/receipt/transition rows and monotonic CAS head semantics
      are represented in both dialects.
- [x] Database-time lease issue, heartbeat, expiry, fence rotation, exact replay, quarantine, and
      bounded retry behavior are implemented.
- [x] Five provider protocols enforce exact host/path/tenant/job/destination/body and bounded parser
      behavior with public-address-only DNS/TLS transport.
- [x] Provider requests omit credentials and cookies, reject redirects and challenges, and perform
      no employer/provider writes.
- [x] Worker evidence is bound to exact release, activation, manifest, runtime, protocol, provider,
      assignment, attempt, and fence authority.
- [x] Mutable posting evidence cannot self-mint production positive authority.
- [x] Review-first preparation remains non-effect authority when independent employer/risk evidence
      is absent.
- [x] Current authority is rechecked at preparation, queue/running admission, lease/local-run, and
      final effect boundaries.
- [x] Release v1 rejection and exact v2 structural acceptance are covered.
- [x] Migration heads, schema parity, release inventory, protocol digests, and CI self-tests include
      the Phase 614 surfaces.
- [x] Jobs readiness includes `OriginalSourceVerification` as the eighth operational capability and
      explicitly composes its global, capability-specific, and native blocker counts.
- [x] Managed heartbeat expiry and runtime-grant revocation retain a typed unavailable
      classification through SQLite/PostgreSQL assignment-authority rechecks; other registry
      failures remain storage/integrity errors.
- [x] The legacy application state-machine regression expects zero-mutation denial when Auto-submit
      queueing lacks an `approved_execution` snapshot.
- [x] Managed PostgreSQL claim/final-submit paths take the combined
      `H -> exclusive M -> ATS -> fleet` prelock first; unmanaged paths take
      `H -> shared M -> ATS`, and both then take account `D`.
- [x] Reject raw duplicate/conflicting JSON object members before ordinary parsing can collapse
      them, require fatal UTF-8/raw-octet evidence, and cover alias conflicts/equivalence.
- [x] Bound lease work to one 32-candidate normal scan window plus eight separate hold rechecks;
      persist hold backoff or typed supersession so later calls progress fairly without an
      unbounded keyset loop.
- [x] Record the accepted replacement lifecycle source after public lifecycle tests and the
      PostgreSQL heartbeat/first-terminal/conflicting-replay correction landed.
- [x] Rerun final global fmt and server all-target check after FIX-723/FIX-724.
- [x] Finish the final strict Clippy run against the intended source freeze.
- [x] Run final local disposable-index privacy, dependency/provenance, schema, CI-self-test,
      release/workflow-contract, and scoped-diff gates.
- [ ] Rerun the full Rust test aggregate against the frozen source.
- [ ] Attach exact-tip CI, Docker/native, live PostgreSQL, and hosted release evidence under separate
      authorization before any activation decision.

## Build & Test Evidence

Observed checkpoints across the accepted focused source digests:

Accepted focused source digests:

```text
server/src/db/jobs/original_source_verification.rs             e952124022c5e97a1fa49fcb34a95bfce8c9c363468792eae61fbd6c40261016
server/src/db/jobs/managed_cloud_release_authority.rs          e7e1f589b411d82b119e841ee778acc24a3b106f36619ec8050fb1a985ac14cb
jobs/automation/src/original-source-verification.ts            934b176fd06c2bb7782c63fa9f0d6c53e42503c013af4cc9c5e4152442c1a165
jobs/automation/tests/original-source-verification.test.ts     d2cce7d88f3675c009e9853ac201a69a77013d3f1bac767db49d06ec8895a3fe
server/src/db/jobs/execution_leases.rs                         e0c95a4b29dc70991f2d386ba4dd4ef151da8aac83a1b6a5fa27934381ed15ce
server/src/db/jobs/tests.rs (FIX-724 exact-run checkpoint)     142aa829a337d4e34a1194a64b203dbc8829becb7495ebc161338e18ab1be320
server/src/db/jobs/tests.rs (final intended source)            0f4775b375c755c63c6563b3885788681f7a45678102058f631dd68935bec325
```

```text
Provider verifier fixtures/adversarial matrix        ✅ 27 / 27
Post-freeze automation Vitest                        ✅ 708 passed / 1 skipped; 38 files / 1 skipped
Post-freeze workflows Vitest                         ✅ 300 / 300; 13 files
Post-freeze automation/workflows typecheck           ✅ passed
Automation TypeScript typecheck                      ✅ passed
Provider Prettier 3.6.2 check                        ✅ passed
First fresh full Jobs aggregate                     ❌ 1,883 passed / 1 failed / 1 skipped
  Runner timeout                                    ❌ 307 / 308; volume-purge 1,000ms sentinel only
Immediate isolated + repeated volume-purge          ✅ 1 / 1; then 20 / 20
Subsequent runner aggregates                        ✅ 3 / 3 at 308 / 308
Second fresh full Jobs aggregate                    ✅ 1,884 passed / 1 skipped
  Test files                                        ✅ 146 passed / 1 skipped
  Automation                                        ✅ 708 passed / 1 skipped; 38 files / 1 skipped
  Browser                                           ✅ 219 / 219; 34 files
  Runner                                            ✅ 308 / 308; 33 files
  Workflows                                         ✅ 300 / 300; 13 files
  Portal                                            ✅ 349 / 349; 28 files
All five Jobs workspace typechecks                  ✅ passed after final parser hardening
All five Jobs production builds                     ✅ passed after final parser hardening
Portal Vite build                                   ✅ 2,299 modules; existing >500 kB warning
Disposable-index privacy gate                      ✅ 2,648 tracked paths / 2,373 text files
Dependency/provenance                              ✅ 663 lock entries / 631 unique versions /
                                                       1 audited override / 14 commit-pinned repos
Browser release CI gate                            ✅ 10 / 10; workflow contract passed
Managed-cloud workflow contract                    ✅ passed
Browser account-deletion pending-flow gate         ✅ 3 / 3
Jobs CI guard self-tests                           ✅ passed
Native runner storage tests                        ✅ 14 / 14 (1 lib + 13 integration; bin 0)
Native runner fmt/strict Clippy/release build      ✅ passed
Darwin native-addon smoke                          ✅ passed
Rust original-source authority                       ✅ 25 / 25 in four owner/reviewer runs; PG URL case self-skipped
Public SQLite lifecycle/replay subset                ✅ 6 / 6 owner and reviewer
Typed assignment expiry/revocation                   ✅ 1 / 1 (4.82s final source)
Held-prefix fairness                                 ✅ 1 / 1 (3.73s final source)
Verifier heartbeat/terminal PG lock order            ✅ focused static regression passed
Jobs operations readiness regression                ✅ 1 / 1 (0.00s final source)
Application state-machine regression                ✅ 1 / 1 (2.33s final source)
Provider-source Review-first regressions             ✅ 2 / 2 (0.07s, 2.84s final source)
Submitted verified-runner finalization denial       ✅ 1 / 1 (2.51s final source)
Certified fixture-scope subset                       ⚠️ 6 passed / 8 expected authority denials
Cloud/local intervention diagnostics                 ❌ 0 / 2; deeper shared-authority blockers
Protected-admission PostgreSQL lock order            ✅ 1 / 1
Managed-cloud release v1/v2 gate                     ✅ 17 / 17
Schema parity                                        ✅ 95 tables / 79 indexes per dialect
SQLite migration/operational-hold replay             ✅ focused pass
Execution-lease regressions                          ✅ 13 / 13
Local-run regressions                                ✅ 4 / 4
Reservation source/discovery recheck                 ✅ 1 / 1; ATS/integrity parked for Phase 614B
Projection/effect regressions                        ✅ 2 / 2
Static PostgreSQL lock-order regressions             ✅ 3 / 3
Runner-plan review-first matrix                      ✅ 2 / 2
Final-source cargo check --all-targets               ✅ passed (34.06s)
Final-source strict Clippy, -D warnings              ✅ passed (49.41s)
Pre-final Rust library baseline                      ❌ 1,416 passed / 34 failed / 1,450 total
Integration E2E                                      ❌ 86 passed / 22 failed / 108 total
```

The provider matrix covers all five families across positive, closed `404`/`410`, identity or
destination mismatch, redirect, malformed/hostile response, authentication, CAPTCHA, rate limit,
timeout, URL userinfo, and private-address/DNS rejection. It uses local fixtures/mocks only. The
post-review expansion also covers raw duplicate keys across all five families plus escaped/nested
equivalence, fatal malformed UTF-8, exact-octet digest distinction, missing raw bodies, 14
contradictory alias cases, and three equivalent-alias positives. The final two tests close response
metadata: only exact `application/json` with optional UTF-8 charset and absent/`identity` content
encoding is accepted; hostile substring/JSONP/other parameters and overlong headers fail closed,
and encoding plus bounded/over-limit header fingerprints are digest-bound. Streamed over-limit
bodies bind the exact bounded `max+1` prefix, and distinct oversized bodies retain distinct content
digests; declared oversize remains bound by exact header evidence.
Recognized-but-malformed aliases and nested records also fail closed rather than becoming absent:
the final focused expansion adds 17 cases across all five providers for scalar IDs/provider fields,
workplace/timestamps, Lever lists and salary shapes, SmartRecruiters job-ad sections, and Ashby
`isListed`.

The replacement 25-test Rust module invokes the public SQLite lease, heartbeat, completion, and failure
APIs. Its 6-test public subset covers lease/reclaim and bounded held-prefix recovery; heartbeat and
exact replay; positive completion; changed-byte conflict quarantine; exact terminal replay after
later runtime revocation with no remint; denial of a fresh request ID; failure and exact replay;
reclaimed-lease stale heartbeat/complete/fail fencing; and hold/source/runtime publication denial.
The PostgreSQL source regression pins heartbeat, first terminal publication, and changed-byte
quarantine to `H -> M -> D -> assignment`, with identity pre-resolution before `D`, locked-row
revalidation afterward, and a read-only exact-replay helper. Live PostgreSQL remains unproven.
It also covers managed heartbeat expiry and runtime-grant revocation as typed
`ManagedRuntimeAuthorityUnavailable`, typed assignment supersession, and zero attempt/receipt
minting. Other registry failures retain their storage/integrity classification.

All 22 integration failures stop at the shared `setup_execution_lease_run` approval with HTTP
`409` and `Confirm the sponsorship answer before Auto-submit.` They are one repeated
production-representative fixture gap, not 22 independent defects, but the aggregate is not green
and is not waived. A successor positive fixture must satisfy confirmed sponsorship and the
separately reviewed **Phase 614B — Signed Job Integrity Authority** that Phase 614 intentionally
cannot self-mint.

The pre-final Rust library baseline passed 1,416 of 1,450 and failed 34 in 2,277.55s. Its readiness,
held-prefix/typed-authority, application-state, and managed-prelock failures are exact-green under
FIX-720, FIX-719/721, FIX-722, and FIX-723. FIX-724 exact-greens two further stale source/category
expectations. A 14-case certified-fixture triage subset has no `ScopeMismatch`: six pass and eight
reach the intended Phase 614B employer-identity/current-authority denial. This focused evidence
does not establish a revised full-library count, is not merged into the separate 86/108 integration
result, and does not support an all-target green claim.

Diagnostic fixture cleanup only: `execution_lease_fixture` and `local_run_authority_fixture` now
bind `provider_verified_original_source`, retain the valid `approved_execution` envelope/checksum,
and persist a test-only preapproved `queued` row. This lets the tests examine later authority
boundaries without asking Phase 614's public queue gate to fabricate Phase 614B authority. The
latest focused intervention run remains 0/2: cloud stops at `claim_execution_lease` with `Conflict`
in the entitlement/shared execution-authority path; local reaches the `running` update and then
fails the canonical match-score threshold. A threshold tweak was removed because `save_profile`
canonicalizes it to 80. Neither failure is retired.

An earlier full Jobs attempt exposed one workflow mock that lacked the now-required raw response byte
stream. `jobs/workflows/tests/original-source-verification-runtime.test.ts` now uses a real
`Response` carrying exact bytes rather than a text-only pseudo-response. The fixture was corrected
without weakening production parsing. After the final parser hardening, `npm test` rebuilt
automation first and an earlier aggregate passed 1,884 tests plus one explicit conditional Playwright
skip. All five workspace typechecks and production builds then passed. The portal Vite build again
processed 2,299 modules with the existing greater-than-500-kB chunk advisory; the warning itself is
a non-blocking performance follow-up, not a failed build.

The later frozen-source aggregate rerun recorded one P2 load-sensitive harness observation rather
than hiding it. The first full Jobs run passed 1,883, failed one, and skipped one when the runner
volume-purge test hit its explicit 1,000ms `purge deadlocked` sentinel. The same case passed
immediately in isolation, 20/20 isolated repetitions, and three subsequent runner aggregates at
308/308. The second full Jobs aggregate passed 1,884 with one skip; all five typechecks and builds
were green. No source changed and no FIX-725 is opened on this branch. A bounded five-second
sentinel belongs in a successor CI-hardening phase.

Not yet claimed:

```text
Final provider Prettier check                                    GREEN
Public verifier lifecycle/replay SQLite regressions              GREEN; 6 / 6
Verifier heartbeat/terminal PostgreSQL lock order                GREEN static; live PG unproven
Integration E2E positive-path authority                         BLOCKED; 86 / 108 pass
Final global fmt/server check/scoped diff                       GREEN after FIX-723/FIX-724
Final-source strict Clippy                                     GREEN; 49.41s
Pre-final Rust library baseline                                NOT GREEN; 1,416 / 1,450 pass
Full Rust all-target aggregate                                 NOT GREEN; final rerun pending
Local privacy/dependency/provenance/CI guards                    GREEN
Disposable/live PostgreSQL normal migrations and contention     UNPROVEN
Exact managed-runner Docker/Linux build                          UNPROVEN
Exact-tip hosted CI                                              UNPROVEN
Hosted runtime, canary, rollback, deployment, flag read-back     PARKED
```

No authorized `BLUEY_TEST_POSTGRES_URL` was available, so PostgreSQL source/schema parity and
compiled paths are not live database behavior. No local statement upgrades them to hosted or
contention evidence.

The Darwin native runner path is locally green, including 14 storage tests, fmt, strict all-target
Clippy, release build, and addon smoke. The Docker command is absent and the host has only 4.7 GiB
free, so the exact Docker/Linux managed-runner image remains unbuilt and unproven.

## Dependency-Security Observation

A clean production `npm audit` across 252 production dependencies reported three advisories outside
the Phase 614 source-authority change: two high and one moderate.

- direct `pdfjs-dist` in the high-severity affected range `>=5.6.83 <6.2.108`;
- high-severity `nanoid <3.3.18` through `postcss`; and
- transitive moderate-severity `DOMPurify <=3.4.12`, with the root override currently pinned to
  `3.4.12`.

This is a separate dependency-security production blocker. It is not silently waived by green
Phase 614 authority tests and should be fixed in a bounded dependency phase.

## Flag And Side-Effect Ledger

```text
sourceVerification in every current activation            false
directDiscovery                                            false
globalDiscovery                                            false
BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED               0
BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED                        0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED              0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED              0
Authenticated provider actions                             none authorized or performed
Provider writes, applications, messages, deployment        not performed
Customer cohort or production flag read-back               not performed
```

Local signed v2 fixtures represent structural verifier authority only. They are not a current
activation, deployment, or production-readiness claim.

## Deviations From The Frozen Plan

| Deviation                                                                            | Rationale                                                                                                                                                                                            |
| ------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Phase 614 adds 14 parity tables and five required indexes                            | Runtime identity/grants/heartbeats, assignment history, immutable evidence, replay events, transitions, and a current head require separate relational authority rather than overloaded posting JSON |
| SQLite operational-hold widening uses a one-time verified rebuild                    | SQLite cannot alter the historical inline capability `CHECK`; the rebuild preserves and proves the existing event/head ledger and becomes a no-op after widening                                     |
| Queue admission became a general persistence boundary                                | A prior eligibility decision could otherwise race source, managed, ATS, Track, resume, entitlement, or hold changes before `queued`/`running` state was persisted                                    |
| Cross-authority PostgreSQL ordering expanded beyond verifier publication             | Application save/queue and final-effect paths use `H -> M -> ATS -> D`; reservation/running transitions remain `H -> M -> D` pending Phase 614B ATS/integrity composition                            |
| A positive source receipt still cannot reach an effect in current Phase 614 fixtures | Provider presence is not independent employer-identity or scam-risk clearance; Review-first fail-closed behavior is intentional until a separately reviewed authority exists                         |

## Known Follow-Ups

- Retain the non-green full Rust and integration aggregates until Phase 614B supplies their missing
  positive authority fixture; keep the completed local privacy/provenance/CI evidence pinned.
- Track the existing portal greater-than-500-kB build advisory as a bounded performance follow-up;
  do not misreport it as a build failure.
- Harden the load-sensitive runner volume-purge deadlock sentinel in a successor CI phase (for
  example, a bounded five-second sentinel); preserve the initial 1,000ms timeout in this evidence.
- Use **Phase 614B — Signed Job Integrity Authority** to restore production-representative
  integration positive routes with confirmed sponsorship and signed employer-identity/scam-risk
  evidence; do not weaken the gate.
- Add Phase 614B route coverage before claiming payment/service/success branches in the plan matrix.
- Resolve the three production dependency advisories in a bounded dependency-security phase.
- Round 615 remains reserved for source enrollment, rights, scheduling, SLOs, canaries, and operated
  rollout; it does not own the Phase 614B signed job-integrity prerequisite.
- Run exact-tip CI, the exact Docker/Linux image, disposable/hosted PostgreSQL concurrency and
  network tests, immutable registry read-back, signing/protected approvals, real runtime capacity,
  read-only-rootfs attestation, ATS canaries, cohorts, kill switches, and rollback separately.
- Keep every production flag at `0` and every current activation at
  `sourceVerification=false` until all external release gates pass.

## Review Checklist

- [x] Current files match the recorded 57-file checkpoint scope.
- [x] No unrelated change under `docs/reviews/` is included.
- [x] Phase 611 v1 remains fail closed and v2 is a separate exact contract.
- [x] Provider tests are fixture/mock based and authorize no authenticated action or write.
- [x] Focused SQLite/Rust/TypeScript/release/schema/lock tests are recorded with observed counts.
- [x] All flags remain false/`0`; no deployment or production read-back is claimed.
- [x] Review-first limitations and dependency advisories remain visible.
- [x] Raw duplicate/conflicting JSON members, malformed UTF-8, and alias conflicts are rejected and
      covered together with exact media/encoding/header semantics and strict recognized-field
      shapes by the post-fix 27/27 provider suite.
- [x] Final local privacy/dependency/provenance/CI commands and exact counts are attached.
- [ ] External-only evidence is attached before any activation or release verdict is promoted.
