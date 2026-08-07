# FIX-673: Fail Closed on Incomplete Curated Materialization

> **Codex preflight:** Loaded `$bluey-ops` and verified the persistence gap in
> the current Phase 606 worktree. No archive, external feed, credential, live
> tenant, or production system was used.

## Issue

A crash between posting persistence and membership persistence could leave a
durable `curated_feed:*` posting without the managed discovery authority that
operational admission expects.

## Root Cause

Global candidate materialization persists the posting and managed membership
in separate transactions. Application/job operational context derived
discovery authority only from memberships and did not reject a curated posting
when that required membership was absent.

## Fix Summary

All SQLite/PostgreSQL application and job context builders now require every
curated materialized posting to have the exact account-level managed source:
provider `curated_feed`, source key `bluey-curated-v1`, and empty Track binding.
An orphan fails closed before queue, runner, submit, or paid-generation
authority. A normal retry may repair the membership, after which admission
context includes the exact source scope.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/operational_holds.rs` | Require exact managed curated membership in all application/job context paths. |
| `server/src/db/jobs/tests.rs` | Prove orphan denial and exact-authority repair for job and application contexts. |

## Edge Cases Handled

- No membership, unrelated membership, wrong provider, wrong source key, or
  Track-bound source cannot authorize a curated posting.
- Both job-scoped paid work and application-scoped execution fail closed.
- Repair adds the exact discovery-source hold scope.
- Ordinary non-curated postings are unaffected.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  curated_posting_without_managed_membership_fails_closed_until_repaired
```

## Known Limitations

- The two writes remain separately retryable; the new invariant makes their
  crash gap safe rather than claiming atomic materialization.
- The named regression directly exercises SQLite helpers. PostgreSQL code is
  compiled and source-audited, but live PostgreSQL evidence remains parked
  without an authorized test URL.
