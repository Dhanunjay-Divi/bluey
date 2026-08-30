# FIX-739: Jobs Integrity Schema Constraint Evidence

**Severity:** P2 audit/evidence gap

**Status:** 🟡 Implemented; final exact PostgreSQL and interruption evidence pending

## Issue

Generic schema parity compared table/column/index shapes but did not prove Phase 614B's immutable,
singleton, restrictive-FK, predecessor, transition, and compare-and-swap constraints.

## Required Fix

- Assert exact Phase 614B constraints and immutability triggers in both dialects.
- Preserve the exact seven-table boundary and replay-safe registration.
- Add successful generation-two, threshold, wrong-role, rotation, and constraint regressions.
- Keep live PostgreSQL behavior explicitly pending unless actually exercised.

## Implementation

The paired migrations
`infra/sqlite/server-runtime/058_jobs_signed_job_integrity_authority.sql` and
`infra/postgres/server-runtime/036_jobs_signed_job_integrity_authority.sql` create exactly seven
Phase 614B tables with immutable/restrictive relationships, predecessor validation, monotonic
control/head state, and compare-and-swap protections. `jobs/scripts/check-jobs-schema-parity.mjs`
asserts Phase 614B columns, foreign keys, functions, and triggers rather than only generic shape.
The configured PostgreSQL catalog helper in
`server/src/db/jobs/postgres_local_authority_tests.rs` asserts the exact current table and trigger
sets.

## Evidence

Mapped coverage:

```text
sqlite_migration_replay_and_replace_bypasses_are_rejected     PRESENT
check-jobs-schema-parity.mjs Phase 614B constraint assertions PRESENT
Exact seven-table / 20-trigger PostgreSQL catalog helper      PRESENT; ENV-GATED
Fresh final-source PostgreSQL 17 catalog manifest             PENDING
SQLite migration interruption/retry on the same pool          PENDING
```

Successful migration/replay and foreign-key restoration paths exist, but migration 058 contains
its own transaction plus `foreign_keys` toggle while the Rust loader uses `execute_batch` without
an explicit mid-migration rollback/restoration wrapper. An interrupted migration on the same pool
has not been proven recoverable; this and the fresh exact PostgreSQL manifest keep the verdict
yellow.
