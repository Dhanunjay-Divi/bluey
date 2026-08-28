# REVIEW: PHASE-614 — Jobs Original-Source Verification Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the current Phase 614 worktree against
> Round 614, the reviewed Phase 613 tip, FIX-712 through FIX-724, and the focused evidence below.
> The SSD archive was not used.

**Status:** Focused local source accepted; full-library, integration, and external release evidence
remain conditional

**Source range:** `825947dc97feaf5c3b2698e7931cb301b9a3bda4..working tree`

**Reviewer:** Codex independent source-review agent and principal-engineer reconciliation

**Date:** 2026-08-28

## Review Preconditions

- [x] The Phase 614 authority implementation is present in the designated feature worktree.
- [x] IMPL records the complete 57-file source/documentation scope at this review checkpoint.
- [x] FIX-712 through FIX-724 record actual implementation/review findings.
- [x] No file under `docs/reviews/` changed.
- [x] Every current activation keeps source verification false and production/write flags remain
      `0` in source configuration.
- [x] No live/authenticated provider action, provider write, employer submission, message, deploy,
      customer-cohort mutation, or production flag read-back occurred.
- [x] Raw duplicate/conflicting JSON object members are rejected before ordinary parsing, with
      malformed UTF-8/raw-octet and alias-conflict coverage.
- [x] Public SQLite heartbeat/terminal lifecycle regressions and the PostgreSQL heartbeat,
      first-terminal, and conflicting-replay `H -> M -> D -> assignment` correction are frozen and
      green as local focused evidence.
- [ ] Every remaining aggregate local gate has been rerun against that final source.
- [ ] Exact-tip CI and external runtime/release evidence are attached.

## Per-Task Review

### Closed Verifier Worker, API, And Provider Protocol

| Field           | Value                                                                                                                   |
| --------------- | ----------------------------------------------------------------------------------------------------------------------- |
| Files           | TypeScript provider verifier/tests; workflow API/runtime/entrypoint/tests; private Rust API/auth; Docker/package wiring |
| Source verdict  | 🟢 focused source accepted; no remaining P0/P1 at this checkpoint                                                       |
| Release verdict | 🟡 local fixtures only; exact runtime image and live canaries unproven                                                  |

**Findings:**

- The supported set is closed to Greenhouse, Lever, Ashby, SmartRecruiters, and Workday. Provider
  target, tenant, posting ID, URL/path, canonical destination, response semantics, and parser output
  are validated per family.
- Lever posting/application variants and SmartRecruiters title slugs converge on stable identity
  without broadening the accepted grammar.
- Retrieval is anonymous and semantically read-only. The verifier omits credentials/cookies,
  rejects redirects, pins public DNS results to TLS hostname validation, rejects special-purpose
  IPv4/IPv6 targets, and bounds attempts, time, bytes, decompression, JSON structure, and text.
- Independent review found that ordinary `JSON.parse` overwrites duplicate object members. The fix
  now scans the bounded raw byte-decoded JSON before parsing, rejects duplicate decoded keys across
  all five families including escaped/nested variants, requires fatal UTF-8, hashes exact octets,
  rejects missing raw bodies and contradictory aliases, and permits only typed-equivalent aliases.
- A subsequent review found loose media/header handling. The verifier now accepts only exact
  `application/json` with optional UTF-8 charset and absent/`identity` content encoding; rejects
  hostile substring, JSONP, other parameters, and overlong headers; and digest-binds encoding plus
  bounded/over-limit header fingerprints. Streamed over-limit failures bind the exact bounded
  `max+1` prefix, so distinct oversized bodies retain distinct digests; declared oversize remains
  bound by exact header evidence.
- A further review found that present-but-malformed recognized aliases/nested records could collapse
  to absent. The parser now strictly validates scalar IDs and provider fields, nested records,
  workplace/timestamps, Lever list/salary shapes, SmartRecruiters job-ad sections, and Ashby
  `isListed`, with 17 adversarial cases spanning all five families.
- The post-fix verifier suite passed 27/27, automation typecheck passed, and the final Prettier
  3.6.2 two-file plus scoped diff checks passed. The shared TypeScript/Rust exact-fixture vector was
  repinned.
