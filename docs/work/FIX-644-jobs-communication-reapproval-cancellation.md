# FIX-644: Communication Reapproval And Cancellation State Drift

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

The documented `approved -> cancelled` transition was rejected, and a
needs-input action could retain a prior approval timestamp without a monotonic
revision proving fresh user authority.

## Root Cause

Approval and cancellation shared one transition query limited to
`awaiting_approval` and `needs_input`, while approval used `COALESCE` and had no
revision or provider-grant binding.

## Fix Summary

Allow cancellation only before dispatch, add monotonic approval revisions, bind
each approval to the exact action and provider grant, require the portal to send
the reviewed action revision and payload hash, compare-and-swap both inside the
transition transaction, and require fresh review after a proved no-side-effect
reconciliation. Approval derives one effective timestamp greater than the prior
audit timestamp and uses it for both `approved_at_ms` and `updated_at_ms`, while
`next_attempt_at_ms` retains the current wall-clock scheduling time so a
backdated audit row cannot postpone otherwise eligible dispatch.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/communication_actions.rs` | Serialized transitions and revisions |
| `server/src/api/jobs_communication_actions.rs` | Server-authoritative availability |
| Portal reviewed-action UI/tests | Fresh acknowledgement and safe cancellation |

## Edge Cases Handled

- Cancel approved or needs-input; reject dispatching/unknown/terminal cancel;
  stale cross-tab reapproval, payload/revision mismatch, concurrent
  approve/cancel, grant rotation, multiple transitions in one millisecond,
  backdated wall-clock input, and persisted timestamps ahead of wall clock.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml communication_ --quiet
npm test --prefix jobs --workspace @bluey/jobs-portal -- ApplicationsView
```

## Known Limitations

- Provider delivery remains release-disabled pending external certification.
