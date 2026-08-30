# FIX-728: Local Browser Claim Prelock Parity

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and verify its memory
> against the final Phase 614 record, Round 614B, and the current repository state.

**Severity:** P1 production route; P2 standalone-helper parity

**Status:** Implemented; mapped focused regressions present; frozen-source aggregate evidence pending

## Issue

Before this correction, the production local-browser claim route split PostgreSQL acquisition
between outer and inner helpers. Its effective order was `H -> ATS -> D -> M`, which inverted the
common managed/ATS/account order and omitted the Phase 614B integrity snapshot before mutation.

The real route already enforced `RunnerClaim` operational holds in SQLite and PostgreSQL; that
protection had to remain intact. The standalone helper in `server/src/db/jobs/local_runner.rs` was
not the production route, but its incomplete lock/hold behavior could mislead tests or future
callers and required semantic parity as a secondary P2 correction.

## Root Cause

The browser-release claim path split its authority acquisition across outer and inner helpers.
Each helper checked a useful subset, but their composed order did not match the canonical prelock
chain and no single caller-owned snapshot covered managed release, ATS, account state, and signed
job integrity before claim mutation.

## Required Fix

The implemented correction:

- make the production route acquire one caller-owned prelock chain
  `H -> M -> ATS -> D -> integrity control FOR SHARE -> exact integrity head FOR SHARE` before any
  claim, lease, capacity, or runner-binding mutation;
- preserve the existing `RunnerClaim` hold check in both SQLite and PostgreSQL;
- prevent inner helpers from reacquiring `M`, ATS, `D`, or integrity locks in a conflicting order;
- recompare the exact current Phase 614 source and Phase 614B integrity receipt/head before claim;
- make revocation, expiry, drift, hold, or lock failure leave claim and runner state unchanged;
- retain exact success and idempotent replay semantics; and
- bring `local_runner.rs:320-422` to the same full hold/current-authority behavior for semantic
  parity without misrepresenting it as the production route.

## Files Modified

| File                                              | Change                                                           |
| ------------------------------------------------- | ---------------------------------------------------------------- |
| `server/src/db/jobs/browser_release_authority.rs` | Use one prelocked production claim chain and current authority    |
| `server/src/db/jobs/local_runner.rs`              | Bring standalone claim/submit helper to signed-domain parity      |
| `server/src/db/jobs/ats_certification_authority.rs` | Add after-prelock ATS helpers that do not reacquire ATS          |
| `server/src/db/jobs/operational_holds.rs`         | Consume typed signed employer-domain authority                    |
| `server/src/db/jobs/tests.rs`                     | Cover hold denial, success/replay, and submitted-domain integrity |

## Edge Cases To Handle

- Integrity or managed authority is revoked after plan creation but before local claim.
- A `RunnerClaim` hold appears before the mutation and denies with zero claim/lease/capacity change.
- ATS certification or source material changes after approval.
- Two local browser runners race for the same application and one replay arrives after success.
- The production wrapper and standalone helper receive the same stale authority and return the same
  fail-closed semantic result.
- An inner helper cannot silently reacquire `M` after `D` or bypass the caller-owned snapshot.

## How To Test

Mapped focused evidence in the current source:

```text
postgres_claim_locks_effect_rows_before_final_database_time_and_never_relocks PRESENT
standalone_local_claim_uses_signed_domain_after_one_postgres_prelock          PRESENT
fix_728_ats_after_prelock_phase_helpers_never_reacquire_ats                   PRESENT
fix_728_submitted_domain_requires_an_authentic_final_receipt_envelope         PRESENT
fix_728_employer_domain_runner_claim_holds_leave_local_and_cloud_unmodified   PRESENT
local_release_claim_is_atomic_exactly_replayable_and_revocation_fences_submit PRESENT
postgres_local_run_claim_and_submit_recheck_current_authority                 PRESENT; ENV-GATED
Frozen-source aggregate and configured PostgreSQL evidence                    PENDING
```

`PRESENT` records mapped regression coverage only. Live/configured PostgreSQL evidence is not
portable until it is rerun and recorded against the final source manifest.

## Known Limitations

- Standalone-helper parity is defense against future misuse; it is not substitute evidence for the
  actual API/browser-release production route.
- Live PostgreSQL behavior remains unproven until actually exercised.
- This fix does not enable local Browser distribution, deploy, or perform a provider write.