- No live provider was contacted and no legal/operational canary conclusion is inferred.

### Fenced Database Authority, Immutable Evidence, And Replay

| Field                      | Value                                                                                    |
| -------------------------- | ---------------------------------------------------------------------------------------- |
| Files                      | SQLite 057; PostgreSQL 035; migration registration; verifier DB module; schema/CI guards |
| Source verdict             | 🟢 focused lifecycle/replay and static lock-order source accepted                        |
| PostgreSQL runtime verdict | 🟡 compiled/schema parity only; no authorized live URL                                   |

**Findings:**

- Fourteen paired Phase 614 tables separate release/runtime identity, grants/heartbeats, assignment
  lifecycle, immutable attempts/events/observations/receipts/transitions, and the current CAS head.
- Lease and heartbeat authority use database time, monotonic generation/fence values, exact worker
  and release/runtime bindings, and current source/hold rechecks. Network I/O occurs outside locks.
- The source path is designed so authenticated byte-identical response-loss replay returns the
  already committed immutable result, even after later release/runtime authority loss, without a
  new publication. Changed-byte replay appends a quarantine event and sets the assignment to
  `quarantined`; it does not fabricate a second nonpositive receipt/head. Positive projection
  rejects the earlier head because it requires the assignment to remain `idle`. The public SQLite
  lifecycle tests below exercise those semantics.
- Focused Rust original-source tests passed 25/25 in two owner and two independent-reviewer
  normal-parallel runs; the public SQLite lifecycle/replay subset passed 6/6 in both reviews;
  schema parity passed at 95 tables/79 indexes per dialect; and SQLite operational-hold
  migration/replay passed locally.
- The public subset covers lease/reclaim and bounded held-prefix recovery; heartbeat and exact
  replay; positive completion; changed-byte quarantine; exact terminal replay after later runtime
  revocation without reminting; denial of a fresh request ID; failure and exact replay;
  reclaimed-lease stale heartbeat/complete/fail fences; and publication-time hold/source/runtime
  denial.
- PostgreSQL heartbeat, first terminal publication, and changed-byte replay quarantine now resolve
  immutable identity without a row lock, acquire `H -> M -> D`, then lock and revalidate the exact
  assignment. The exact replay helper remains read-only. A static regression pins that order; live
  contention remains unproven.
- Managed heartbeat expiry and runtime-grant revocation now retain the typed unavailable
  classification through assignment-authority rechecks, supersede the assignment, and mint no
  attempt or receipt. Other registry errors remain storage/integrity failures.
- Live disposable/hosted PostgreSQL migration replay, row-lock contention, interruption, and
  network-fault behavior remain unproven because no authorized `BLUEY_TEST_POSTGRES_URL` existed.

### Projection, Review-First, Queue, And Final Effect

| Field                   | Value                                                                                                                    |
| ----------------------- | ------------------------------------------------------------------------------------------------------------------------ |
| Files                   | Jobs projection/eligibility/application/discovery; execution authority/leases/local runner; focused/integration fixtures |
| Source verdict          | 🟢 fail-closed behavior; one P2 coverage limitation remains                                                              |
| Customer-effect verdict | 🟡 no effect authority activated or exercised                                                                            |

**Findings:**

- Mutable discovery JSON and imports cannot mint production positive source authority. The explicit
  positive helper is `cfg(test)` only; provider snapshots remain non-authoritative inputs until a
  current immutable receipt/head is validated.
- Current source authority is rechecked at execution-capable preparation, transactional
  `queued`/`running` persistence, lease/local-run claim and submit, and the existing final-effect
  boundary together with Track, identity, resume, risk, ATS, hold, entitlement, deletion, and
  policy authority.
- Auto-submit drafts remain `awaiting_review` until the exact `approved_execution` snapshot and
  complete current authority are committed atomically.
- Phase 614 intentionally has no independent employer-identity/scam-clear authority. A hosted-ATS
  snapshot or source receipt alone therefore remains Review-first and cannot queue or authorize an
  employer-facing effect.
- Focused evidence passed 13/13 execution-lease, 4/4 local-run, 1/1 reservation, 2/2 projection,
  and 2/2 runner-plan tests.
