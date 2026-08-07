# FIX-643: Communication Idempotency Omitted Source Authority

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

Reusing an idempotency key could return an existing action even when the source
message changed, and no single digest committed the account, application,
connection, source, provider, and payload together.

## Root Cause

The replay comparison covered the payload hash and several relationships but
omitted `source_message_id`; the queue had no immutable action-authority digest.

## Fix Summary

Bind replay to the exact source message and persist a canonical authority digest
used by approval, dispatch, completion, and reconciliation fencing.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs.rs` | Authority and lease contracts |
| `server/src/db/jobs/communication_actions.rs` | Canonical digest and replay checks |
| Paired communication-execution migrations | Durable authority projections |

## Edge Cases Handled

- Same key/different source, connection, provider, application, or payload;
  source-message deletion/change after draft creation; stale approval digest.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml communication_ --quiet
```

## Known Limitations

- Provider-side identity still requires the transport evidence in this batch.
