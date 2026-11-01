# Phase640 verification — prepared, not dispatched

> **Codex preflight:** `$bluey-ops`. These are parent-controlled gate requirements,
> not a completed run or permission to query an owner database.

Workspace `/Users/uno/.codex/worktrees/jobs-cost-evidence/cue`.
Branch `feat/phase-640-jobs-cost-evidence`; accepted base32ca96db.
No compilation/SQLite/PostgreSQL gate has run on this branch. Full verification
remains behind the unchanged32-GiB free-space preflight. Do not copy a prior
source pin or lower that guard to manufacture acceptance.

## Admission before any heavy command

1. Verify FIX856/857/858's reviewed source freeze and formatting. Capture the final whole
   Jobs/server/core/infra/workflow/Cargo scope, exact HEAD/branch/status and empty
   real index. Include all new source/test files; docs alone do not prove source.
2. Require host free space>=32GiB, acquire the existing shared `mac-build` lock,
   and enter `scripts/run-bluey-tests.sh -- <controller>` with a new private
   `/private/tmp/bluey-test-runs-phase640-*` parent. No owner HOME/data/database
   or default/shared Docker/service context is a test target.
3. Reuse the reviewed environment-cleaning/cache-link pattern in
   `COMMAND-PHASE-638-NOV1-REVERIFY.md`, not its old source hashes or output
   acceptance. Fresh HOME/Cargo target/data/config/log/tmp/SQLite paths; offline
   locked Cargo, no inherited PostgreSQL/provider/auth variables. Keep registry
   and toolchain reads separate from credentials. No paid network calls.
4. Exclusively create/fsync a bounded private external journal and bounded raw
   diagnostic files before dispatch. Install signal/child cleanup first. Freeze
   all source through actual terminal and independent final cleanup/hash checks.

## Command families inside the owned controller

Use the feature set:
`integration-test-support,business-messaging-simulator-test-support`.
Retain the previously qualified test-only optimization overrides for
curve25519-dalek, ed25519-dalek and sha2; they are compile settings, not changes
to authorization or test assertions.

- Formatting: direct changed-file rustfmt check and server `cargo fmt --all --
  --check`. Parent ran a fresh post-correction formatting pass; recheck at
  dispatch and do not substitute the original pre-fix result.
- Strict all-target Clippy: `cargo clippy --offline --locked --manifest-path
  server/Cargo.toml --all-targets --features <feature-set> -- -D warnings`.
- Discover/count then run the actual new financial SQLite cases with filter
  `db::jobs_provider_cost_holds::tests::sqlite_exposure_report_`.
- Discover/count then run the actual new receipt SQLite cases with filter
  `db::jobs::provider_exposure_receipt_tests::sqlite_exposure_`.
- Actual private PG: use `jobs/scripts/run-nonprod-postgres-tests.mjs`, installed
  PG17 binaries and exact required-fresh case
  `db::jobs::provider_exposure_receipt_tests::postgres_exposure_report_matches_sqlite_metadata_lifecycle_and_rejects_tampering`.
  Pass the ordinary locked/offline Cargo test command through its existing
  `--case-filter` protocol. Its URL/required-PG flags must be created only inside
  this owned wrapper. Inspect its real case report, server exit and physical
  PG/PID/root evidence; a configured-test early return is not a PG pass.
- Stop/verify PG, remove its URL/required flags, then run full server all-target
  tests with the same feature/crypto profile. Observe every target summary,
  zero failures/ignored, non-shrinking baseline and every named new test.
  Keep configured-PG execution proof separate from any default no-PG return.
- Run relevant source/CI/privacy/containment guards and complete prospective
  privacy including every owned new file. No workflow dispatch or push is implied.

## Acceptance and limits

All stages require actual exit0, correct counts/source/branch/index, and complete
owned process/PG/browser/root cleanup. A formatting pass, authored test, source
review, missing process handle or empty root alone is not a runtime pass. Any
source drift, partial journal, cap rejection, skip, signal or cleanup ambiguity
retains failure evidence and rejects acceptance.

The new test fixtures must compare all16 authority families plus holds, usage,
application evidence, object uploads and accounts before/after every report or
report denial. The PG case's SQLite/PG result projection must match without
inventing remote object or employer evidence. Explicit missing-cost/coverage
labels remain required even if arithmetic and database tests pass.

No report/publication/release success is inferred for Phase639's held image,
normal managed runner, object-byte read-back, employer delivery, invoice COGS,
complete application attribution or six-month economics. The full Jobs goal
and all separately authorized integration/promotion gates remain intact.
