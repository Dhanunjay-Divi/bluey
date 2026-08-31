# FIX-754 — Jobs managed v2 signature audience and production-positive fixtures

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B
> and the current authoritative worktree. The SSD archive was not used.

**Severity:** P1 test-authority and migration-compatibility defect

**Status:** Implemented in source; earlier focused checkpoints predate the runner-specific cloud
fixture, so exact-tip cloud, configured-PostgreSQL, aggregate, and strict-Clippy evidence remains
pending

## Issue

The public managed-cloud import paths validate the signed v2 release and activation audiences, but
the existing `jobs_managed_cloud_signature_sets.target_audience` database constraint admitted only
the v1 audiences. A production-representative Phase 614B positive fixture therefore could not
publish a valid v2 source-verifier release through the ordinary importer.

The old shared test fixture hid that mismatch by disabling SQLite foreign-key and CHECK enforcement
and directly fabricating managed-release rows. Several downstream application tests also replaced
frozen receipt JSON or inserted queued state directly. Those tests could pass without proving the
same source -> ATS -> signed-integrity -> approval -> reservation -> queue path used by production
code.

## Root Cause

Migration 057/035 introduced v2 managed release and activation contracts without widening the
immutable signature-set audience constraint originally created by SQLite migration 055 and
PostgreSQL migration 033. Test setup then optimized for convenient mutable fixtures instead of
using the signed public package/import/apply lifecycle, so the incompatible storage contract was
not exercised.

## Fix Summary

- SQLite migration 058 rebuilds only `jobs_managed_cloud_signature_sets` with the v1 and v2
  audience set, copies every existing row exactly, restores its target index and immutable
  update/delete triggers, commits, and restores foreign-key enforcement.
- PostgreSQL migration 036 inspects the named audience constraint, replaces and validates it only
  when either v2 audience is absent, and leaves an already-correct replay unchanged.
- A replay regression seeds a v1 signature set and a referencing signature, replays the SQLite
  migrations twice, and checks row preservation, zero foreign-key violations, enabled FK
  enforcement, restored index/triggers, accepted v2 audiences, rejected unknown audiences, and
  immutable rows.
- The test-only managed runtime installer now publishes a signed v2 trust policy, release, cohort,
  and activation through the normal import/apply APIs, then issues, claims, and heartbeats a real
  source-verifier runtime grant. The production public wrapper remains unchanged; the explicit
  bootstrap anchor seam is test-only.
- The shared authority fixture now saves a real public discovery import and installs source, ATS,
  and dual-role signed job-integrity authority before preparation. Its runtime cache is isolated by
  database backend and concrete database identity; the general-channel verifier runtime is
  intentionally reusable across accounts while the resolved source authority remains account
  bound.
- `install_production_positive_job_authorities_for_runner` now creates an exact ATS/runtime target
  for the requested runner. Local fixtures retain the local target, while cloud fixtures use
  Linux/x86_64 plus the exact runner build and image identities required by managed execution.
- A test-only exact-source scheduler selector delegates to the ordinary discovery-lease
  transaction. It preserves enrollment, hold, eligibility, fencing, run, and completion behavior
  while preventing an older due source in the shared configured-PostgreSQL database from being
  leased by the wrong fixture.
- Common SQLite and configured-PostgreSQL positive tests use current approval, exact schema-2
  approval persistence, ordinary reservation, and queueing instead of direct receipt or mutable
  managed-table bypasses. The execution-lease fixture now follows the same public discovery,
  signed source-v2, ATS, integrity, approval, reservation, and queue lifecycle for cloud.
- The schema-parity self-test now includes migrations 058/036, validates their migration-runner
  registration, and proves representative Phase 614B semantic drift is rejected.

## Files Modified

| File | Change |
| ---- | ------ |
| `infra/sqlite/server-runtime/058_jobs_signed_job_integrity_authority.sql` | Constraint-safe signature-set rebuild plus signed job-integrity authority. |
| `infra/postgres/server-runtime/036_jobs_signed_job_integrity_authority.sql` | Conditional v2 audience constraint replacement plus signed job-integrity authority. |
| `server/src/db/mod.rs` | Register the paired migrations and exercise the paired authority head. |
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Test-only signed v2 lifecycle installer and SQLite replay regression. |
| `server/src/db/jobs/production_positive_authority_fixture.rs` | Backend-neutral public positive source/ATS/integrity fixture and runtime cache. |
| `server/src/db/jobs/execution_leases.rs` | Route the cloud execution fixture through runner-specific public authority, approval, reservation, and queue paths. |
| `server/src/db/jobs/discovery.rs` | Test-only exact-source selection through the production discovery-lease transaction. |
| `server/src/db/jobs/original_source_verification.rs` | Consume the exact managed verifier runtime in public source verification. |
| `server/src/db/jobs/tests.rs` | Replace shared synthetic positive state with the public lifecycle. |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | Replace direct PostgreSQL receipt/queue fabrication with public approval and queueing. |
| `jobs/scripts/check-jobs-schema-parity.mjs` | Guard the paired Phase 614B schema and migration registration. |
| `jobs/scripts/ci-guards-self-test.mjs` | Include 058/036 and prove registration/semantic drift failures. |

