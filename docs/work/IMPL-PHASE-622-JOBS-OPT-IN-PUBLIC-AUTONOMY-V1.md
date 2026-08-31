# Implementation — Phase 622 Jobs Opt-In Public Autonomy V1

**Status:** local aggregate source candidate accepted; autonomous V1 and production release are
not claimed

**Source state:** aggregate source plus FIX-784
`95696dd0ce204c06ed382ed73f52d46920501bfb`; prior aggregate integration
`60fb5f3e0e9b27c034d7443c706c5a4cc7f28093`; dependency-security and deterministic portal
correction `b3d0c79cecc44ca45bf31e6fbe2838108bfabe3a`; parent CI-budget authority
`680160b14e9113a49d778fe8ad3cf8c974a9da72`; integrated through managed-cloud authority
`66ad0fa87e9216d2190caeb742448df3c796cb3f`. Fresh exact-tip hosted verification remains required
before release.

## Current Aggregate

Phase 622 composes the Phase 621 capacity-limited public Jobs gate with the existing review-first
Jobs stack. Cohort admission opens only the Jobs workspace. It does not grant a runner, enable a
provider, authorize a Career Track, send a message, or submit an application.

Portal copy now states that a separately authorized Career Track and an actually available runner
are required before cloud action. Runtime availability remains server-authoritative.

The Phase 621 administration surface is audit-only in production: cohort, grant, and override
mutations require an actor-backed context, and unaudited setup helpers exist only in unit tests.
The configured-PostgreSQL rollback fixture is uniquely scoped and panic-safe so a failed test
cannot silently leave a global audit-blocking trigger behind.

Before any new managed-effect authorization, fresh production or standalone local Browser claim,
fresh local or managed irreversible submit, communication claim, or provider request start, the
database now rechecks the public-beta master gate, permanent verified account, deletion intent,
enrollment, cohort state, and denial override inside the same SQLite or PostgreSQL transaction.
Losing authority prevents a new external write while preserving exact local claim replay, exact
submit replay, result evidence, and lookup-only reconciliation for already-started ambiguous
effects. The local-run HTTP master middleware now covers only fresh resume mutation; claim and
submit reach replay-first database authority, while result remains reachable for terminal evidence.
The raw local-distribution flag is evaluated inside the claim transaction after exact replay and
the fresh beta decision, alongside the durable fleet-readiness check. Exact local submit recovery
supports both the existing schema-3 review-first proof and schema-4 ATS-certified proof. It
revalidates the stored ticket, proof, release, application, session, and evidence capacity, recovers
ATS authority only for schema 4, and returns the original database-owned authorization timestamp so
an exact retry is byte-identical.

## Verified Local Evidence

- The Phase 621 record contains the exact 2026-08-30 command evidence: 1,981 Jobs tests passed
  with one intentional simulator skip, all Jobs workspaces typechecked/built, 363 portal tests
  passed, and schema parity covered 105 tables/90 indexes.
- Privacy, provenance/license, release-authority, messaging-containment, deletion, and CI guard
  suites passed on the aggregate working tree.
- Ten focused public-beta unit tests and five shared/standalone API integration tests passed under
  Rust 1.95. The configured-PostgreSQL cases were not executed because no isolated test URL was
  configured and remain a release gate.
- FIX-768 corrected the integration fixture that previously stopped at Axum's 422 JSON rejection.
  FIX-769 subsequently narrowed the local-run route boundary: fresh claim/submit still close, but
  exact claim/submit replay and result reconciliation are no longer stranded by master-off or
  local-distribution-off.
- The public-beta mutation source guard and transaction-coupled audit rollback tests passed.
- After the security fixes, focused portal tests passed 15/15, portal typecheck passed, the CI
  guard self-test passed, schema parity passed at 105 tables/90 indexes, and `git diff --check`
  passed. A default-debug Rust compile was stopped before testing when it consumed 5 GiB of local
  disk; the build output was safely cleaned.
- A controlled low-debug follow-up passed nine focused Rust checks: both the current and legacy
  end-to-end local-run recovery journeys, the master-off route boundary, four local-submit unit and
  structural tests, the atomic release-claim replay test, and the PostgreSQL click-started lock/time
  ordering guard. Full exact-tip Rust and live PostgreSQL verification remain release gates.
- The first full low-debug server library run passed 1,604 tests and exposed seven stale test-only
  contracts: three operations-route cache expectations, one runner-volume fixture without durable
  admission, and three account-deletion assertions that expected a lower fence after public-beta
  deletion denial. FIX-776 and FIX-777 correct only those tests. Their focused checks passed, and
  the final exact-state full-suite evidence is recorded below.
- The corrected aggregate then passed 1,611/1,611 server library tests and 113/113 HTTP integration
  journeys. The subsequent exact all-target strict Clippy gate found one source-shape blocker in the
  eight-argument local distribution-claim wrapper. FIX-778 replaces that positional interface with
  one typed claim request without changing runtime behavior; its focused claim/replay test and the
  strict Clippy rerun pass.
