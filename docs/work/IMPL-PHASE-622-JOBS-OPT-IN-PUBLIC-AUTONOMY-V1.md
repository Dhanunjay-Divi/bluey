# Implementation — Phase 622 Jobs Opt-In Public Autonomy V1

**Status:** local aggregate source candidate accepted; autonomous V1 and production release are
not claimed

**Source state:** source commit `55d24e95234c1fcc2db22a912c739dfd5760eb66`; deterministic
generated portal commit `95bb966fd897db94599a0c7e2defe1fb02ea3912`; both based through
storage predecessor `3f90bc01210345df56f24a8e95a493895c9744ae`. Integrated Phase 611
compatibility and hosted verification remain required before release.

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
- Final aggregate Jobs verification passed 1,982 tests with one intentional no-egress simulator
  skip: automation 791, Browser 219, runner 308, workflows 300, and portal 364. All five workspaces
  typechecked and built. A repeat portal build retained the exact 32-file directory digest
  `65be1e2ebda67b02fcd1c9340240bb0100aade36a2748c2266369898b3d65235`; Vite emitted only its
  existing advisory for a chunk slightly above 500 kB.
- Final schema, privacy, provenance, release, deletion, containment, and CI guards passed: 105
  tables/90 indexes; 2,719 tracked paths/2,443 text files; 663 lock entries/631 package versions/14
  pinned repositories; Browser release 10/10; managed-cloud release 17/17; and deletion 3/3.
- Native runner storage formatting, strict Clippy, 14 tests, release build, and the Darwin N-API
  smoke passed. Docker is unavailable on this host, so the exact managed-runner image build/smoke
  remains an exact-tip CI gate.
- Independent aggregate security review and the FIX-776 through FIX-778 reviews reported no P0-P3
  findings and returned GO for the bounded local source commit. Hosted launch remains NO-GO.

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