- P2 evidence limitation: the plan matrix now proves plan entitlement/release observability and
  fail-closed zero-effect approval/queue behavior, but it cannot reach route-level
  payment/service/success branches until **Phase 614B — Signed Job Integrity Authority** supplies a
  production-representative independent employer/risk fixture. Unit tests cover availability
  mapping; no production authority was weakened to obtain a positive route.
- The 108-test integration E2E run passed 86 and failed 22. Every failure reached the same shared
  `setup_execution_lease_run` approval and received `409` with `Confirm the sponsorship answer
before Auto-submit.` This is one repeated positive-fixture authority gap, not 22 independent
  defects, but the integration gate remains explicitly non-green.
- The pre-final Rust library baseline passed 1,416 of 1,450 and failed 34 in 2,277.55s. Its
  readiness, held-prefix/typed-authority, application-state, and managed-prelock failures are now
  exact-green under FIX-720, FIX-719/721, FIX-722, and FIX-723. FIX-724 exact-greens two further
  stale source/category expectations. A 14-case certified-fixture subset has no `ScopeMismatch`:
  six pass and eight reach the intentional Phase 614B denial. This does not establish a revised
  full-library count, and the full all-target gate remains non-green.

### Managed Release Successor And PostgreSQL Lock Order

| Field           | Value                                                                                       |
| --------------- | ------------------------------------------------------------------------------------------- |
| Files           | Managed release authority/gate/tests; execution/application paths; static lock-order guards |
| Source verdict  | 🟢 focused release/lock-order source accepted                                               |
| Release verdict | 🟡 exact stored artifact/runtime/approvals absent                                           |

**Findings:**

- Phase 611 v1 still rejects source verification. Version 2 requires the exact
  `original_source_verifier` capability, signed protocol, measured workflow entrypoint,
  role-separated runtime identity, grant/instance/heartbeat readiness, and false direct/global
  discovery.
- The release gate passed 17/17 and all-target Rust check/strict Clippy passed at the replacement
  lifecycle source. After FIX-723/FIX-724, final-source global fmt, server all-target check, and
  scoped diff checks passed again; strict Clippy also passed. None of these runs proves a deployable
  stored image.
- Application save/queue and final-effect PostgreSQL paths use `H -> M -> ATS -> D`;
  managed-registry readers take a shared fence and writers remain exclusive. The focused static
  order group passed 3/3. Verifier lease, heartbeat, first terminal publication, and changed-byte
  quarantine use `H -> M -> D -> assignment`; the focused verifier static regression passed in the
  25/25 suite.
- Reservation/running-status transitions currently use `H -> M -> D` and recheck
  original-source/discovery authority. They do not independently re-resolve ATS; Phase 614B owns
  the composed reservation ATS/integrity prerequisite, and the 1/1 reservation regression is not
  represented as proof of that missing authority. Later claim/final-effect gates prevent an
  external effect, but a stale reservation may still consume capacity.
- Managed-cloud claim and final Submit now begin with the combined
  `H -> exclusive M -> ATS -> fleet` prelock; unmanaged claim/submit use
  `H -> shared M -> ATS`, and both branches take account `D` only afterward. The protected-admission
  static regression passed 1/1. This is source evidence only; live PostgreSQL contention remains
  unproven.
- The lifecycle source is frozen at its recorded digest. Later bounded fixes retain their own
  accepted digests and focused evidence; final-source global fmt, server all-target check, and
  scoped diff checks passed. Strict Clippy also passed; the full test aggregate is not yet green.

### FIX-719 — Bounded Verifier Lease Fairness And Audit

| Field                      | Value                                                                                                   |
| -------------------------- | ------------------------------------------------------------------------------------------------------- |
| Files                      | Original-source verification scheduler/tests; strict-v2 runtime test fixture; Round/IMPL/REVIEW/FIX-719 |
| Source verdict             | 🟢 scheduler and public lifecycle 25/25 in four owner/reviewer runs green                               |
| PostgreSQL runtime verdict | 🟡 static lock order green; live contention pending                                                     |

**Findings:**

- One public lease call examines one deterministic 32-candidate normal scan window. It does not
  perform an unbounded keyset walk. Invalid candidates are typed `superseded`; newly held candidates
  enter persisted `retry_wait` backoff, so the next call progresses to valid tail work.
