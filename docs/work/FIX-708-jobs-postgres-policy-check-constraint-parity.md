# FIX-708: PostgreSQL Policy CHECK Bounds Diverged From SQLite

> **Codex preflight:** Loaded `$bluey-ops` and compared the paired Phase 613 SQLite/PostgreSQL
> authority migrations, schema guard, and real PostgreSQL behavior. No hosted migration,
> deployment, or production database was used.

## Issue

PostgreSQL migration 034 did not express every generation, version, and timestamp bound with the
same non-negative or positive JavaScript-safe-integer range required by SQLite and the portal
contract.

## Root Cause

Some PostgreSQL checks were one-sided or incomplete while the SQLite migration used exact
`BETWEEN` bounds. Static table/index parity alone could therefore report the expected schema shape
without proving that PostgreSQL rejected representative out-of-range rows at the database boundary.

## Fix Summary

- Align activation, input-generation, policy-revision, receipt, and head integer bounds with the
  paired SQLite safe-integer contract.
- Require positive generations/versions and non-negative timestamps or predecessor generations,
  all capped at `9007199254740991` where applicable.
- Extend schema parity to require the critical PostgreSQL CHECK text and add mutation self-tests
  that weaken one bound at a time.
- Add a real PostgreSQL regression that resolves the exact catalog constraint name and asserts
  SQLSTATE `23514` for a negative timestamp, an above-safe-integer canonicalizer version, and a
  zero account-input generation.

## Files Modified

| File | Change |
|------|--------|
| `infra/postgres/server-runtime/034_jobs_canonical_taxonomy_authority.sql` | Mirror SQLite generation/version/timestamp CHECK bounds |
| `jobs/scripts/check-jobs-schema-parity.mjs` | Require the Phase 613 safe-integer CHECK contract |
| `jobs/scripts/ci-guards-self-test.mjs` | Prove weakened PostgreSQL bounds are rejected by the guard |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | Exercise representative constraints in a real isolated PostgreSQL schema |

## Edge Cases Handled

- a negative taxonomy activation timestamp;
- a canonicalizer schema version above JavaScript's maximum safe integer;
- a zero policy-revision account-input generation;
- a same-shaped table whose CHECK text is weaker than the paired migration; and
- identification of the exact failing table constraint rather than any generic SQL error.

## How to Test

```bash
node jobs/scripts/check-jobs-schema-parity.mjs
# Observed locally: PASS, 81 tables / 74 indexes per dialect

node jobs/scripts/ci-guards-self-test.mjs
# Observed locally: PASS

# Requires BLUEY_TEST_POSTGRES_URL to name an isolated disposable PostgreSQL database.
cargo test --manifest-path server/Cargo.toml --lib \
  postgres_local_authority_tests -- --nocapture
# Observed locally on PostgreSQL 17.10 + pgvector 0.8.3: 13 / 13
```

The PostgreSQL module ran after the normal migration runner completed both passes. Its disposable
cluster was removed afterward.

## Known Limitations

- The real PostgreSQL regression samples three critical constraint classes; the schema guard checks
  the broader source contract but is not a hosted post-migration catalog audit.
- Backup, rollback, network interruption, exact-tip CI, Docker/Linux, deployment, and production
  flags remain unproven external gates.
