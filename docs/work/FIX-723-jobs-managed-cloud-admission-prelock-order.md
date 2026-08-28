# FIX-723: Managed-Cloud Claim And Submit Performed Work Before Combined Prelock

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against FIX-714, the Phase 614
> PostgreSQL lock-order guard, and the current managed-cloud claim/final-effect paths. The SSD
> archive was not used.

**Status:** Implemented; focused static regression and final-source compile/lint gates green

## Issue

The pre-final Rust library baseline exposed a real PostgreSQL ordering regression: managed-cloud
execution-lease claim performed protected work before the combined workflow-admission prelock. The
first correction advanced the guard to the same defect in managed-cloud final Submit.

## Root Cause

The common PostgreSQL paths acquired operational-hold and ATS locks around the managed branch even
though `lock_managed_cloud_workflow_admission_postgres_tx` already owns the combined managed
`H -> exclusive M -> ATS -> fleet` prelock. Managed claim/submit therefore duplicated authority
acquisition and did not make the combined prelock the first protected operation. Unmanaged
execution still needed its three explicit locks.

## Fix Summary

- For managed claim and final Submit, call the combined workflow-admission prelock as the first
  protected transaction operation.
- For unmanaged execution only, acquire explicit operational hold, shared managed registry, then
  ATS certification.
- After either branch completes its canonical prelock, acquire account discovery `D` and continue
  through the existing after-prelock resolver.
- Remove duplicate managed-path H/ATS acquisition without weakening any authority check.

## Files Modified

| File                                                              | Change                                                                                                                      |
| ----------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `server/src/db/jobs/execution_leases.rs`                          | Split managed/unmanaged claim and final-submit prelocks so each branch acquires one canonical authority sequence before `D` |
| `docs/work/FIX-723-jobs-managed-cloud-admission-prelock-order.md` | Record the structural lock-order defect, correction, and evidence limit                                                     |
| Phase 614 Round/IMPL/REVIEW/CHANGELOG                             | Include the correction while preserving the live-PostgreSQL blocker                                                         |

## Edge Cases Handled

- Managed claim and managed final Submit both use the combined
  `H -> exclusive M -> ATS -> fleet` prelock first.
- Unmanaged claim/submit retain explicit `H -> shared M -> ATS -> D` ordering.
- Managed paths do not reacquire H or ATS after the combined prelock.
- Resolution and mutation still occur only after the branch-specific prelock and account `D`.

## How To Test

```text
PostgreSQL protected-admission static regression      1 / 1 (0.00s final-source rerun)
Scoped diff check                                      passed
Global Rust fmt check                                  passed
Server cargo check --all-targets                       passed (34.06s)
Server strict Clippy, -D warnings                      passed (49.41s)
Full Rust library/all-target aggregate                PENDING post-fix rerun
Live PostgreSQL contention/failure injection          UNPROVEN; URL absent
```

The accepted `server/src/db/jobs/execution_leases.rs` SHA-256 is
`e0c95a4b29dc70991f2d386ba4dd4ef151da8aac83a1b6a5fa27934381ed15ce`.
The final regression source `server/src/db/jobs/tests.rs` SHA-256 is
`0f4775b375c755c63c6563b3885788681f7a45678102058f631dd68935bec325`.

## Known Limitations

- The regression is static source evidence; it does not prove deadlock freedom, lock wait behavior,
  or interruption handling on a live PostgreSQL server.
- This correction does not enable managed execution, source verification, or any production flag.
