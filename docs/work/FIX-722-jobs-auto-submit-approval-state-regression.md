# FIX-722: Auto-Submit State Regression Bypassed Approved-Execution Prerequisite

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against FIX-718, the current
> Phase 614 application state machine, and the final library-test checkpoint. The SSD archive was
> not used.

**Status:** Implemented; focused regression green, full-library aggregate non-green

## Issue

`application_updates_enforce_state_machine_and_submission_mode` still expected local/cloud queue
capability and a direct `awaiting_review -> queued` transition for an Auto-submit application
without Phase 614B employer/risk authority or an attached `approved_execution` snapshot. FIX-718
intentionally made both decisions fail closed, so the stale expectations broke the full Rust
library aggregate.

## Root Cause

The transactional queue-admission implementation and its focused authority tests were updated, but
this older state-machine regression retained the pre-FIX-718 positive path. It therefore treated
the intended Review-first queue and approval denials as production failures and then exercised
invalid submission-mode handling from a state that should never have been reached.

## Fix Summary

- Assert Review-first preparation is available while local/cloud queue capability remains false for
  the explicit employer-identity/job-risk blockers.
- Assert that queueing without `approved_execution` fails.
- Exercise the invalid submission-mode case against the unchanged `awaiting_review` application.
- Assert that both denied mutations preserve the original `awaiting_review` and `review_first`
  values.
- Do not add a test bypass, relax queue authority, or fabricate an approval snapshot.

## Files Modified

| File                                                              | Change                                                                                                                  |
| ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------- |
| `server/src/db/jobs/tests.rs`                                     | Repin the state-machine regression to the transactional approved-execution prerequisite and assert zero-mutation denial |
| `docs/work/FIX-722-jobs-auto-submit-approval-state-regression.md` | Record the stale-fixture correction and its fail-closed boundary                                                        |
| Phase 614 Round/IMPL/REVIEW/CHANGELOG                             | Include the correction without treating the integration aggregate as green                                              |

## Edge Cases Handled

- A denied queue attempt cannot advance state.
- A rejected submission-mode mutation cannot alter the existing mode.
- The test does not use the missing Phase 614B employer/risk authority as a shortcut to a positive
  route.

## How To Test

```text
Focused application state-machine regression        1 / 1 (2.33s final-source rerun)
Pre-final Rust library baseline                     1,416 / 1,450; this failure repaired
```

The earlier exact focused checkpoint used `server/src/db/jobs/tests.rs` SHA-256
`d994cc328597f7f1f566cff67c6e2a45708f952b6ecb7a2ce33f8205b84391b7`.
The exact regression reran green on final current source SHA-256
`0f4775b375c755c63c6563b3885788681f7a45678102058f631dd68935bec325`.

## Known Limitations

- This is a regression-fixture correction; it does not make the known integration aggregate green.
- A production-representative positive queue route still requires confirmed sponsorship and the
  separately reviewed Phase 614B signed employer/risk authority.