- A separate eight-row due-hold budget rotates a still-active hold without another event, returns a
  released hold to `pending`, and supersedes authority that became invalid.
- An expired active attempt records the exact old-attempt `lease_expired` event before hold
  backoff. Entering backoff does not increment the attempt/fence.
- PostgreSQL enumerates without a row lock, takes account `D`, then reloads and locks the exact
  assignment, preserving `H -> M -> D -> assignment` with hard 32+8 lock budgets.
- The public strict-v2 regression proves 33 held rows ahead of one valid tail: call one moves
  exactly 32 and returns `None`; call two reaches the valid 34th assignment. The focused module
  passed 25/25 in four owner/reviewer runs, including repeated 6/6 public SQLite lifecycle/replay
  tests. PostgreSQL heartbeat, first terminal publication, and conflict quarantine are statically pinned D-before-assignment. Live
  PostgreSQL was unavailable; the URL-conditional test self-skipped.

### FIX-720 — Original-Source Readiness Capability Coverage

| Field             | Value                                                                     |
| ----------------- | ------------------------------------------------------------------------- |
| Files             | Jobs operations readiness unit test; FIX-720; Round/IMPL/REVIEW/CHANGELOG |
| Source verdict    | 🟢 focused regression 1/1                                                 |
| Production impact | None; the implementation already returned the complete capability set     |

**Findings:**

- Phase 614 added `OriginalSourceVerification` as the eighth concrete operational-hold capability,
  but the readiness unit test retained a seven-entry cardinality assertion.
- The fixture now selects the original-source row and proves one inherited global hold, zero native
  blockers, and one combined blocker. This prevents a count-only update from hiding incorrect
  global/specific/native composition.
- The correction changes test expectations only. It does not enable source verification or alter
  production readiness behavior.

### FIX-721 — Managed-Authority Error Classification

| Field                      | Value                                                                              |
| -------------------------- | ---------------------------------------------------------------------------------- |
| Files                      | Original-source assignment authority and tests; long-horizon test fixture; FIX-721 |
| Source verdict             | 🟢 focused 25/25 twice; typed authority 1/1                                        |
| PostgreSQL runtime verdict | 🟡 paired source/static coverage only; live expiry/revocation unproven             |

**Findings:**

- SQLite and PostgreSQL assignment-authority rechecks now map only managed-registry
  `Unavailable`/`Revoked` to `ManagedRuntimeAuthorityUnavailable`; invalid authority, identity
  conflict, unexpected grant-expired, and storage/integrity failures remain `Storage`.
- The SQLite expiry/revocation regression proves typed unavailable, deterministic
  `superseded:managed_authority_revoked`, and zero attempts/receipts.
- The heavy scheduler/lifecycle fixture uses a canonical test-only five-minute heartbeat and
  fifteen-minute authority horizon plus exact public heartbeat refresh. It removes parallel
  wall-clock-load flakiness without changing production TTL or introducing global serialization.

### FIX-722 — Auto-Submit Approval-State Regression

| Field             | Value                                                                     |
| ----------------- | ------------------------------------------------------------------------- |
| Files             | Application state-machine unit test; FIX-722; Round/IMPL/REVIEW/CHANGELOG |
| Source verdict    | 🟢 focused regression 1/1                                                 |
| Production impact | None; FIX-718 already denied the unauthorized transition                  |

**Findings:**

- A legacy regression still expected local/cloud queue capability and
  `awaiting_review -> queued` without Phase 614B employer/risk authority or the exact
  `approved_execution` snapshot required by transactional queue admission.
- The corrected test expects Review-first-only eligibility and queue denial, exercises invalid
  submission-mode handling from the unchanged `awaiting_review` state, and proves both mutations
  preserve the original `awaiting_review`/`review_first` values.
- No approval fixture, queue bypass, or Phase 614B employer/risk substitute was added.

### FIX-723 — Managed-Cloud Admission Prelock Order

