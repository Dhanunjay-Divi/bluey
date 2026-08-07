# FIX-645: Communication Failure Could Retry Without Fresh Review

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

The queue automatically reclaimed `failed` actions, while expired dispatches
became unknown without a dedicated provider lookup authority. A weak failure
classification could therefore permit an unintended duplicate write.

## Root Cause

Claim selected both `approved` and `failed`; completion accepted a string
outcome and arbitrary nonempty provider object ID; no append-only request-start
or reconciliation record existed.

## Fix Summary

Make approval the only dispatchable state, persist a fenced request-start before
network I/O, require structured provider evidence, classify ambiguous writes as
terminal unknown, bind the complete post-marker provider future to the exact
lease deadline, and add separate read-only reconciliation. Confirmed absence
returns to fresh review and never directly retries; a suspended stale attempt is
never polled after its deadline.

## Files Modified

| File | Change |
|------|--------|
| Communication DB state and migrations | Attempts, evidence, reconciliation, fences |
| `server/src/jobs_communication_dispatch/` | Disabled provider write/lookup runtime |
| Focused fault and provider fixture tests | Ambiguity and restart matrices |

## Edge Cases Handled

- Timeout, connection loss, 5xx, 409, malformed 2xx, lease expiry, process
  suspension/restart, stale fence, duplicate callback, lookup conflict, and
  bounded absence.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml communication_ --quiet
cargo test --manifest-path server/Cargo.toml jobs_communication_dispatch --quiet
```

## Known Limitations

- Live provider outage/recovery remains an authorized sandbox gate.
