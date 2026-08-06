# FIX-655: Deletion And Disconnect Did Not Persist A Draining Fence

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

When dispatch already held the irreversible lease, account deletion or mailbox
disconnect returned a waiting response without persisting intent. Other
approved actions could therefore claim new leases while deletion or disconnect
was pending, creating a moving lifecycle target.

## Root Cause

The lifecycle guard counted unresolved communication actions but represented
that result as a transient refusal rather than durable draining authority.
Cancellation and claim paths consequently had no state to fence later work.

## Fix Summary

Persist account- and connection-scoped draining intent before reporting that
irreversible communication is still unresolved. Block new draft creation,
approval, cancellation races, OAuth grant changes, mailbox sync writes, and
dispatch claims while preserving exact completion and read-only reconciliation
for already-started attempts. Require current, unexpired exact leases for
completion and reconciliation transitions, and advance the action revision for
disconnect-driven cancellation exactly as for every other persisted mutation.

## Files Modified

| File | Change |
|------|--------|
| Paired communication/account migrations | Durable account/connection drain state |
| `server/src/db/account_data.rs` | Account deletion intent and waiting lifecycle |
| Communication/mailbox database modules | Claim, disconnect, cancel, and finish fences |
| Lifecycle concurrency tests | Claim-wins and deletion/disconnect-wins races |

## Edge Cases Handled

- Claim wins before deletion, claim wins before disconnect, second approved
  action during drain, expired lease completion, cancellation during deletion,
  disconnect/reconnect races, legacy failed cleanup, unknown outcome, and exact
  reconciliation after a drain begins. The cross-account reauthorization
  regression fixture creates the second active account before testing tenant
  isolation, so the assertion reaches the account write fence instead of
  failing on a nonexistent account.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml deletion --quiet
cargo test --manifest-path server/Cargo.toml disconnect --quiet
cargo test --manifest-path server/Cargo.toml communication_ --quiet
```

## Known Limitations

- Final production retention and operator escalation timelines require owner
  and data-rights approval.
