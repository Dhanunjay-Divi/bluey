# FIX-752 — Execution integrity-policy lock order

## Status

Implemented; an earlier configured PostgreSQL contention checkpoint was green but is superseded by
later source changes. Exact-tip static, configured PostgreSQL, and integrated evidence remain
pending.

## Issue

The PostgreSQL current-execution authority resolver could participate in a reachable deadlock
between the job-integrity publication fence and account-policy rows during queue or reservation
admission.

## Root Cause

`resolve_current_execution_authority_postgres_after_prelock` acquired account-policy inputs before
calling composed integrity resolution, which then acquired the job-integrity publication fence.
Workspace and approval paths acquire those same resources in the opposite order: integrity
publication fence before account policy. Concurrent execution and publication transactions could
therefore form a policy-to-integrity / integrity-to-policy lock cycle.

The first repair sampled database time after the integrity fence but before the account-policy and
conditional Auto-submit locks. A policy or authorization writer could therefore hold execution
across a signed source, ATS, or integrity expiry while the resolver continued with the stale
pre-wait time.

## Fix Summary

After the caller-owned `H -> M -> ATS -> D` prelock, the execution resolver now acquires the shared
job-integrity publication fence, locks account-policy inputs, validates the stored posting and
Career Track, and conditionally acquires the exact Auto-submit authority. Only after those locks are
held does it sample one PostgreSQL database time. It resolves composed source, ATS, and
job-integrity authority through bounded `..._after_prelock_at_ms` seams, so an effect path can reuse
one final post-lock scalar without reacquiring the common publication, policy, or advisory locks.
Exact subject rows may be re-read beneath those already-held fences.

The resulting order is:

`H -> M -> ATS -> D -> integrity publication fence -> account policy -> Auto-submit -> DB time`

SQLite does not use this PostgreSQL row-lock path and is unchanged.

## Files Modified

| File | Change |
| ---- | ------ |
| `server/src/db/jobs/execution_authority.rs` | Acquire integrity before policy and compose after the fence at one database-time sample. |
| `server/src/db/jobs/tests.rs` | Assert the production resolver's canonical order and absence of downstream relocks. |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | Exercise the reachable PostgreSQL contention order with a real publication-fence wait. |

## Edge Cases Handled

- The caller continues to own the complete `H -> M -> ATS -> D` prelock.
- Composed authority does not reacquire the job-integrity publication fence.
- Freshness is evaluated from one database-time sample taken after account policy and the exact
  conditional Auto-submit lock.
- The explicit at-ms resolver does not reacquire common or advisory authority locks; exact
  policy/source/ATS/integrity rows may be re-read beneath the already-held fences.
- A publication transaction can lock account policy while execution is waiting on integrity,
  preventing the former policy-to-integrity / integrity-to-policy cycle.
- Authority denial after the contention is released remains fail closed.

## Evidence

- **Pending — static source-contract test:**
  `postgres_auto_submit_execution_and_revocation_share_one_lock_order`.
- **Pending — configured PostgreSQL contention test:**
  `postgres_execution_authority_locks_integrity_before_account_policy`; requires
  `BLUEY_TEST_POSTGRES_URL`.
- **Superseded diagnostic checkpoint:** the configured contention test passed on the earlier local
  PostgreSQL 17 `r5` binary. Later fixture and FinalSubmit edits changed the source, so that result
  is not final-source release evidence and must be repeated in the fresh `r6` manifest.
- **Pending — integrated compile:** `CARGO_INCREMENTAL=0 cargo check --manifest-path
  server/Cargo.toml --tests` after the FIX-752 source change.

No exact-tip FIX-752 Cargo command is reported as passing. Root owns the serial Cargo lane and will
replace the pending slots only with exact observed results from the final frozen source.

## How to Test

```bash
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  postgres_auto_submit_execution_and_revocation_share_one_lock_order -- --nocapture

# Requires BLUEY_TEST_POSTGRES_URL to identify the configured test database.
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib \
  postgres_execution_authority_locks_integrity_before_account_policy -- --nocapture

CARGO_INCREMENTAL=0 cargo check --manifest-path server/Cargo.toml --tests
```

## Known Limitations

- Configured PostgreSQL contention evidence and final integrated gates remain pending.
- This fix does not replace the wider Phase 614B lock-order, interruption, or hosted-database
  evidence requirements.
