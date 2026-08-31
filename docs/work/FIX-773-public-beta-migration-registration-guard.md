# FIX-773: Public-beta migration guard verified declarations but not runtime registration

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The Jobs schema-parity guard could still pass after removing the SQLite 060
entry from `MIGRATIONS` or the PostgreSQL 038 tuple from
`POSTGRES_POST_JOBS_MIGRATIONS`.

## Root Cause

The Phase 621 checks searched the complete concatenated migration source for an
include path and counted symbol occurrences. The constant declarations and
Rust unit-test references kept those strings present even if the actual runtime
migration-array entry was removed.

## Fix Summary

Added a dedicated Phase 621 registration checker that independently requires:

- exactly one SQLite include declaration and exactly one 060 symbol inside the
  parsed `MIGRATIONS` array;
- exactly one PostgreSQL migration-ID declaration and include declaration; and
- exactly one `(migration ID, SQL)` tuple inside the parsed
  `POSTGRES_POST_JOBS_MIGRATIONS` array.

The CI guard self-test now mutates each runtime array independently and proves
that both omissions are rejected.

## Files Modified

| File | Change |
|------|--------|
| `jobs/scripts/check-jobs-schema-parity.mjs` | Validate the exact SQLite and PostgreSQL runtime-array registrations |
| `jobs/scripts/ci-guards-self-test.mjs` | Remove each registration in memory and require the guard to fail |
| `docs/work/FIX-773-public-beta-migration-registration-guard.md` | Record diagnosis and verification |

## Edge Cases Handled

- Duplicate declarations or duplicate runtime registrations are rejected.
- A declaration retained only for a test cannot satisfy runtime registration.
- SQLite and PostgreSQL failures identify the affected dialect and migration.

## How to Test

```bash
node jobs/scripts/ci-guards-self-test.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
git diff --check
```

Observed locally on 2026-08-30: the CI guard self-tests passed, schema parity
passed with 105 tables and 90 required indexes, and the diff check passed.

## Known Limitations

- The guard proves source registration. Hosted migration execution and database
  read-back remain separate release gates.
