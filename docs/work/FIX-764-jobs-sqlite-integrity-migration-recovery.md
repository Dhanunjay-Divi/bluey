# FIX-764: Jobs SQLite integrity migration recovery

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against the
> authoritative Phase 614B worktree. The SSD archive was not used.

**Severity:** P2 local migration failure containment

**Status:** Implemented; injected interruption coverage is green on the current source. Final
aggregate evidence remains pending.

## Issue

SQLite migration 058 temporarily disables foreign-key enforcement and performs a raw transaction
while rebuilding the immutable managed-cloud signature-set table. A mid-batch SQL error could
return the pooled connection with an open transaction or foreign keys still disabled.

## Root Cause

The migration runner called `execute_batch` and propagated its error without host-side cleanup.
The SQL script normally commits and reenables foreign keys, but those trailing statements do not
run after an earlier statement fails.

## Fix Summary

- Record the connection's incoming `foreign_keys` state before migration 058.
- On every return path, roll back a still-open transaction, restore and verify the original
  foreign-key state, and include cleanup status in a propagated migration error.
- After success, run `PRAGMA foreign_key_check` and reject any violation.
- Add an injected missing-relation failure after `BEGIN IMMEDIATE`; prove rollback removes the
  partial table, autocommit is restored, foreign keys are enabled, and a later invalid child insert
  is rejected.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/mod.rs` | Wrap migration 058 with rollback, foreign-key restoration/verification, and `foreign_key_check`. |
| `docs/work/FIX-764-jobs-sqlite-integrity-migration-recovery.md` | Record the defect, correction, and evidence. |

## Edge Cases Handled

- Failure before or after the raw `BEGIN IMMEDIATE` attempts cleanup without assuming a
  transaction exists.
- The original connection-level foreign-key setting is restored rather than unconditionally
  forcing a different caller state.
- Cleanup failures are retained in the returned diagnostic instead of hiding the original SQL
  error.
- Successful replay still verifies the whole current foreign-key graph.

## Evidence

- **PASS:**
  `db::sqlite_migration_replay_tests::signed_job_integrity_migration_failure_restores_transaction_and_foreign_keys`,
  1 passed and 0 failed.
- **PENDING:** full migration replay suite, final full Rust aggregate, and strict Clippy.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  db::sqlite_migration_replay_tests::signed_job_integrity_migration_failure_restores_transaction_and_foreign_keys \
  -- --exact --nocapture --test-threads=1
```

## Known Limitations

- The injected regression covers a handled SQL error in-process. It does not simulate power loss
  or operating-system termination; SQLite journal recovery remains responsible for crash recovery.
- This change affects SQLite migration containment only. PostgreSQL migration 036 does not disable
  foreign keys or use the same raw transaction pattern.
