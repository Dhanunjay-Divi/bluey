# FIX-701: PostgreSQL Track Heads Lacked SQLite Trigger Parity

> **Codex preflight:** Loaded `$bluey-ops` and compared the paired Phase 613 SQLite and PostgreSQL
> authority migrations and schema guard. No hosted migration, deploy, or production database was
> used.

## Issue

The PostgreSQL `jobs_track_policy_heads` table did not initially have the same insert, monotonic
update, and immutable-delete enforcement carried by the SQLite authority.

## Root Cause

Relational foreign keys and checks bound individual fields, but they could not prove that a new or
updated mutable head exactly matched its immutable transition, approved revision, receipt,
predecessor, identity, source resume, semantic generations, and time ordering. The first parity
guard checked table structure without requiring all PostgreSQL head functions and triggers.

## Fix Summary

- Add PostgreSQL functions that validate an initial head and enforce exact one-generation
  compare-and-swap updates.
- Bind those functions to insert/update triggers and protect current-head deletion with the
  tenant-aware immutable-evidence trigger.
- Require exact immutable transition, revision, approved receipt, predecessor, identity, resume,
  digest, generation, and timestamp matches.
- Extend schema parity and its mutation self-test to require the functions, trigger bindings, and
  critical invariants in both dialects.

## Files Modified

| File | Change |
|------|--------|
| `infra/postgres/server-runtime/034_jobs_canonical_taxonomy_authority.sql` | Add initial-head, monotonic-update, and immutable-delete trigger authority |
| `jobs/scripts/check-jobs-schema-parity.mjs` | Require PostgreSQL functions, triggers, and exact head invariants |
| `jobs/scripts/ci-guards-self-test.mjs` | Prove that head-trigger and predecessor-binding drift is rejected |

## Edge Cases Handled

- a non-generation-one initial head;
- an update that skips or reuses a generation;
- a mismatched predecessor transition or revision;
- a head that differs from its immutable event or approved receipt;
- cross-tenant identity/resume bindings; and
- direct deletion outside an authorized account/Track cascade.

## How to Test

```bash
node jobs/scripts/check-jobs-schema-parity.mjs
# Observed checkpoint: PASS, 81 tables / 74 indexes per dialect

node jobs/scripts/ci-guards-self-test.mjs

# Requires BLUEY_TEST_POSTGRES_URL for an isolated disposable database:
cargo test --manifest-path server/Cargo.toml --lib \
  postgres_canonical_track_policy_ledger_encrypts_and_rejects_projection_drift \
  -- --nocapture
```

The configured disposable-PostgreSQL canonical-ledger regression passed 1/1, and the broader
authority run passed 13 tests after the normal two-pass migration path. This is real local
PostgreSQL evidence in addition to static parity; hosted-catalog and interruption evidence remains
external.

## Known Limitations

- The local schema parser verifies checked-in SQL, not a hosted catalog after migration.
- Production migration, backup, rollback, and network-interruption evidence remains external.
