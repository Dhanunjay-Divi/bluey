# Implementation — Phase 621 Jobs Public Beta Gate

**Status:** local source candidate accepted; production activation is not authorized

**Source state:** source commit `55d24e95234c1fcc2db22a912c739dfd5760eb66`; deterministic
generated portal commit `95bb966fd897db94599a0c7e2defe1fb02ea3912`; both based through
storage predecessor `3f90bc01210345df56f24a8e95a493895c9744ae`.

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
- Final aggregate Jobs verification on 2026-08-31 passed 1,982 tests with one intentional
  no-egress simulator skip: automation 791, Browser 219, runner 308, workflows 300, and portal 364.
  All five workspaces typechecked and built; a repeat portal build produced the same closed
  `web/jobs` directory digest
  `65be1e2ebda67b02fcd1c9340240bb0100aade36a2748c2266369898b3d65235` from 32 files.
- Final low-debug Rust verification passed 1,611/1,611 server library tests and 113/113 HTTP
  integration journeys. Server all-target check, strict all-target Clippy with warnings denied,
  repository formatting, and diff checks passed.
- Final repository guards passed: 105 SQLite/PostgreSQL tables and 90 indexes; privacy scan of
  2,719 tracked paths and 2,443 text files; 663 lock entries, 631 unique package versions, and 14
  commit-pinned provenance repositories; Browser release 10/10; managed-cloud release 17/17;
  browser deletion 3/3; business-messaging containment; and CI guard self-tests.
- Native runner storage formatting, strict Clippy, 14 tests, release build, and the Darwin N-API
  smoke passed. Docker is unavailable on this host, so the exact managed-runner image build/smoke
  remains an exact-tip CI gate.
- Independent aggregate security review and the subsequent FIX-776 through FIX-778 reviews found
  no P0-P3 findings and returned GO for the bounded local source commit. This is not a production
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
