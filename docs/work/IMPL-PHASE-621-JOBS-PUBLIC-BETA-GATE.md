# Implementation — Phase 621 Jobs Public Beta Gate

**Status:** local source candidate accepted; production activation is not authorized

**Source state:** aggregate source plus FIX-784
`95696dd0ce204c06ed382ed73f52d46920501bfb`; prior aggregate integration
`60fb5f3e0e9b27c034d7443c706c5a4cc7f28093`; dependency-security and deterministic portal
correction `b3d0c79cecc44ca45bf31e6fbe2838108bfabe3a`; parent CI-budget authority
`680160b14e9113a49d778fe8ad3cf8c974a9da72`; integrated through managed-cloud authority
`66ad0fa87e9216d2190caeb742448df3c796cb3f`. Fresh exact-tip hosted verification remains required.

## Implemented

- Added paired SQLite 060 and PostgreSQL 038 durable cohort migrations. Both seed `draft`, cap `0`,
  and no enrollment window.
- Added atomic first-come admission, sticky cumulative capacity, denial overrides, suspension, and
  compare-and-swap administration for SQLite and PostgreSQL.
- Enrollment, administrative grants, and override writes check account deletion intent inside the
  same transaction before consuming capacity or changing durable account/cohort state.
- Customer routes enforce the cohort independently of the closed authenticated status endpoint.
- Administrative cohort changes, grants, and overrides write their redacted operations-audit row
  in the same database transaction. An audit insertion failure rolls back the mutation.
- Production mutation entry points require a real current administrator actor with no deletion
  intent, validated inside the same transaction as the audited cohort, grant, or override change.
  The unaudited helpers are private to unit tests, and a source guard rejects production visibility
  or optional audit contexts.
- The configured-PostgreSQL audit-failure fixture installs a transactionally created, unique,
  actor-scoped trigger/function pair and removes it transactionally with catalog absence proof.
  Its drop fallback retries cleanup during test unwinding without masking the original panic.
- Administrative grants return the effective state after denial and cohort suspension checks; an
  enrollment row alone is not reported as current access.
- The Jobs portal resolves cohort access before workspace data and keeps public-beta admission
  separate from runner and external-effect authority.
- Every public-beta status or error projection is private and non-storable, and the portal fetch
  explicitly bypasses caches.
- Owner export includes enrollment and override state even when an account never created a Jobs
  profile; that path remains read-only and does not synthesize application identity.
- CI structurally verifies both migration registration arrays, including the exact SQLite and
  PostgreSQL migration tuples, and rejects missing registration as well as missing files.
- The public-beta metric rejects impossible states where live admission exceeds cumulative
  assigned capacity.

## Verified Local Evidence

Executed on 2026-08-30 against the current working tree:

- `npm test --workspace @bluey/jobs-portal`: 30 files and 363 tests passed.
- `npm test --prefix jobs`: automation 791 passed/1 intentional simulator skip, Browser 219,
  runner 308, workflows 300, and portal 363; 1,981 tests passed in total.
- `npm run typecheck --prefix jobs` and `npm run build --prefix jobs`: every Jobs workspace passed;
  the production portal bundle was rebuilt into `web/jobs`.
- schema parity: 105 tables and 90 required indexes passed across SQLite/PostgreSQL.
- Jobs CI guard self-tests, privacy scan (2,714 tracked paths/2,439 text files), dependency/license
  inventory (663 lock entries/631 unique package versions), source provenance, Browser release,
  managed-cloud release, messaging containment, and browser deletion guards passed.
- `cargo +1.95.0 fmt --manifest-path server/Cargo.toml -- --check`: passed after mechanical
  formatting.
- focused `jobs_beta_access` server unit run: 10 passed and 1,587 filtered. The two
  configured-PostgreSQL test functions returned without executing because
  `BLUEY_TEST_POSTGRES_URL` was absent; they are not counted as hosted PostgreSQL proof.
- focused shared/standalone wire run: five `public_beta_*` integration tests passed after FIX-768
  corrected an invalid local-runner claim fixture. The test now proves the distinct master-off 404
  and distribution-paused 503 boundaries.
- a source guard proves that only actor-audited public-beta administration mutations are available
  to production callers. SQLite coverage includes concurrency, deletion, denial/suspension, admin
  truth, and forced audit-failure rollback.
- after the security fixes, focused portal coverage passed 15 tests across the beta gate and API,
  strict portal typecheck passed, the CI guard self-test and 105-table/90-index schema parity
  passed, and `git diff --check` passed.
