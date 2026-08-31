# FIX-707: Timestamp-Looking Career Fact Keys Could Be Dropped From Policy Semantics

> **Codex preflight:** Loaded `$bluey-ops` and reconciled Career Fact persistence with the Phase 613
> account semantic digest. No customer fact, hosted database, deployment, or production flag was
> used.

## Issue

A user-owned `CareerFact.value` object could use a legitimate key such as `created_at_ms` or
`confirmedAtMs`. Changing only that value could leave the account semantic digest unchanged and
allow an old reviewed Track policy to appear current.

## Root Cause

Transport-field stripping was applied by JSON key name to the arbitrary fact value. That confused
relational Career Fact envelope timestamps with user-owned semantic keys inside `value`, collapsing
distinct fact values onto the same canonical policy input.

## Fix Summary

- Exclude actual relational transport timestamps in the query that constructs the semantic fact.
- Parse and retain the complete arbitrary `CareerFact.value` JSON without stripping reserved-looking
  keys from it.
- Continue binding fact ID, category, label, value, source, verification state, confirmer, and schema
  version into the account semantic digest.
- Add a regression proving camelCase and snake_case timestamp-looking keys advance the account
  generation, require review, and invalidate the old policy ledger.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/taxonomy_policy.rs` | Preserve arbitrary Career Fact value keys and add semantic-generation regression coverage |

## Edge Cases Handled

- `createdAtMs` and `created_at_ms` inside a fact value;
- `updatedAtMs` and `updated_at_ms` inside a fact value;
- `confirmedAtMs` and `confirmed_at_ms` inside a fact value;
- unchanged relational envelope timestamps with changed user-owned JSON; and
- an old approved Track policy evaluated after the fact semantic generation advances.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  reserved_looking_fact_value_keys_advance_semantics_and_revoke_old_policy
```

The named regression is present in the current source. No final Rust all-target result is
attributed to this record.

## Known Limitations

- This correction distinguishes known relational transport fields from arbitrary fact values; it
  does not define application semantics for every possible customer JSON schema.
- Exact-tip CI, Docker/Linux, hosted-database, deployment, and production-flag evidence remain
  separate gates.