| Field                      | Value                                                                            |
| -------------------------- | -------------------------------------------------------------------------------- |
| Files                      | Execution-lease managed claim/final-submit paths; FIX-723; Round/IMPL/REVIEW/log |
| Source verdict             | 🟢 protected-admission static regression 1/1                                     |
| PostgreSQL runtime verdict | 🟡 live contention and failure injection remain unproven                         |

**Findings:**

- Managed claim and final Submit had performed protected work before the combined workflow
  admission prelock and duplicated operational-hold/ATS acquisition around that helper.
- Managed paths now make `H -> exclusive M -> ATS -> fleet` the first protected operation;
  unmanaged paths retain `H -> shared M -> ATS`; both acquire account `D` afterward.
- The focused static regression passed 1/1 and scoped diff check passed. This does not establish
  deadlock freedom or interruption behavior without a live PostgreSQL run.

### FIX-724 — Certified Fixture Scope And Review-First Expectations

| Field          | Value                                                                                |
| -------------- | ------------------------------------------------------------------------------------ |
| Files          | Jobs database tests; FIX-724; Round/IMPL/REVIEW/CHANGELOG                            |
| Source verdict | 🟢 two exact regressions green; certified subset reaches intended authority boundary |
| Aggregate      | 🟡 six passed/eight expected Phase 614B denials; full suite remains non-green        |

**Findings:**

- Two certified ATS fixtures now use the honest `provider_verified_original_source` scope instead
  of the execution-grade, test-only source label. The 14-case certified subset has no
  `ScopeMismatch`: six pass and eight reach the intentional employer-identity/current-authority
  denial owned by Phase 614B.
- Two historical source/category regressions now assert Review-first preparation without local or
  cloud queue authority. Their exact runs passed 1/1 each.
- The correction neither fabricates signed employer/risk evidence nor treats the eight downstream
  authority denials as green.
- `execution_lease_fixture` and `local_run_authority_fixture` now bind provider-verified evidence,
  retain the real approved-execution envelope/checksum, and use a test-only preapproved queued row.
  The latest focused intervention result is still 0/2: cloud stops at claim with a shared
  entitlement/execution-authority `Conflict`; local reaches the running update before the canonical
  match-score threshold denies it. This is honest diagnostic cleanup, not acceptance evidence.

### Documentation, Flags, And Dependency Security

| Field              | Value                                                      |
| ------------------ | ---------------------------------------------------------- |
| Files              | Round 614, IMPL/REVIEW, FIX-712 through FIX-724, CHANGELOG |
| Source verdict     | 🟢 boundaries recorded                                     |
| Production verdict | 🟡 external gates and dependency blocker remain            |

**Findings:**

- The audit record distinguishes focused source evidence from final local aggregates and external
  production proof. `docs/reviews/` remains untouched.
- Every current activation remains `sourceVerification=false`; direct/global discovery and all
  production/provider-write flags remain false or `0`. No deploy or production flag read-back is
  claimed.
- A clean audit across 252 production dependencies found three advisories: high-severity direct
  `pdfjs-dist >=5.6.83 <6.2.108`, high-severity transitive `nanoid <3.3.18`, and moderate-severity
  transitive `DOMPurify <=3.4.12` at the current root override. This is a separate production
  dependency-security blocker, not a waived Phase 614 nit.

## Adversarial Review Matrix

| Boundary                                                         | Reviewer evidence                                                                                                                                                   |
| ---------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Concurrent/stale worker                                          | Public reclaimed-lease stale heartbeat/complete/fail fences green; live PostgreSQL contention pending                                                               |
| Lease expiry and database-time heartbeat                         | Exact expired-attempt event/reclaim, heartbeat replay, and bounded hold backoff green; hosted clock/failure behavior pending                                        |
| Identical replay and changed-byte conflict                       | Public exact replay/no-remint and changed-byte quarantine/old-head rejection green                                                                                  |
| Runtime/release/activation revocation                            | Exact replay after later runtime revocation and denial of a fresh request green; real propagation pending                                                           |
| Provider host/tenant/path/job/destination binding                | Covered for all five families in post-fix 27/27 provider suite                                                                                                      |
| Redirect, SSRF, DNS, size, parser, timeout and retry bounds      | Local matrix green, including duplicate keys, fatal UTF-8, raw-octet/header digests, exact JSON media/encoding, alias conflicts, and strict recognized-field shapes |
| Closed/mismatch/material-change/unknown/unreachable/auth/CAPTCHA | Closed vocabulary is fail closed; exact worker/Rust observation tuples are server validated                                                                         |
| Receipt immutability and monotonic CAS head                      | Paired constraints/triggers plus public positive terminal, exact replay, and quarantine regressions green                                                           |
| Review-first versus execution-capable preparation                | Review-first denial and zero mutation proven; independent risk authority intentionally absent                                                                       |
| Queue and final effect rechecks                                  | Save/queue/effect groups are focused-green; reservation is source/discovery-only pending Phase 614B ATS/integrity; integration remains 86/108                       |
| Phase 611 v1 rejection and successor exactness                   | Managed release gate 17/17                                                                                                                                          |
| SQLite/PostgreSQL parity and migration behavior                  | 95 tables/79 indexes per dialect; SQLite replay green; live PostgreSQL unproven                                                                                     |

