# FIX-705: PostgreSQL Managed-Cloud Migration Was Not Replay-Safe

> **Codex preflight:** Loaded `$bluey-ops` and reviewed the embedded PostgreSQL migration runner
> and migration 033 in the current authority worktree. No hosted database, deploy, release
> activation, or production flag was used.

## Issue

The server's required second PostgreSQL migration pass could stop on migration 033 with a duplicate
column or duplicate named constraint instead of completing idempotently.

## Root Cause

The runtime runner intentionally executes post-Jobs migrations on every startup. Migration 033
contained fifteen unguarded additions across cleanup-target and execution-lease tables plus six
unguarded named constraints; the first repeated `managed_cloud_binding_sha256` addition failed
before later managed-cloud authority could be checked or repaired.

## Fix Summary

- Make every one of migration 033's sixteen `ADD COLUMN` statements use `IF NOT EXISTS` (including
  the one already guarded before this correction).
- Guard each of the six named constraints by both `pg_constraint.conname` and the exact
  `conrelid`, then add it only when absent.
- Preserve the original checks, foreign keys, delete actions, and complete-binding invariants.
- Add an embedded-migration regression that inventories all sixteen guarded columns and all six
  table-scoped constraint guards.
- Continue requiring an actual PostgreSQL double migration pass before production acceptance.

## Files Modified

| File | Change |
|------|--------|
| `infra/postgres/server-runtime/033_jobs_managed_cloud_release_authority.sql` | Make managed-cloud column and named-constraint additions replay-safe |
| `server/src/db/mod.rs` | Assert the embedded migration contains all guarded column and constraint additions |

## Edge Cases Handled

- replay after all columns and constraints already exist;
- a same-named constraint on another table;
- partial migration with columns present but constraints missing;
- cleanup-target and execution-lease managed-cloud bindings; and
- preservation of the existing migration-ledger row while self-healing schema objects.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  managed_cloud_release_authority_postgres_migration_is_replay_safe

# Requires BLUEY_TEST_POSTGRES_URL for an isolated disposable database and must run migrations
# twice through the normal server path:
cargo test --manifest-path server/Cargo.toml --lib postgres_local_authority_tests -- --nocapture
```

The source regression is present in the shared tree. The configured disposable-PostgreSQL suite
passed all 13 selected tests after `postgres_pool` completed the normal migration runner twice, so
replay is no longer supported only by parsed SQL. The source regression is also included in the
clean 1,401-test server-library target, and the complete 1,517-test all-target command passed with
zero failures or ignored tests.

## Known Limitations

- This correction addresses replayability, not hosted migration scheduling, backup, rollback, or
  interruption recovery.
- The exact managed-runner Docker/Linux image and native smoke remain separate pending gates.
