# FIX-847: Preserve private usernet identity ambiguity through cleanup

Codex preflight: `bluey-ops`; no production/runtime authority is established here.

## Issue and root cause

Independent review found a pre-launch P1 in the draft usernet binding. Cleanup
recaptured helper artifacts only when `usernetPidRecorded` wasfalse. If a later
live revalidation observed replacement PID/socket identity but the original PID
was already recorded, cleanup could ignore the replacement and delete state
after checking only the original PID. No VM was run on that draft.

## Fix Summary

Remove that Boolean shortcut. Preserve the original complete artifact identity
and a sticky ambiguity flag. Mark validation in progress before reading live
identity; exact success restores only the prior ambiguity value. A previously
observed failure therefore cannot be cleared by restoring old files later.
Native helper binding is covered by the same pending-validation fence.

Before stopping, cleanup always recaptures current helper artifacts and records
any valid current PID. It compares with the original authority. Any drift/error
blocks destructive deletion, while bounded exact-profile stop/force-stop still
runs and all recorded/current PIDs must be absent. Normal post-stop PID/socket
removal is not misclassified as live identity drift: the continuity observation
happens before shutdown; socket-path absence is verified after owned removal.

The draft artifact uniqueness key also incorrectly included its label. Parent
corrected uniqueness to device/inode alone; the existing alias negative now
passes. Labels remain included in the distinct artifact-evidence hash.

## Files Modified and tests

`jobs/scripts/run-nonprod-managed-runner-vm-tests.mjs` and its dedicated test.
Parent20 Node tests pass, including replacement PID/hash/inode continuity,
sticky prior ambiguity, socket aliases, native selection/bracketing and bounds.
Independent rereview and actual usernet lifecycle qualification remain pending.

## Limitations

This does not accept new UDP, relax management TCP, qualify a runner image,
start an application workflow or change a production flag.
