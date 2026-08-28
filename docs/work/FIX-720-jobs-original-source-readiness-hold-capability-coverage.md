# FIX-720: Jobs Readiness Test Omitted Original-Source Verification Capability

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against the current Phase 614
> source, operational-hold readiness contract, and frozen review scope. The SSD archive was not
> used.

**Status:** Implemented; focused regression green, full-library aggregate non-green

## Issue

The Jobs readiness unit test still expected seven operational capabilities after Phase 614 added
`OriginalSourceVerification` as the eighth concrete capability. The production readiness composer
returned the complete set, so the stale count assertion made the full Rust library aggregate fail.

## Root Cause

The Phase 614 operational-hold capability migration and readiness composition were updated, but the
aggregate cardinality assertion in
`api::jobs_operations::tests::readiness_composes_global_specific_and_native_blockers` was not
repinned. A count-only correction would not prove that the new capability inherited the intended
global hold without fabricating capability-specific or native blockers.

## Fix Summary

- Expect all eight concrete operational capabilities.
- Select the `OriginalSourceVerification` readiness entry explicitly.
- Assert that it inherits one global operational hold, has zero native blockers, and reports one
  combined blocker.
- Preserve the existing per-capability and native-blocker assertions for the other readiness rows.

## Files Modified

| File                                                                           | Change                                                                                                           |
| ------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------- |
| `server/src/api/jobs_operations.rs`                                            | Repin the capability cardinality and assert the exact original-source global/specific/native blocker composition |
| `docs/work/FIX-720-jobs-original-source-readiness-hold-capability-coverage.md` | Record the stale-fixture diagnosis, correction, and evidence boundary                                            |
| Phase 614 Round/IMPL/REVIEW/CHANGELOG                                          | Include the correction without promoting the release verdict                                                     |

## Edge Cases Handled

- A global hold applies to original-source verification even when no source-verification-specific
  hold exists.
- The new capability does not inherit an unrelated native blocker.
- Readiness remains non-ready while the inherited global blocker is active.

## How To Test

```text
Focused Jobs operations readiness regression        1 / 1 (0.00s final-source rerun)
Pre-final Rust library baseline                     1,416 / 1,450; this failure repaired
```

The final current `server/src/db/jobs/tests.rs` SHA-256 is
`0f4775b375c755c63c6563b3885788681f7a45678102058f631dd68935bec325`.

## Known Limitations

- This is a test-fixture correction; it does not change production readiness composition or enable
  source verification.
- It does not replace hosted operational-hold propagation, exact-tip CI, or production flag
  read-back evidence.
