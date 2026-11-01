# FIX-836: Prepared negative matrix misclassifies consumed proof replay

Preflight: bluey-ops/current source/local handoff. Test-contract correction only.

## Issue

Private review-first55887 exited1 at negative_proof_consumed_replay: actual409
with exact public body Execution lease is not available., versus expected401
Runner-volume authority is invalid. This failed run is not matrix qualification.

## Root Cause

The review conflated initial read-only proof verification with transactional
proof consumption. A correctly signed proof from the successful claim still
passes verification. In consume_runner_volume_authority_postgres_tx, duplicate
volume/epoch/request insertion returns RunnerVolumePurgeError::Unauthorized.
The existing execution_lease_from_runner_volume_error deliberately maps that to
ExecutionLeaseError::Conflict; execution_lease_error emits409/unavailable. The
initial cryptographic verifier's401 mapper does not own this later boundary.
No production security or replay defect is demonstrated by this result.

## Fix Summary

Change only the exact consumed-proof case in the controller catalog and separately
pinned test fixture to409 and SHA256 of the exact unavailable body. Keep fresh
worker nonce, original consumed proof,20-family equality and later retained-valid
lease checks. Other proof signature/payload and alternate-worker negatives still
require401. Never accept an arbitrary4xx or change production mapping to suit a test.

## Files Modified

- jobs/scripts/nonprod-runner-control-plane.mjs
- jobs/scripts/nonprod-runner-control-plane.test.mjs
- This record and Phase638 evidence records.

## Edge Cases Handled

Do not confuse transport nonce replay401 with consumed volume proof409. Keep
operation-specific body hashes, actual HTTP count and mode-bound report validation.
Earlier cases progressing before the failure do not qualify the complete matrix;
the failed case's after-snapshot and retained lease tail were not reached.

## How to Test

Run the focused Node report tests, independent review, complete prospective-index
privacy/whitespace, then the same fresh review-first private-PG command in
COMMAND-PHASE-638-PREPARED-PROTOCOL-NEGATIVES.md. Auto remains separately required.
No corrected runtime pass exists at creation of this record. Current294 focused
Node report tests pass/zero skips, including rejecting the old401/public-body
combination explicitly in both modes. These are not actual API/PG passes.

Failed55887: dev build1m25s,774-source manifest
59b955b1434baf9739f97b137306e43a09d5535b947ddc36d92590758eb237de,
HEAD40949858/status unchanged beforeafter. API SHA
20d4c72a8749fcbb9c36b2ffb6e04734dddd7349045520a368e40f2cd586acbe;
fixture SHA48b3c3cfa432ba2c501dba4e02bd7a7bbae8b7c431eacf8eaa5db2c71cbb5322.
Observed409 responseSHA
b0c0ade598bbb038c6314b2174ca12113b874d7fc692d5a669fd5777dce2ece5;
1171ms. No panic/PG error/blocker, API+PG cleanup succeeded; exact ownedroot
run.1793226796.88501.17483151990 removed, parentempty,37GiBfree. No runtime remains.

## Known Limitations

Subsequent83098(review-first) and50372(auto) both exited0 on exact source
e9591654d8b9b91d7c58976887579f655dd0bd38f043fccb299bec9422cb19a3;
all24cases/20families/tail/zeroeffects/cleanup passed in both.295 Node tests pass.
The command record has complete artifacts and limits; original55887 remains failed.

No production changes, browser/employer/Temporal, real application/mail or release
acceptance. Broader Phase638 gates remain open.
