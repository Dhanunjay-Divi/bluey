# FIX-709: Some Portal Authority Mutations Did Not Advance The Refresh Epoch

> **Codex preflight:** Loaded `$bluey-ops` and reconciled portal refresh ordering with Auto-submit
> authorization, revocation, and Career Track deletion in the current Phase 613 source. No OAuth,
> external account, application, deployment, or production flag was used.

## Issue

A workspace refresh started before Auto-submit authorization, Auto-submit revocation, or Track
deletion could finish afterward and reinstall stale pre-mutation workspace state in the portal.

## Root Cause

Profile, preference, resume, identity, and Track writes used the shared authority-sensitive
mutation epoch, but the three callbacks updated local state directly after their API calls. They
therefore did not advance the epoch, participate in coalesced authoritative read-back, or reject an
older refresh token.

## Fix Summary

- Route Auto-submit authorize, Auto-submit revoke, and Track deletion through
  `runWorkspacePolicyAuthorityMutation`.
- Advance the shared authority epoch before each request and reject any earlier refresh result.
- Apply the mutation result locally while the common reconciler waits for one current workspace
  read-back after overlapping writes settle.
- Remove a deleted Track's local authorization and detach its matches while awaiting server truth.
- Preserve preview behavior without performing a network request.

## Files Modified

| File | Change |
|------|--------|
| `jobs/portal/src/App.tsx` | Put authorize/revoke/delete in the shared authority mutation epoch and read-back flow |
| `jobs/portal/src/App.test.ts` | Add stale-refresh regressions for all three operations |

## Edge Cases Handled

- a pre-authorization refresh returning after the new authorization;
- a pre-revocation refresh restoring the removed authorization;
- a pre-deletion refresh restoring the deleted Track or authorization;
- matches that referenced a locally deleted Track; and
- the same ordering behavior in public preview without external I/O.

## How to Test

```bash
(cd jobs && npm run test --workspace @bluey/jobs-portal -- src/App.test.ts)
# Observed locally: 20 / 20

(cd jobs && npm run test --workspace @bluey/jobs-portal)
# Observed locally: 349 / 349 across 28 files
```

## Known Limitations

- Portal epoch fencing is presentation defense in depth. The server and database remain the only
  Auto-submit and execution authority.
- These local tests do not prove a deployed browser bundle, exact-tip CI, Docker/Linux, hosted
  behavior, deployment, or production-flag state.
