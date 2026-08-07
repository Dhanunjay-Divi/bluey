# FIX-665: Guard Operational-Hold Semantics Across Both Databases

> **Codex preflight:** Loaded `$bluey-ops` and verified the finding against the
> current Phase 606 worktree. No archive, live database, credential, or
> production system was used.

## Issue

Column and index parity alone could report a green SQLite/PostgreSQL schema
while a trigger, function, foreign key, ancestry rule, or trigger binding had
drifted. That could make local evidence pass while the deployed database
accepted a different operational-hold history.

## Root Cause

The schema-parity guard compared normalized table and index shapes but did not
assert the semantic contract implemented by database triggers and PostgreSQL
functions. Its self-test also lacked mutations proving that removal or
inversion of one safety invariant was detected.

## Fix Summary

The parity guard now extracts each operational-hold trigger and PostgreSQL
function by object name and requires exactly one correctly bound object. It
checks first-event state, exact predecessor revision and event links,
predecessor time ordering, released-to-released rejection, head/event/ref
foreign keys, insert/update projection links, actor/time links, monotonic
revision advancement, and event/head immutability.

The CI self-test mutates representative invariants in both dialects and
requires every drift to fail the semantic guard. An optional isolated
PostgreSQL regression replays migration `030_jobs_operational_holds.sql` twice
inside a rollback-only schema, verifies one ledger row, zero seeded event/head
rows, the exact function/trigger inventory, and then rolls back.

## Files Modified

| File | Change |
|------|--------|
| `jobs/scripts/check-jobs-schema-parity.mjs` | Add object-scoped operational-hold semantic requirements for SQLite and PostgreSQL. |
| `jobs/scripts/ci-guards-self-test.mjs` | Prove representative semantic mutations are rejected. |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | Add isolated, rollback-only PostgreSQL migration replay and inventory coverage. |
| `infra/{sqlite,postgres}/server-runtime/*jobs_operational_holds.sql` | Supply the paired trigger/function authority checked by the guard. |

## Edge Cases Handled

- A trigger with the correct name but wrong operation or statement-level
  binding fails parity.
- A PostgreSQL function that exists but omits one head/event projection link
  fails parity.
- First release, released-to-released transition, non-`n-1` ancestry, or
  backwards predecessor time drift fails parity.
- Missing foreign-key components, event-ref links, actor/time links, or delete
  immutability fails parity.
- Migration replay cannot manufacture a permissive event or head seed.

## How to Test

```bash
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/ci-guards-self-test.mjs
cargo test --manifest-path server/Cargo.toml \
  postgres_operational_hold_migration_replay_is_exact_and_seedless
```

## Known Limitations

- The PostgreSQL regression requires an explicitly authorized isolated URL in
  `BLUEY_TEST_POSTGRES_URL`; it self-skips when the variable is absent.
- Source parsing is a deterministic CI guard, not a substitute for an
  authorized live migration, backup/restore rehearsal, or production catalog
  inspection.
