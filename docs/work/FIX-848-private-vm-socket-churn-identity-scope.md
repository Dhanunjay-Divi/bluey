# FIX-848: Socket observation churn incorrectly blocked private VM cleanup

> **Codex preflight:** Load `$bluey-ops`; current Jobs checkout and evidence
> remain authoritative. No production or owner VM resource is in scope.

## Issue

Actual sixth VM attempt28269 failed `host_listener_snapshot_changed` at
`listener_observation` and then retained stopped disposable storage with
`cleanup_unproven_state_retained`. It is not a D0 pass or cleanup-proven run.

## Root Cause

The harness set `usernetIdentityAmbiguous` before both initial helper identity
validation and the entire unrelated all-host socket/native/socket bracket.
Ordinary socket churn could throw before any helper identity mismatch was
observed, leaving the sticky flag set. Cleanup correctly respected that flag
but unnecessarily retained the stopped private disks. The failed report did
not persist its original in-memory PID list, preventing later reconstruction
of the normal original-PID-absence proof.

## Fix Summary

Establish immutable usernet authority immediately after private artifact
capture, using two selected native snapshots, artifact revalidation and the
existing native helper binding. Only then restore the prior sticky state.
Listener collection is separate and retries at most three complete attempts,
only on exact `host_listener_snapshot_changed`. Each successful attempt still
requires exact socket rows, both native snapshots, artifact continuity and
the same immutable helper binding. Real artifact/native uncertainty stays
sticky; unknown/malformed/timeout failures are never retried or accepted.

## Files Modified

- `jobs/scripts/run-nonprod-managed-runner-vm-tests.mjs`: separate helper proof
  from socket observation, preserve sticky drift and bound exact-code retries.
- `jobs/scripts/nonprod-managed-runner-vm.test.mjs`: retry bounds, non-retry
  negatives and both ordered helper proof chains.

## Edge Cases Handled

- Three changing observations still fail; none becomes a success by timeout.
- Producer/parser/artifact/native/timeout errors fail on the first attempt.
- A previously sticky ambiguity is never cleared by a later matching capture.
- Final third socket equality remains mandatory inside the retried attempt.
- Baseline authority is assigned once after its own bounded stable capture.

## How to Test

Parent executed `npm --offline --prefix jobs run test:nonprod-managed-vm-harness`:
21 Node and31 Python tests passed. Initial stale source-order assertion and
duplicate test-variable errors were corrected before that pass. A fresh frozen
VM run, source review and cleanup observation remain required.

## Known Limitations

This does not approve arbitrary host traffic, remove strict listener policy,
establish peer purpose, qualify an image/browser/runner or prove production.
The sixth run remains failed. Its one-time storage recovery must use a separate
reviewed exact-root stopped-inventory/no-open-file procedure; it cannot invent
the unavailable original-PID proof or retroactively set `cleanupProven`.
