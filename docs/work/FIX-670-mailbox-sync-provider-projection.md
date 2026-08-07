# FIX-670: Reject Mailbox Sync Provider Projection Drift

> **Codex preflight:** Loaded `$bluey-ops` and verified the finding in the
> current Phase 606 worktree. No archive, mailbox credential, provider request,
> live tenant, or production system was used.

## Issue

Mailbox sync admission could derive an operational-hold scope from sync state
without proving that it still matched the canonical mailbox connection and its
encrypted projection.

## Root Cause

The direct and batch claim paths treated the relational sync provider as the
only provider identity. A corrupted or stale row could therefore evaluate the
wrong mailbox-provider hold before a lease was written.

## Fix Summary

Direct and batch SQLite/PostgreSQL claim paths now load the canonical mailbox
connection provider, validate the relational and encrypted sync projections,
and use the canonical provider for hold evaluation. Their guarded updates
recheck the same provider and connected state; any mismatch or malformed
projection fails closed before mutation.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/mailbox_sync.rs` | Validate all provider projections, recheck canonical connection authority during lease writes, and cover direct/batch mismatches in module tests. |

## Edge Cases Handled

- Relational sync provider differs from the connection provider.
- Encrypted sync JSON differs from either relational projection.
- Malformed encrypted state is rejected before mutation.
- PostgreSQL compare-and-update counts fail closed.
- A held provider still cannot starve a later eligible provider.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  relational_provider_mismatch_fails_closed_before_direct_or_batch_lease
cargo test --manifest-path server/Cargo.toml \
  encrypted_provider_mismatch_fails_closed_before_direct_or_batch_lease
```

## Known Limitations

- No real Gmail or Outlook credential or provider request was used.
