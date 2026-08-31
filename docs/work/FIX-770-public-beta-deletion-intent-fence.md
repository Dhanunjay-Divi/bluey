# FIX-770: Deletion-pending accounts could consume or mutate beta authority

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

An account with a durable deletion intent could still consume an unreclaimable
public-beta slot through a GET-driven enrollment, receive an administrator
grant, or acquire a new denial-override mutation.

## Root Cause

The outer account middleware treats GET requests as read-only, while public
beta evaluation may atomically enroll. Eligibility checked account existence,
verification, and temporary status but did not inspect
`account_deletion_intents` inside that write transaction.

## Fix Summary

SQLite admission now checks the deletion intent inside its immediate
transaction. PostgreSQL locks the account row and checks the deletion intent in
the same transaction, matching the account-deletion lock boundary. Automatic
enrollment, administrator grant, and target override writes all return the
closed `AccountDeletionPending` error before changing the cohort counter or an
account-scoped beta row.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs_beta_access.rs` | Add paired transactional deletion-intent checks and race fixtures |
| `server/src/api/jobs_beta_access.rs` | Map the new internal error to closed public/admin projections |
| `docs/work/FIX-770-public-beta-deletion-intent-fence.md` | Record the root cause and proof boundary |

## Edge Cases Handled

- Deletion-first consumes no slot and creates no enrollment or override.
- A SQLite deletion transaction that wins the write race blocks later
  enrollment after serialization.
- Normal account deletion still removes account-scoped rows without decrementing
  historical cumulative capacity.
- Export remains readable while deletion is pending.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml jobs_beta_access::tests::sqlite_deletion
cargo test --manifest-path server/Cargo.toml \
  jobs_beta_access::tests::configured_postgres_enforces_exact_cap_and_serializes_account_deletion
```

## Known Limitations

- The configured PostgreSQL race requires an explicitly isolated
  `BLUEY_TEST_POSTGRES_URL`; it was not available in the local evidence pass.