- Final aggregate Jobs verification on 2026-08-31 passed 1,983 tests with one intentional
  no-egress simulator skip: automation 792, Browser 219, runner 308, workflows 300, and portal 364.
  All five workspaces typechecked and built; a repeat portal build produced the same closed
  `web/jobs` directory digest
  `bf6ab3febf060cddc303be099cd01cc4c3d79d1c0a3a05d625b19e1d3f5d2042` from 34 files.
- FIX-781 removed every known dependency advisory. Production audit covered 252 dependencies and
  the full graph covered 673 dependencies, both with zero advisories; the portal retains a
  text-only PDF.js 6 extraction boundary and all pull-request/release paths now fail on a known
  moderate-or-higher advisory.
- Final low-debug Rust verification passed 1,611/1,611 server library tests and 113/113 HTTP
  integration journeys. Server all-target check, strict all-target Clippy with warnings denied,
  repository formatting, and diff checks passed.
- Final repository guards passed: 105 SQLite/PostgreSQL tables and 90 indexes; privacy scan of
  2,751 tracked paths and 2,475 text files; 663 lock entries, 631 unique package versions, and 14
  commit-pinned provenance repositories; Browser release 10/10; managed-cloud release 18/18;
  browser deletion 3/3; business-messaging containment; and CI guard self-tests.
- Native runner storage formatting, strict Clippy, 14 tests, release build, and the Darwin N-API
  smoke passed. Docker is unavailable on this host, so the exact managed-runner image build/smoke
  remains an exact-tip CI gate.
- Independent aggregate security review and the subsequent FIX-776 through FIX-784 reviews found
  no P0-P3 findings and returned GO for the bounded local source commits. FIX-782 corrects only the
  demonstrably exhausted hosted CI budget. FIX-784 corrects the only PR #35 hosted failure by
  establishing real beta admission in a stale integration fixture; it does not change production
  middleware or authority. A fresh exact-tip green run remains required. This is not a production
  activation verdict.

## Files Created Or Modified

| Area | Files |
|------|-------|
| Durable authority | `infra/postgres/server-runtime/038_jobs_public_beta_access.sql`; `infra/sqlite/server-runtime/060_jobs_public_beta_access.sql`; `server/src/db/jobs_beta_access.rs`; `server/src/api/jobs_beta_access.rs` |
| Server integration | `server/src/api/jobs.rs`; `server/src/api/mod.rs`; `server/src/api/metrics.rs`; `server/src/db/jobs.rs`; `server/src/db/jobs/workspace.rs`; `server/src/db/mod.rs`; `server/src/db/metrics.rs`; `server/src/db/ops_audit.rs`; `server/tests/integration_e2e.rs` |
| Portal gate | `jobs/portal/src/App.tsx`; `jobs/portal/src/api.ts`; `jobs/portal/src/components/AppShell.tsx`; `jobs/portal/src/components/PublicBetaGate.tsx`; `jobs/portal/src/components/PublicBetaGate.test.tsx`; `jobs/portal/src/public-beta-api.test.ts`; `jobs/portal/src/styles.css` |
| Guards and operations | `jobs/scripts/check-jobs-schema-parity.mjs`; `jobs/scripts/ci-guards-self-test.mjs`; `jobs/ARCHITECTURE.md`; `jobs/OPERATIONS.md`; `jobs/README.md`; `ops/bluey-jobs.env.example` |
| Built portal | `web/jobs/index.html` and the exact hashed assets produced by the Jobs portal build |
| Records | `docs/rounds/ROUND-621-JOBS-PUBLIC-BETA-GATE.md`; this implementation record; `docs/work/FIX-768-public-beta-local-runner-gate-fixture.md`; `docs/work/FIX-770-public-beta-deletion-intent-fence.md`; `docs/work/FIX-771-public-beta-private-no-store-responses.md`; `docs/work/FIX-772-public-beta-real-admin-audit-actor.md`; `docs/work/FIX-773-public-beta-migration-registration-guard.md`; `docs/work/FIX-774-public-beta-account-export-completeness.md`; `docs/work/FIX-775-public-beta-metric-count-invariant.md`; `CHANGELOG.md` |

## Remaining Release Gates

- An executed isolated PostgreSQL race/audit run, including the hardened failure-fixture cleanup,
  and hosted migration replay/read-back.
- Exact artifact, deployed auth/suspend smoke, metrics/alert delivery, backup/restore, and rollback.
- Exact-tip CI, resource-capable managed-runner Docker verification, and the separately reviewed
  preproduction cap-2 race before any production cap is opened.

The master flag, cohort, and every model, runner, workflow, mailbox, communication, and provider
effect remain dark until those gates have separate evidence and approval.
