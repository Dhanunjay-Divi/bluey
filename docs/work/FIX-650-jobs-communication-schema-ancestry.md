# FIX-650: Communication Schema Allowed Cross-Ancestry Rows

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

Independent foreign keys could admit a syntactically valid communication row
whose account, application, connection, source message, provider, action, or
attempt belonged to different authority lineages. SQLite and PostgreSQL also
did not enforce every provider-object control-character rule identically.

## Root Cause

The first paired migrations checked the referenced rows independently instead
of enforcing the complete composite ancestry at each root and append-only child
boundary.

## Fix Summary

Add equivalent dialect constraints and triggers for one exact account,
application, connection, source, provider, action, and attempt ancestry. Bind
attempt/evidence/reconciliation descendants through composite keys, preserve
repeatable identical absence observations without weakening their lineage, and
apply the same bounded/control-character rules in SQLite and PostgreSQL. Add a
positive action revision initialized at one and incremented atomically on every
persisted action transition so clients never infer order from wall-clock
milliseconds. Bound it to JavaScript's exact integer range and reserve enough
headroom before dispatch for every bounded reconciliation claim/finish cycle.

## Files Modified

| File | Change |
|------|--------|
| `infra/sqlite/server-runtime/051_jobs_communication_execution.sql` | Composite lineage and strict text triggers |
| `infra/postgres/server-runtime/029_jobs_communication_execution.sql` | Matching lineage constraints and triggers |
| `jobs/scripts/check-jobs-schema-parity.mjs` | Paired migration contract checks |
| Server database tests | Cross-account/provider tamper and dialect-parity cases |

## Edge Cases Handled

- Cross-account application/connection substitution, wrong-provider source,
  wrong-action attempt, wrong-attempt evidence, control bytes, and repeated
  identical authoritative absence observations, timestamp/revision overflow,
  and exhausted reconciliation headroom.

## How to Test

```bash
node jobs/scripts/check-jobs-schema-parity.mjs
cargo test --manifest-path server/Cargo.toml communication_ --quiet
```

## Known Limitations

- Live PostgreSQL migration and multi-process contention remain external gates.