## Build & Test Verification

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
Provider verifier suite                             ✅ 27 / 27 post-fix
Post-freeze automation Vitest                       ✅ 708 passed / 1 skipped; 38 files / 1 skipped
Post-freeze workflows Vitest                        ✅ 300 / 300; 13 files
Post-freeze automation/workflows typecheck          ✅ passed
Automation TypeScript typecheck                     ✅ passed
Provider Prettier 3.6.2 check                       ✅ passed
First fresh full Jobs aggregate                    ❌ 1,883 passed / 1 failed / 1 skipped
  Runner timeout                                   ❌ 307 / 308; volume-purge 1,000ms sentinel only
Immediate isolated + repeated volume-purge         ✅ 1 / 1; then 20 / 20
Subsequent runner aggregates                       ✅ 3 / 3 at 308 / 308
Second fresh full Jobs aggregate                   ✅ 1,884 passed / 1 skipped
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
Rust original-source authority                      ✅ 25 / 25 in four owner/reviewer runs; PG URL case self-skipped
Public SQLite lifecycle/replay subset               ✅ 6 / 6 owner and reviewer
Typed assignment expiry/revocation                  ✅ 1 / 1 (4.82s final source)
Held-prefix fairness                                ✅ 1 / 1 (3.73s final source)
Verifier heartbeat/terminal PG lock order           ✅ focused static regression passed
Jobs operations readiness regression               ✅ 1 / 1 (0.00s final source)
Application state-machine regression               ✅ 1 / 1 (2.33s final source)
Provider-source Review-first regressions            ✅ 2 / 2 (0.07s, 2.84s final source)
Submitted verified-runner finalization denial      ✅ 1 / 1 (2.51s final source)
Certified fixture-scope subset                      ⚠️ 6 passed / 8 expected authority denials
Cloud/local intervention diagnostics                ❌ 0 / 2; deeper shared-authority blockers
Protected-admission PostgreSQL lock order           ✅ 1 / 1
Managed-cloud release v1/v2                         ✅ 17 / 17
Schema parity                                       ✅ 95 tables / 79 indexes per dialect
SQLite operational-hold migration/replay            ✅ focused pass
Execution-lease suite                               ✅ 13 / 13
Local-run suite                                     ✅ 4 / 4
Reservation source/discovery recheck                ✅ 1 / 1; ATS/integrity parked for Phase 614B
Projection/effect group                             ✅ 2 / 2
Static PostgreSQL lock order                        ✅ 3 / 3
Runner-plan review-first matrix                     ✅ 2 / 2
Final-source Rust all-target check                  ✅ passed (34.06s)
Final-source strict Clippy, `-D warnings`            ✅ passed (49.41s)
Pre-final Rust library baseline                     ❌ 1,416 passed / 34 failed / 1,450 total
Integration E2E                                     ❌ 86 passed / 22 failed / 108 total

