# FIX-623: Cover the ATS Runtime Quarantine Ledger in Migration Parity

> **Codex preflight:** Loaded `$bluey-ops` and used only the active Round 604
> worktree and the failing full server test as evidence.

## Issue

The paired ATS migrations gained the bounded runtime-layout quarantine ledger,
but the migration regression still asserted 22 ATS tables. The full server
suite failed with PostgreSQL reporting 23 tables.

## Root Cause

The AC10/AC11 fix updated both migration files and schema-parity inventory, but
did not update the embedded migration test's exact table count or require the
new table, index, overflow marker, and immutability triggers.

## Fix Summary

- Raised the exact SQLite/PostgreSQL ATS migration table count from 22 to 23.
- Required the runtime-layout quarantine table, bounded-overflow marker, scope
  index, and update/delete rejection triggers in both dialects.
- Added the ledger to the fresh-SQLite existence and zero-authority checks.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/mod.rs` | Extend exact paired-migration discovery, table counts, and empty-database assertions. |
| `docs/work/FIX-623-jobs-ats-quarantine-migration-parity.md` | Record the parity regression and repair. |

## Edge Cases Handled

- A migration cannot silently omit the overflow bound or immutable-ledger
  triggers in one dialect.
- A fresh database proves the ledger exists but grants no default authority.
- The regression still rejects seeded certification heads or trust heads.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  db::postgres_migration_tests::ats_certification_authority_is_runtime_migrated_with_dialect_parity

cargo test --manifest-path server/Cargo.toml \
  db::sqlite_migration_replay_tests::ats_certification_authority_starts_empty_with_immutable_history

node jobs/scripts/check-jobs-schema-parity.mjs
cargo test --manifest-path server/Cargo.toml
```

## Known Limitations

- The migration definitions and embedded parity tests are local source
  evidence. Applying them to an authorized live PostgreSQL service remains a
  parked external-environment gate.
