# FIX-584: Unknown Employer Submission Reconciliation

> **Codex preflight:** Loaded `$bluey-ops` and verified its production flags,
> PostgreSQL authority, receipt, metering, and no-blind-retry invariants against
> the current repository before implementation.

## Issue

If a browser clicked an employer's final Submit control and then lost the
response, Bluey correctly marked the result `side_effect_unknown`, but there was
no complete owner-facing path to resolve a confirmed non-submission and the
local and cloud late-receipt behavior was not proven equivalent.

## Root Cause

The runner could create the terminal unknown state, but `server/src/api/jobs.rs`
and `server/src/db/jobs/customer_data.rs` only supported the ordinary receipt
finalization path. There was no account-scoped reconciliation transaction, no
bounded capability exception for a delayed trusted local receipt, and one cloud
browser-state mapping did not use the intervention state expected by recovery.

## Fix Summary

- Added an authenticated, account-scoped owner action that accepts only an
  explicit `not_submitted` confirmation.
- Added one transactional reconciliation function for PostgreSQL and SQLite.
- Releases the exact attempt and execution authority without granting another
  automatic retry.
- Preserves an immutable reconciliation receipt and makes repeat confirmation
  idempotent.
- Allows a trusted runner receipt to win only while the exact unknown attempt is
  inside the 24-hour reconciliation window.
- Maps all unknown cloud browser sessions to `needs_input` with a clear current
  step and maps authority races to HTTP 409.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs.rs` | Owner route, late receipt handling, cloud state mapping |
| `server/src/api/jobs_local_capability.rs` | Bounded reconciliation verification |
| `server/src/db/jobs.rs` | Shared grace constant and reconciliation module |
| `server/src/db/jobs/submission_reconciliation.rs` | Atomic owner reconciliation |
| `server/src/db/jobs/customer_data.rs` | Fenced local/cloud late receipt finalization |
| `server/src/db/jobs/tests.rs` | Real worker-state rollback fixture |
| `server/tests/integration_e2e.rs` | Local and cloud fault/recovery tests |
| `jobs/portal/src/*` | Owner confirmation API and accessible dialog |

## Edge Cases Handled

- Repeated owner confirmation is idempotent.
- A late trusted receipt and owner confirmation serialize on the application.
- Submitted applications can never be reverted by the owner action.
- Ordinary expired capabilities remain rejected.
- Reconciliation capabilities expire after 24 hours.
- Cross-account applications remain indistinguishable/not found.
- Browser, attempt, lease/ticket, application, and receipt changes commit or
  roll back together.

## How to Test

```bash
cd jobs/portal
npm test -- src/api.test.ts src/views/ApplicationsView.test.ts
npm run typecheck
npm run build

cd ../../server
cargo test --test integration_e2e jobs_local_side_effect_unknown_is_terminal_and_requires_reconciliation
cargo test --test integration_e2e jobs_cloud_side_effect_unknown_can_be_reconciled_not_submitted
cargo test --test integration_e2e jobs_cloud_side_effect_unknown_accepts_late_trusted_receipt
cargo test --test integration_e2e jobs_
cargo test --lib jobs
```

## Known Limitations

- This certifies Bluey's state machine and recovery authority. It does not by
  itself certify a real employer tenant or enable Browser distribution.
