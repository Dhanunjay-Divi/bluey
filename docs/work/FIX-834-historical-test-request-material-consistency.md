# FIX-834: Keep historical test request and frozen material IDs consistent

Preflight:bluey-ops; exact Phase638 private run6759 and source inspection.

## Issue

The first FIX832 qualification run6759 stopped in the first13-test filter:
9passed/4failed/zeroignored. All four failures occurred in shared pure fixture
setup at execution_session_authority.rs:894, before a recovery assertion or API run.

## Root Cause

historical_fixture changed the command and envelope request_id to a canonical
wfreq-v2 ID, then immediately validated historical material. The START material
still carried start()'s old result_request_id. The strict production material
validator correctly returned Conflict. A later assignment was too late.

## Fix Summary

Only cfg(test) fixture code changes: assign START material.result_request_id with
command/envelope IDs before the first validation; remove the redundant later
assignment. RESUME already uses its new current.request_id; explicitly validate
post-branch material consistency for either form. No production check is weakened.

## Files Modified

server/src/db/jobs/execution_session_authority.rs, cfg(test) fixture only.

## Edge Cases Handled

START and RESUME material must each agree with their immutable request envelope.
Existing cross-scope/packet/event/owner/token/fence and strict replay checks remain.

## How to Test

Rerun COMMAND-PHASE-638-CHECKPOINT-RECOVERY-GREEN.md only after review/privacy.
Frozen corrected file SHAb6852a50a42e728da11752786be221a7aa0b2d4b8d4725bb0233be150829a423.
Scoped rustfmt/diff checks pass; no corrected Rust/API result is yet claimed.

Actual failed6759 build2m55s; subsequent5DB/3fixture/2example/API gates were NOT
reached. PG shutdown/root cleanup passed,parentempty/38GiBfree.774-source manifest
f7226ec3ea1ef4c7d2c5778449e465f26fa5b3e33d508623943daa4193d07027 and
HEAD409498589b8d2e62b5ef54ebdee1372aeb8d005f/scopedstatus matched before/after.
Privacy2927tracked/2651text passed treeed76e80109265b8f776d108476e088d42237a41c
before failure. Node186 passes remain validator scope, not recovery qualification.

## Known Limitations

The FIX832 production recovery/dual-mode API/full source gates remain unqualified.
This repair does not by itself prove historical authority or safe mutation.
