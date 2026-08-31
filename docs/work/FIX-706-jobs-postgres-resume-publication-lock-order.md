# FIX-706: PostgreSQL Resume Publication Could Deadlock With Account Deletion

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the Phase 613 resume,
> object-lifecycle, policy-input, and account-deletion paths in the current worktree. No hosted
> database, object store, deployment, release activation, or production flag was used.

## Issue

PostgreSQL resume publication could acquire policy-input child locks before the parent account
deletion fence, while account deletion acquired the parent first. A concurrent logical-object lock
could complete the cycle, deadlock the operations, or obscure which operation won.

## Root Cause

`save_resume_source_asset_internal` took the discovery-account advisory fence and then locked
policy-input child rows before publishing the reserved object. Object publication later acquired
the logical-object fence and active-account write fence. Account deletion starts from the parent
account row before inspecting upload and policy children, so the two paths did not share one
parent-before-child order.

## Fix Summary

- Keep the discovery-account advisory fence as the outer policy-write boundary.
- Publish a reserved object, or explicitly require the active-account write fence for legacy
  publication, before locking policy-input child rows.
- Preserve the logical-object-before-account order already used by upload reservation.
- Fail reservation or publication with `AccountDeleting` when a committed deletion intent wins.
- Add a two-connection PostgreSQL regression covering both reservation-versus-deletion and
  publication-versus-deletion ordering.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/resume_assets.rs` | Acquire the object/account lifecycle fence before PostgreSQL policy-input child locks |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | Prove reservation/publication serialize with deletion and fail closed after deletion wins |

## Edge Cases Handled

- deletion owns the parent account while a new resume upload reservation begins;
- publication waits on the resume logical-object fence while deletion begins;
- deletion commits while reservation or publication is blocked;
- publication without a new upload still validates the active-account write fence; and
- the losing operation returns a typed deletion error without creating or publishing new state.

## How to Test

```bash
# Requires BLUEY_TEST_POSTGRES_URL to name an isolated disposable PostgreSQL database.
cargo test --manifest-path server/Cargo.toml --lib \
  postgres_resume_publication_locks_account_before_policy_children -- --nocapture

cargo test --manifest-path server/Cargo.toml --lib \
  postgres_local_authority_tests -- --nocapture
```

The fresh local PostgreSQL 17.10 plus pgvector 0.8.3 authority-module run passed 13/13 tests after
the normal migration runner completed both passes. The disposable cluster was removed afterward.

## Known Limitations

- The regression exercises local database lock and deletion-intent behavior, not object-store
  network loss or a hosted PostgreSQL interruption.
- Exact-tip CI, Docker/Linux, deployment, and production-flag evidence remain separate gates.
