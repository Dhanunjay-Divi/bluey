# FIX-626: Serialize ATS Intervention and Pre-Click Lock Order

> **Codex preflight:** Loaded `$bluey-ops` and inspected only the current
> PostgreSQL intervention and irreversible-submit transactions.

## Issue

A PostgreSQL employer-answer intervention could lock the application before the
execution lease and ATS binding while cloud Phase B locked those rows in the
opposite order, allowing a lease/application deadlock cycle.

## Root Cause

The intervention transaction predated ATS certification and discovered its run
scope after acquiring the application row. It neither took the ATS advisory
lock nor followed the shared pre-click row-lock hierarchy.

## Fix Summary

- Acquire the global ATS advisory lock before certification-related row locks.
- Lock lease/ticket, binding, application, and intervention state in the same
  order used by pre-click execution.
- Revalidate the discovered run/account/application scope after locking before
  invalidation or packet mutation.
- Add source-order and focused intervention regressions; keep the optional live
  PostgreSQL contention run environment-gated.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/customer_data.rs` | Apply the global PostgreSQL lock hierarchy and scope revalidation. |
| `server/src/db/jobs/tests.rs` | Guard lock order and certified intervention behavior. |
| `docs/work/FIX-626-jobs-ats-intervention-lock-order.md` | Record the concurrency defect and repair. |

## Edge Cases Handled

- Local-ticket and cloud-lease interventions follow one deterministic order.
- Stale or changed run scope fails before employer-facing packet mutation.
- Runs at or beyond the irreversible marker remain reconciliation-only.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml intervention_answer
cargo test --manifest-path server/Cargo.toml postgres_ats_lock
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
```

## Known Limitations

- A real concurrent PostgreSQL contention exercise requires an authorized
  `BLUEY_TEST_POSTGRES_URL` and remains parked when it is unavailable.
