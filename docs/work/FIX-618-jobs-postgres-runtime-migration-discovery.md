# FIX-618: PostgreSQL runtime migrations were not operator-discoverable

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The full server test suite reported that PostgreSQL runtime migrations 027 and
028 would be skipped by `scripts/bluey-postgres-migrate.sh`.

## Root Cause

The migration operator intentionally applies only files whose first twelve
lines contain a `Target: PostgreSQL` marker. The two new runtime-authority
migrations omitted that marker even though the server embedded and tested them.

## Fix Summary

Added the required target marker as the first line of both PostgreSQL migration
files. This keeps operator migration discovery aligned with the server's
embedded migration registry without changing either schema.

## Files Modified

| File | Change |
|------|--------|
| `infra/postgres/server-runtime/027_jobs_browser_release_runtime_components.sql` | Added the PostgreSQL target marker. |
| `infra/postgres/server-runtime/028_jobs_runner_process_runtime_authority.sql` | Added the PostgreSQL target marker. |

## Edge Cases Handled

- Both new migrations now satisfy the same bounded-header contract as every
  existing operator-applied PostgreSQL migration.
- The correction changes comments only; schema parity and migration identity
  remain unchanged.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  db::postgres_migration_tests::embedded_postgres_migrations_are_operator_discoverable \
  -- --exact
# 1 passed; 0 failed
```

## Known Limitations

- This source test proves operator discovery, not application against a live
  PostgreSQL service; the live migration/concurrency gate remains external.
