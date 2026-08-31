# FIX-699: PostgreSQL Auto-Submit Revocation Could Race Final Execution

> **Codex preflight:** Loaded `$bluey-ops` and reconciled Auto-submit mutation and final
> execution against the current Phase 613 authority worktree. No hosted database, provider,
> deployment, production flag, or employer-facing effect was used.

## Issue

A PostgreSQL Auto-submit revocation could commit concurrently with execution validation for the
same account and Career Track, allowing the validator to act on a snapshot that was not serialized
with the revocation.

## Root Cause

Policy-input locking stabilized the profile, preferences, identity, resume, facts, and Track, but
it did not serialize the mutable Auto-submit authorization row. Revocation and authorization used
separate transactions, while final execution read the active authorization without a shared lock
in the same per-Track namespace.

## Fix Summary

- Add one transaction-scoped PostgreSQL advisory-lock namespace derived from account and Track.
- Take that lock exclusively while authorizing or revoking Auto-submit and shared while reading
  current authorization or validating an Auto-submit execution.
- Preserve one lock order: discovery account, account policy inputs, then Track Auto-submit
  authority, before row reads or mutation.
- Read the active execution authorization `FOR SHARE` after acquiring the shared fence.
- Add source-order and real-PostgreSQL two-connection regressions for both race directions.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/auto_submit.rs` | Add shared/exclusive Track authorization locks and use them for read, authorize, and revoke transactions |
| `server/src/db/jobs/execution_authority.rs` | Hold the shared Auto-submit lock through final PostgreSQL execution validation |
| `server/src/db/jobs/tests.rs` | Assert the common namespace and lock order in each source path |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | Exercise revocation versus execution with two PostgreSQL transactions |

## Edge Cases Handled

- revocation starting while execution holds the shared authority;
- execution validation starting while revocation holds the exclusive authority;
- a review-first application that does not need the Auto-submit lock; and
- consistent ordering with the broader discovery-account and policy-input fences.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  postgres_auto_submit_execution_and_revocation_share_one_lock_order

# Requires BLUEY_TEST_POSTGRES_URL for an isolated disposable database:
cargo test --manifest-path server/Cargo.toml --lib \
  postgres_auto_submit_revocation_and_execution_share_one_fence -- --nocapture
```

The configured disposable-PostgreSQL regression
`postgres_auto_submit_revocation_and_execution_share_one_fence` passed 1/1 without self-skip; the
broader local PostgreSQL authority run passed 13 tests after the normal two-pass migration path.
The clean fmt, all-target check/Clippy, and 1,517-test all-target Rust command also passed with zero
failures or ignored tests.

## Known Limitations

- The fence prevents a new effect from crossing revocation; it does not reinterpret a provider
  effect that may already have happened.
- Hosted PostgreSQL/network behavior and production enablement remain external gates.
