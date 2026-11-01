# FIX-837: Managed run mismatch negative uses the wrong public contract

Preflight: bluey-ops/current source/local handoff. Test-contract correction only.

## Issue

Private review-first40249 exited1 at negative_wrong_run: actual409/unavailable in
32ms and client lease_unavailable, versus expected404/target-missing/request_failed.
No corrected matrix qualification follows from this failed execution.

## Root Cause

The case changes only run_id, retaining the original frozen managed workflow
request/release and an otherwise valid fresh signed proof. Active managed session
resolution validates the command/request run binding before target lookup. A run
mismatch is a managed authority conflict mapped to the exact409 unavailable
response. It is not the same as the separate wrong-application404 target case.
The previous static review conflated the missing-target and frozen-run boundaries.

## Fix Summary

Change only wrong_run to409/unavailable/lease_unavailable in the controller catalog
and separately pinned test fixture. Add an explicit both-mode regression rejecting
the former404/body/client-code combination. Preserve fresh correctly signed proof,
fresh worker nonce, unchanged other inputs,20-family equality and retained lease
tail. No production mapping, authorization predicate or replay guard is changed.

## Files Modified

- jobs/scripts/nonprod-runner-control-plane.mjs
- jobs/scripts/nonprod-runner-control-plane.test.mjs
- This record and Phase638 evidence records.

## Edge Cases Handled

Wrong application and wrong run have distinct exact contracts. No any4xx acceptance,
report-inferred mode, alias seed or change to trusted command material is allowed.
The failed case did not reach its after-snapshot or retained lease tail; prior
progress cannot be represented as a complete matrix pass.

## How to Test

Use COMMAND-PHASE-638-PREPARED-PROTOCOL-NEGATIVES.md only after independent review
of this mapping and all remaining cases, focused Node tests and prospective privacy.
Current-byte review-first and separate auto-submit matrix passes remain required.

Failed40249: dev build1m24s,774-source manifest
10e54a5d4efe4649378ab47b083c9e51ea5cfb713c878064e21cb4ef0ba7050c,
HEAD40949858/status matched beforeafter. API SHA
e9305cbc378505daaaf16283f209654c8c246f5584ed99fe4fd8c5d70f9f7569;
fixture SHA572e67ca06072be41ce6c38186250c7cf8eb45f0d7bd6c0b2c054024cc2fb66c.
Actual409 responseSHA
b0c0ade598bbb038c6314b2174ca12113b874d7fc692d5a669fd5777dce2ece5.
No panic/PG error/blocker; API/PG cleanup succeeded and exact root
run.1793227081.91253.2420471370 removed, parentempty. No runtime remains.

## Known Limitations

Subsequent83098(review-first) and50372(auto) both exited0 on exact source
e9591654d8b9b91d7c58976887579f655dd0bd38f043fccb299bec9422cb19a3;
all24cases/20families/tail/zeroeffects/cleanup passed in both.295 Node tests pass.
The command record has complete artifacts and limits; original40249 remains failed.

This corrects an inaccurate test expectation, not a production failure. Remaining
negative/recovery/full-source/native-cloud/provider/cost/release gates stay open.
