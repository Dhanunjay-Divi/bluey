# FIX-606: Runner storage-attestation ID flake

> **Codex preflight:** Loaded `$bluey-ops` and verified this fix in the isolated
> Round 602 worktree without touching production, credentials, or the
> meeting-owned checkout.

## Issue

The three-volume purge fault matrix could fail nondeterministically while a
runner created its signed current-storage attestation.

## Root Cause

`RunnerVolumeClient.submitStorageAttestation` used raw base64url entropy as the
attestation ID. Base64url permits `-` and `_` as the first character, while the
shared Node/Rust runner-identifier grammar requires an alphanumeric first
character. Roughly one in 32 generated IDs therefore failed its own strict
codec before it could be signed or submitted.

## Fix Summary

Prefix every generated ID with the fixed purpose string `att-`. The remaining
32 base64url characters still carry 192 bits of entropy, and the complete ID is
always canonical under the shared identifier grammar. The client regression
asserts the exact generated shape.

## Files Modified

| File | Change |
|------|--------|
| `jobs/runner/src/runner-volume-client.ts` | Mint canonical `att-<entropy>` IDs. |
| `jobs/runner/tests/runner-volume-client.test.ts` | Assert the generated ID grammar. |

## Edge Cases Handled

- Entropy beginning with either base64url punctuation character is valid after
  the fixed alphanumeric prefix.
- When predecessor, tombstone, enrollment, and local storage-evidence bindings
  remain exact, retry behavior reuses the cached signed attestation while
  minting a fresh outer authority proof. A committed body is promoted, changed
  tombstone bindings discard an obsolete body, and any other conflict fails
  closed as documented in `FIX-608`.

## How to Test

```bash
cd jobs/runner
npx vitest run tests/runner-volume-client.test.ts \
  tests/runner-volume-fault-matrix.test.ts
npm test
npm run typecheck
npm run build
```

## Known Limitations

- None.