Provider Prettier check                              GREEN
Public verifier lifecycle/replay SQLite regressions  GREEN; 6 / 6
Verifier heartbeat/terminal PG lock order            GREEN static; live PG unproven
Final global fmt/server check/scoped diff              GREEN after FIX-723/FIX-724
Final-source strict Clippy                              GREEN; 49.41s
Full Rust library/all-target aggregate               NOT GREEN; final rerun pending
Local privacy/dependency/provenance/CI guards        GREEN on final source
Live PostgreSQL migration/contention                 UNPROVEN; no authorized URL
Docker/Linux image                                   UNPROVEN; Docker absent, host has 4.7 GiB free
Exact-tip hosted CI                                  UNPROVEN; worktree not pushed
```

An earlier full Jobs attempt found one workflow mock without the newly required raw response bytes.
After replacing the text-only pseudo-response in
`jobs/workflows/tests/original-source-verification-runtime.test.ts` with a real byte-bearing
`Response`, workflows passed 300/300. After the final parser hardening, `npm test` rebuilt
automation first and an earlier aggregate passed 1,884 plus one explicit conditional Playwright
skip. All five workspace typechecks and production builds then passed; the portal again processed
2,299 modules with its existing chunk advisory only. That advisory is a non-blocking performance
follow-up, not a build failure. No parser authority was weakened to repair the fixture.

The frozen-source aggregate rerun also preserved one P2 load-sensitive harness observation. Its
first full Jobs run passed 1,883, failed one, and skipped one because the runner volume-purge test
hit its explicit 1,000ms `purge deadlocked` sentinel. Immediate isolation passed, 20/20 isolated
repetitions passed, and three subsequent runner aggregates passed 308/308; the second full Jobs
aggregate then passed 1,884 with one skip, with all five typechecks/builds green. Source did not
change. This is not a Phase 614 production/source blocker and does not warrant FIX-725 on this
branch; a bounded five-second sentinel remains a successor CI-hardening follow-up.

## External-Only Evidence

Still required before activation or release:

- exact-tip hosted CI;
- exact managed-runner Docker/Linux image and verifier entrypoint/native smoke;
- immutable registry read-back, threshold signatures, and protected approvals;
- disposable and hosted PostgreSQL migration, lock, replica, interruption, and network-fault proof;
- real verifier heartbeat/capacity/task-queue and revocation propagation;
- legally approved anonymous live-provider canaries, rights/rate budgets, and operated SLOs;
- runtime digest and read-only-rootfs attestation; and
- dark deploy, kill switch, rollback rehearsal, customer cohort approval, monitoring/on-call, and
  production flag read-back.

None is inferred from local source or fixture evidence. All production/write flags remain `0`, and
no current activation or customer effect changed.

The Darwin native runner/addon path is locally green. That does not substitute for the absent exact
Docker/Linux image; no Docker command was available and the host had only 4.7 GiB free.

## Overall Verdict

🟡 **FOCUSED LOCAL SOURCE ACCEPTED; NO RELEASE AUTHORITY** — The public SQLite lifecycle and
verifier PostgreSQL static lock-order P1 findings are closed by 25/25 focused tests in four
owner/reviewer runs, including repeated 6/6 public subsets; the later managed admission ordering
regression is separately exact-green 1/1. The integration E2E gate remains 86/108 because Phase
614 intentionally lacks the independent signed employer/risk fixture owned by Phase 614B. The full
Rust test aggregate, exact-tip CI, live PostgreSQL, the Docker/Linux image, every hosted release
gate, and the three production dependency advisories remain open; source verification must stay
false in every current activation.

## Follow-Ups

- Preserve the final local privacy/dependency/provenance/CI counts while keeping exact-tip hosted CI
  separate and unproven.
- Restore a production-representative integration positive fixture through **Phase 614B — Signed
  Job Integrity Authority**, with confirmed sponsorship and independent signed employer/risk
  evidence; keep the current 22 shared fail-closed denials visible until then.
- Do not move this prerequisite into Round 615; that round remains reserved for the Source Control
  Plane and Freshness SLOs.
- Resolve the three production dependency advisories in a bounded dependency-security phase.
- Track the existing portal chunk-size advisory as a bounded performance follow-up.
- Harden the load-sensitive runner volume-purge deadlock sentinel in a successor CI phase while
  preserving this audit's initial 1,000ms timeout.
- Preserve Round 615 source-control-plane and SLO scope separately.
- Run external release/deployment evidence only under separate authorization; do not deploy or
  enable flags from this review.