- After FIX-778, the final exact-state rerun passed 1,611/1,611 server library tests in 1,872.19
  seconds and 113/113 HTTP integration journeys in 558.24 seconds. Server all-target check, strict
  all-target Clippy with warnings denied, repository formatting, and diff checks passed.
- Final aggregate Jobs verification passed 1,983 tests with one intentional no-egress simulator
  skip: automation 792, Browser 219, runner 308, workflows 300, and portal 364. All five workspaces
  typechecked and built. A repeat portal build retained the exact 34-file directory digest
  `bf6ab3febf060cddc303be099cd01cc4c3d79d1c0a3a05d625b19e1d3f5d2042` from 34 files; Vite
  emitted only its
  existing advisory for a chunk slightly above 500 kB.
- FIX-781 upgraded both résumé PDF extractors and the affected transitive packages, preserved the
  text-only parser boundary, and added mandatory moderate-or-higher audit gates to pull-request and
  release workflows. Production audit covered 252 dependencies and the full graph covered 673;
  both reported zero advisories.
- Final schema, privacy, provenance, release, deletion, containment, and CI guards passed: 105
  tables/90 indexes; 2,751 tracked paths/2,475 text files; 663 lock entries/631 package versions/14
  pinned repositories; Browser release 10/10; managed-cloud release 18/18; and deletion 3/3.
- Native runner storage formatting, strict Clippy, 14 tests, release build, and the Darwin N-API
  smoke passed. Docker is unavailable on this host, so the exact managed-runner image build/smoke
  remains an exact-tip CI gate.
- On the integrated V1 state, the complete production storage-guard suite, cloud-preflight suite,
  and restore-drill suite passed; the restore run reported `real_postgres_scenario=executed`. This
  re-proves the local scripts but does not substitute for host activation, remote alert delivery,
  provider cleanup, or production rollback evidence.
- Independent aggregate security review and the FIX-776 through FIX-784 reviews reported no P0-P3
  findings and returned GO for the bounded local source commits. FIX-782 raises only the closed
  hosted Jobs CI budget from 45 to 90 minutes after the previous run timed out at 89/108 integration
  tests with zero failures. FIX-784 admits the stale runner-plan fixture through the real public
  beta gate and preserves the preexisting signed-integrity/no-mutation fence without production
  changes; exact-tip hosted completion remains mandatory. Hosted launch remains NO-GO.

## Aggregate Source Surface

- Phase 621 owns the durable public cohort, server/API integration, portal access gate, metrics,
  guards, migrations, and wire tests listed in its implementation record.
- Phase 622 adds the non-invitation limited-public product language and runner-authority truth in
  `server/src/api/jobs.rs`, `server/src/db/jobs.rs`,
  `server/src/db/jobs/browser_release_authority.rs`, `server/src/db/jobs/local_runner.rs`, portal
  preview/types/runner-access/view tests, `jobs/portal/src/lib/application-flow.test.ts`,
  `jobs/portal/src/views/SettingsView.tsx`, `jobs/scripts/ci-guards-self-test.mjs`, the rebuilt
  `web/jobs` bundle, this Round, and this implementation record.
- FIX-769 records the transactional external-effect authority fence. FIX-770 through FIX-775
  record the deletion, cache, administrator, migration-registration, export, and metric findings
  closed during the aggregate security review. FIX-776 and FIX-777 record the bounded full-suite
  cache-contract and legacy-fixture corrections; neither changes production behavior. FIX-778
  records the typed local distribution-claim boundary that closes the strict Clippy source gate.
  FIX-779 and FIX-780 correct Linux opened-descriptor attestation without weakening trusted-path
  substitution checks. FIX-781 closes the dependency-security release gate, and FIX-782 binds the
  hosted Jobs CI job to the reviewed 90-minute budget. FIX-784 corrects the stale public-beta
  runner-plan test fixture without weakening runtime authority.
- FIX-767 is a separate storage-resilience unit in the same working tree and is inventoried in its
  own record; shared `CHANGELOG.md` and `jobs/OPERATIONS.md` hunks must be staged deliberately.

## Remaining Release Gates

- Push, exact-tip CI, integrated Phase 611 compatibility evidence, and a resource-capable
  managed-runner Docker build/smoke on those exact bytes.
- Hosted PostgreSQL, Temporal, managed-runner capacity/runtime, encrypted-volume, and multi-process
  locking evidence.
- Signed stored-byte artifacts, provenance/SBOM/read-only-rootfs attestation, and rollback proof.
- Controlled ATS application, Gmail/Outlook mailbox, recruiter reply, calendar, ambiguity,
  revocation, deletion, and suspension canaries.
- Preproduction cap-2 race proof, production cap-25 authorization, and the 24–48 hour observation
  record.

Phase 622 must not be called autonomous V1 while every external-effect gate is off. No production
flag, cohort state, cap, deployment, or provider permission is changed by this implementation
record.
