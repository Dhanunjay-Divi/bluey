# FIX-794: Clean up failed SQLite boot-authority migrations

> Codex preflight: `bluey-ops`; Jobs-only Phase 628 local correction.

## Issue

Failure inside SQLite migrations 063, 064 or 065 could return an open transaction
with foreign-key enforcement disabled to the connection pool.

## Root Cause

The three scripts temporarily disable foreign keys and own an IMMEDIATE
transaction. The migration loop used raw `execute_batch`, so a statement error
skipped their COMMIT and PRAGMA restoration. The installed r2d2_sqlite 0.25.0
connection manager reports `has_broken=false`; pool drop is not cleanup. The
guarded 058/062 executors did not protect these later scripts.

## Fix Summary

Route all three boot scripts through one bounded executor, preserving committed
063/064 SQL bytes. Require autocommit before taking ownership; run the script;
reject an unfinished transaction even if individual statements succeeded;
roll back any remaining transaction; restore and read back the prior foreign-key
mode on success and failure. Report execution plus cleanup errors. Successful
execution also checks foreign-key consistency.

## Files Modified

| File | Change |
|---|---|
| `server/src/db/mod.rs` | Dispatch 063-065 to guarded executor; cleanup boundary tests |
| `server/src/db/jobs/source_verifier_boot.rs` | Real populated script failure/replay regression |

## Edge Cases Handled

- Statement failure and omitted COMMIT both roll back partial schema.
- Prior FK ON and OFF modes are restored, not blindly forced ON.
- An existing caller transaction is refused and left untouched.
- After failure with FK ON, an invalid child insert is still rejected and a
  valid write succeeds.
- Injecting a pre-COMMIT error into each real 063/064/065 script preserves
  sqlite_master and seven populated authority/manifest tables. Two subsequent
  full migration replays preserve committed readiness and canonical results.

## How to Test

Use isolated `source_verifier_boot` and `sqlite_migration_replay_tests` library
filters, then strict all-target Clippy and the full all-target server suite.
Observed results are recorded in the Phase 628 IMPL continuation ledger.

## Known Limitations

This wrapper cannot undo a script that already committed before a later error;
the bundled scripts have one terminal COMMIT. It reports cleanup failures rather
than concealing them. This is not a rewrite of unrelated historical migrations
or evidence for PostgreSQL runtime behavior.
