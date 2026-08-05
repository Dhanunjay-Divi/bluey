# FIX-608: Runner storage-attestation ambiguity

> **Codex preflight:** Loaded `$bluey-ops` and verified this fix in the isolated
> Round 602 worktree without touching production, credentials, or the
> meeting-owned checkout.

## Issue

An unresolved current-storage attestation could be replayed after an
intervening purge changed the signed storage snapshot, while a lost
post-purge successor could also be rejected when the server still considered
its predecessor current.

## Root Cause

The client cached an ambiguous signed body without binding it to a local
storage-evidence revision. It therefore lacked a safe distinction between an
exact retry, a server-committed response loss, and a stale snapshot after
mutation. The first reconciliation fix also incorrectly required the server's
`storageAttestationRequired` hint for exact-base retry, even though local
post-purge staleness legitimately requires a successor while the predecessor
remains server-current.

## Fix Summary

Every storage mutation advances a local evidence revision, and no mutation may
begin until a pending attestation is resolved. Poll evidence now drives four
explicit outcomes: an exact successor generation and SHA promotes a committed
body; an unchanged predecessor, enrollment, tombstone binding, and local
revision retries the byte-identical body with a fresh outer proof; changed
tombstone bindings discard a transactionally obsolete body; and any other
predecessor, enrollment, or revision conflict fails closed. Exact-base retry is
independent of the server-required hint, preserving safe recovery for a lost
post-purge successor.

## Files Modified

| File | Change |
|------|--------|
| `jobs/runner/src/runner-volume-client.ts` | Add evidence revisions and exact pending-body reconciliation. |
| `jobs/runner/tests/runner-volume-client.test.ts` | Cover commit loss, pre-commit loss, mutation, tombstone change, and required-false successor retry. |

## Edge Cases Handled

- A response lost after commit is promoted without resubmitting the body.
- A response lost before commit retries the exact signed body and never mints a
  conflicting successor for the same predecessor.
- A purge cannot begin while an unresolved signed snapshot is ambiguous.
- Changed tombstone bindings produce a new bound body rather than replaying
  stale evidence.
- An unchanged current predecessor permits exact retry even when the server
  does not request an attestation, because local evidence is known stale.

## How to Test

```bash
cd jobs
npm test --workspace @bluey/jobs-runner -- \
  --run tests/runner-volume-client.test.ts tests/volume-purge.test.ts
npm test
npm run typecheck
```

## Known Limitations

- This protocol proves software-observed current storage and exact replay; it
  does not distinguish a powered-off sequential clone without provider, KMS,
  TPM, or equivalent managed-resource attestation.
