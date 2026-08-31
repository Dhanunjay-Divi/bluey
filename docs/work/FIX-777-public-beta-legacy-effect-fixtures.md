# FIX-777: Legacy effect fixtures bypassed the public-beta authority boundary

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against the Phase 621/622
> worktree. No deployment, production flag, provider, credential, or hosted state was changed.

## Issue

The full server unit suite exposed one runner-volume test that attempted a fresh effect without a
durable public-beta admission and three account-deletion tests that still expected lower storage
fences to determine the exact error after the newer public-beta deletion fence had already denied
the fresh effect.

## Root Cause

The Phase 621/622 authority composition deliberately places verified public-beta admission and
deletion intent before new external-effect authority. Older fixtures were written before that
boundary existed. One fixture therefore never created the admission required to reach the
runner-volume lease behavior it was intended to test. The deletion fixtures did enroll their
accounts, but asserted the former lower-layer error or mutation result even though the new
public-beta check now short-circuits first.

## Fix Summary

Added a test-only helper that creates one durable, verified public-beta enrollment in the exact
runner-volume fixture that needs to exercise post-admission lease behavior. Updated the three
deletion tests to assert the current fail-closed `Conflict` or `None` result while retaining their
existing state and capacity no-mutation proofs. Production code, lock order, authorization,
database migrations, API behavior, and release configuration are unchanged.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/runner_volume_purge.rs` | Add and use an exact test-only verified public-beta admission fixture |
| `server/src/db/jobs/tests.rs` | Align deletion-fenced effect expectations with the earlier public-beta boundary |
| `docs/work/FIX-777-public-beta-legacy-effect-fixtures.md` | Record the full-suite failures and bounded fixture correction |
| `CHANGELOG.md` | Record the test-fixture correction under Unreleased |

## Edge Cases Handled

- The runner-volume success and failure paths both enter through the same durable admission
  contract before testing lease/residency behavior.
- Deletion intent still prevents every fresh external effect even when a lower storage fence would
  also deny the operation.
- Existing assertions continue to prove that denied claims, submissions, profile writes, and
  capacity counters do not mutate.
- The helper is available only to tests and does not read environment configuration or alter
  production defaults.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --lib \
  runner_volume_execution_lease_claim_is_atomic_with_residency_binding
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --lib \
  execution_lease_claim_is_fenced_before_account_child_mutation
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --lib \
  account_deletion_fence_blocks_active_lease_and_profile_operations
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --lib \
  account_deletion_fence_blocks_local_launch_and_jobs_mutations
```

## Known Limitations

- This fix repairs test setup and expectations; it does not replace the exact-tip full Rust suite,
  hosted PostgreSQL deletion canary, or live runner-volume verification required before release.
