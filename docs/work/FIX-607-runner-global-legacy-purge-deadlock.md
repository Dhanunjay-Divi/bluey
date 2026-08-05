# FIX-607: Runner global-legacy purge deadlock

> **Codex preflight:** Loaded `$bluey-ops` and verified this fix in the isolated
> Round 602 worktree without touching production, credentials, or the
> meeting-owned checkout.

## Issue

A managed volume with classified legacy artifacts for more than one purge
subject could delete the first subject and then remain unable to reach the
later signed command that authorized deletion of the remaining subject.

## Root Cause

The client finalized each command as it arrived. Finalization correctly
required a globally empty classified-legacy inventory before signing an
acknowledgement, but the first command removed only its locator-authorized
target and then failed that global check. The command loop stopped on the
failure, so later command pages never ran and global zero was unreachable.

## Fix Summary

Purge is now a durable two-phase protocol. Phase A follows signed, volume-scoped
keyset pages and calls `prepare` for every command: it fences and quiesces the
subject, records immutable before evidence, deletes only authorized legacy
targets, and emits no tombstone or acknowledgement. Only after a complete pass
proves global legacy zero does phase B restart at the null cursor, re-prepare
pending commands, and finalize acknowledgements. Bounded cursor state resumes
across work-budget yields, and journal replay after a process restart uses the
target-only empty check needed to complete the barrier safely.

## Files Modified

| File | Change |
|------|--------|
| `jobs/runner/src/volume-purge.ts` | Split durable preparation from globally-zero finalization. |
| `jobs/runner/src/runner-volume-client.ts` | Add resumable prepare-all and finalize phases. |
| `server/src/{api,db}/jobs_runner_volumes.rs` | Add signed stable keyset command pagination. |
| `jobs/runner/tests/runner-volume-client.test.ts` | Prove three pages prepare before the first ACK. |
| `jobs/runner/tests/volume-purge.test.ts` | Prove restart between preparation and finalization. |

## Edge Cases Handled

- An ACK or supersession after a page is issued does not invalidate its cursor.
- Unknown or wrong-volume cursors fail closed.
- Cursor pages cannot report readiness; only an empty null-cursor poll can.
- A large backlog yields with bounded state instead of crashing or retaining
  every command in memory.
- Classified legacy without matching signed authority keeps work unavailable
  while leaving the control listener online.

## How to Test

```bash
cd jobs
npm test --workspace @bluey/jobs-runner -- \
  --run tests/runner-volume-client.test.ts tests/volume-purge.test.ts
npm test
npm run typecheck
```

## Known Limitations

- Live legacy-volume reconciliation and physical wipe evidence require
  separately authorized production operations and remain outside this source
  batch.
