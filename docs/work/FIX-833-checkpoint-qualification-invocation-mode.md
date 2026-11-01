# FIX-833: Bind checkpoint qualification mode to its invocation

Preflight:bluey-ops, current Phase638 source and independent source review.

## Issue

Independent review found a P2 false-green in the new, unexecuted checkpoint green
test: its validator selected review-first versus auto-submit requirements from
report.claim.submissionMode rather than the expected launch mode.

## Root Cause

Both flags shared a validator/report path. A structurally valid review-first report
could satisfy the outer auto-submit invocation without auto consent/ATS evidence
and its seventh child process. The controller internally derived mode from argv,
but that did not bind the acceptance boundary itself.

## Fix Summary

Require an explicit expectedSubmissionMode argument. Validate its closed two-value
domain and exact report mode before choosing required authority fields/process
count. Both inner controller and independent outer launcher pass expectations
derived from argv, never from the report. Unknown/missing/cross-mode reports deny.

## Files Modified

Private Node controller/validator, report tests and launcher only. No production
mode, feature flag, price, consent or lease authority changed.

## Edge Cases Handled

Review-first-as-auto, auto-as-review-first, missing and unknown expectations fail.
Auto still requires its ordinary consent/frozen ATS proof and7 clean processes;
review-first requires zero ATS and6. Existing causal-red/prepared modes stay strict.

## How to Test

node --test jobs/scripts/nonprod-runner-control-plane.test.mjs:186 passed,0failed,
0skipped, including the new cross-mode/missing/unknown expectation regression.
Independent re-review and actual checkpoint green API qualification remain pending.

## Known Limitations

This is a test acceptance-boundary correction, not actual recovery or release
evidence. FIX832 backend/inspector/focused/PG/full-source gates remain required.
