# FIX-807: ATS Phase B serializable waiter retains a stale snapshot

> **Codex preflight:** `bluey-ops`, current source, Round636 and separate AI review.

## Issue

Private PostgreSQL session78617 passed7/8 cases, then the signed-canary race
returned one success and one Storage/SQLSTATE40001 rather than one success and
one actual capacity denial. No employer action occurred. The failing assertion
and zero-loser-effects requirements are retained unchanged.

## Root Cause

The standalone pool-owning `validate_consume_reserve_ats_application_certification`
opens SERIALIZABLE before acquiring its transaction-scoped advisory ATS lock.
The advisory SELECT establishes the waiter's snapshot before the winner commits;
after acquiring the lock, the waiter can retain stale capacity and be aborted
by PostgreSQL's serializable dependency checks. This wrapper predates Phase636.
The new attempt indexes do not fix that transaction snapshot lifecycle.

## Fix Summary

Keep SERIALIZABLE. The standalone wrapper retries its entire local database
transaction at most three total attempts, with a fresh snapshot and lock each
time, only for a real driver `postgres::Error` with SQLSTATE40001. Dropping the
failed transaction precedes the next attempt. Capacity is reread; the error is
never relabeled as capacity. All other storage/domain failures return unchanged.
Caller-owned context/after-prelock transactions are not retried here.

This follows [PostgreSQL17 serialization-failure guidance](https://www.postgresql.org/docs/17/mvcc-serialization-failure-handling.html):
retry the whole transaction rather than a failed statement. This boundary does
not perform a provider, object-storage, mailbox or employer side effect.

## Files Modified

`server/src/db/jobs/ats_certification_authority.rs`: narrowly scoped retry,
strict typed classifier, untyped/ambiguous/domain negative tests and actual
private-PG generated-error bounds/classification coverage.

## Edge Cases Handled

- Exhaustion preserves the genuine storage failure after three attempts.
- Error text containing40001 is not sufficient; unique violations and lost
  connections are not retried.
- A fresh attempt must actually observe capacity to return CapacityUnavailable.
- Existing winner count, reservation count and denied binding assertions remain.

## How to Test

Use the canonical isolated launcher/private-PG wrapper, selecting the existing
`postgres_concurrent_canary_phase_b_enforces_signed_concurrency_cap_atomically`
and new `postgres_phase_b_local_transaction_retries_only_typed_40001_and_is_bounded`
cases, plus the pure `phase_b_local_transaction_never_retries_untyped_or_domain_errors`
case and strict all-target Clippy. Session97759 passed both actual private-PG
cases, but later stopped on a separate quota fixture before Clippy. The direct
generated-error test proves classification/bounds/domain propagation, not an
actual capacity reread. The canary race provides that production-body proof;
its successor now waits for two observed advisory-lock waiters before release
to force overlapping snapshots. This later source still needs execution.
Session78617 remains a failure, not post-fix evidence.

## Known Limitations

No universal retry policy, caller-owned transaction retry, other ATS wrapper
change, new external action, deployment or production readiness claim. High
contention can exhaust the closed retry bound and continues to fail closed.