## Edge Cases Handled

- Existing v1 signature sets and their child signatures survive SQLite migration replay.
- SQLite foreign-key enforcement is explicitly restored and checked after each replay.
- The rebuilt table retains its uniqueness contract, target lookup index, and immutable triggers.
- Both v2 audiences are accepted; unknown audiences remain denied by storage.
- PostgreSQL avoids replacing an already-correct constraint on every server startup.
- PostgreSQL fixture caching distinguishes TCP host/port, database, user, and schema; SQLite uses
  the concrete main-database path.
- Runtime heartbeat sequence advances on reuse so a cached verifier remains a valid exact runtime.
- The shared runtime installer preserves the requested bounded 15-minute grant lifetime instead of
  silently reducing it to 60 seconds and poisoning the cache on a longer configured test run.
- Targeted test leasing resolves the exact source through public membership/source reads and does
  not read or mutate the process-global scheduler scan cursor.
- Positive application fixtures enter through public preference, source, ATS, integrity, approval,
  reservation, and queue APIs; direct mutable-row and receipt-replacement helpers are absent.
- Cloud fixtures cannot inherit a local-only ATS runtime target; their exact platform, architecture,
  build, and image identities must match the managed runtime authority.

## Evidence

- **PASS — current JavaScript CI guard checkpoint:** `node jobs/scripts/ci-guards-self-test.mjs`
  completed its schema-parity, migration-registration, and semantic-drift cases after migrations
  058/036 were added.
- **PASS — historical focused checkpoints before the latest runner-specific cloud-fixture edits:**
  SQLite signature-set v2 replay, public source/ATS/integrity lifecycle composition, production
  signed manual approval/reservation/queue, and bounded verifier-runtime grant lifetime.
- **PENDING — exact-tip rerun of those focused checks after the cloud-fixture edits.**
- **PENDING — exact-tip cloud FinalSubmit/single-submit production-positive routes.**
- **PENDING — configured PostgreSQL, full Rust, strict Clippy, and aggregate release gates.**

Historical Rust focused passes are diagnostic context only; they are not final-source release
evidence. The current JavaScript guard pass does not substitute for migration execution on fresh
PostgreSQL or the pending Rust gates.

## How to Test

```bash
# CI guard registration and semantic self-tests.
node jobs/scripts/ci-guards-self-test.mjs
node jobs/scripts/check-jobs-schema-parity.mjs

# Integrated Rust compilation.
CARGO_INCREMENTAL=0 cargo check --manifest-path server/Cargo.toml --tests

# SQLite migration replay and public positive lifecycle.
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  sqlite_signature_set_v2_constraint_rebuild_is_replay_safe -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  public_lifecycles_resolve_one_production_positive_composition -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  production_signed_manual_approval_reserves_and_queues_without_legacy_sanitization -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  production_positive_runtime_fixture_preserves_requested_bounded_grant_ttl -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  cloud_final_submit_proof_is_strict_and_atomic_with_click_and_capacity -- --nocapture
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  certified_cloud_production_path_is_single_submit_even_after_response_loss -- --nocapture

# Configured PostgreSQL production-positive routes. Use only an isolated disposable database.
BLUEY_TEST_POSTGRES_URL=<isolated-postgres-url> CARGO_INCREMENTAL=0 \
  cargo test --manifest-path server/Cargo.toml --lib postgres_ -- --nocapture

# Aggregate gates after focused tests pass.
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml
CARGO_INCREMENTAL=0 cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
```

## Known Limitations

- Exact-tip focused cloud/SQLite, configured PostgreSQL, full Rust, and strict Clippy execution
  remain pending; no final-source behavioral pass is claimed from source inspection, formatting,
  or the earlier focused checkpoint alone.
- The local PostgreSQL fixture is isolated release evidence, not hosted PostgreSQL, interruption,
  failover, registry publication/read-back, or production-key-custody evidence.
- The signed v2 fixture is test-only and does not seed production trust, enable source verification,
  change a production flag, or authorize an external provider write.
