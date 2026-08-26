# FIX-697: Portal Mutations Could Leave Stale Auto-Submit Readiness Visible

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the portal's Career Track, resume,
> preference, identity, Auto-submit, and Command Center projections against the current server
> authority. No OAuth connection, external message, application, deployment, or flag was used.

## Issue

After a policy-relevant portal mutation, the local workspace could continue displaying an approved
Career Track and active Auto-submit authorization until a later refresh, even though the server
had invalidated that authority.

## Root Cause

Profile saves, resume import, Jobs-preference saves, and application-identity update/verify/delete callbacks
patched only the changed object in React state. They neither downgraded dependent Track authority
nor awaited exact `/workspace` read-back before showing success. A failed read-back therefore left
optimistic stale authority in memory.

Settings could also launch its preference and profile saves concurrently. Even after adding
read-back, two racing mutation/reconciliation cycles could install an older workspace after a newer
one. An unrelated polling/manual refresh that started before the mutation could likewise finish
after the fail-closed downgrade and restore an old approved/active projection. An authorization
whose persisted status was `active` could still render as enabled when the current Track policy
predicate had already failed.

Settings and the Command Center also used weaker readiness proxies such as an active Track, a
verified identity, or a resume reference instead of validating the complete server policy
projection. The creation UI now assigns a client UUID before persistence, but the save toast still
treated every nonempty ID as an existing Track.

## Fix Summary

- Add one shared reconciler that immediately marks affected approved Track projections
  `needs_review` and downgrades active Auto-submit authorizations before awaiting server read-back.
- Invalidate every Track for Jobs-preference, source-resume, or application-identity changes;
  identity semantics participate in the account-wide policy generation.
- Apply the same all-Track invalidation to ordinary Career Profile saves and serialize the Settings
  preference/profile writes so their authoritative read-backs cannot overtake one another.
- Advance a monotonic authority epoch at mutation start, reject refresh/read-back installs from an
  older epoch, and coalesce overlapping authority-sensitive writes into one current read-back only
  after every write settles.
- Await and install the exact authenticated `/workspace` result before resolving the mutation and
  showing its success toast. If read-back fails, preserve the local fail-closed downgrade.
- Use the same behavior in preview without a network write or read-back.
- Add a shared readiness predicate requiring an approved state, no review reasons, positive
  activation/input/head generations, the account and Track semantic digests (including
  `account_input_semantic_sha256`), complete IDs and transition/receipt/head digests, and an exact
  current source-resume and application-identity binding.
- Use that predicate in Settings and the Command Center; incomplete, legacy, or stale projections
  require action and cannot enable Auto-submit.
- Render a stored `active` Auto-submit authorization as `needs_review` whenever the current Track
  predicate fails.
- Determine create/update toast copy from workspace membership rather than the now-stable client
  UUID.

## Files Modified

| File | Change |
|------|--------|
| `jobs/portal/src/App.tsx` | Fail closed after profile/resume/preference/identity changes, await read-back, and correct Track toast semantics |
| `jobs/portal/src/App.test.ts` | Cover all-Track, stale-refresh, overlapping-write, failed/awaited read-back, and create/update behavior |
| `jobs/portal/src/{api,api.test}.ts` | Map the server identity authority exactly and send the Track identity binding on writes |
| `jobs/portal/src/lib/track-policy-authority.ts` | Centralize complete approved-projection validation |
| `jobs/portal/src/lib/track-policy-authority.test.ts` | Reject partial, stale, malformed, or source-mismatched authority |
| `jobs/portal/src/lib/{command-center,command-center.test}.ts` | Report ready only when every active Track has complete current authority |
| `jobs/portal/src/views/SettingsView.tsx` | Serialize policy writes, show exact revision state, and render/gate Auto-submit conservatively |
| `jobs/portal/src/types.ts` | Carry activation, input-generation, identity, revision, receipt, and head evidence |

## Edge Cases Handled

- a preference or source-resume change affecting every Track;
- an ordinary Career Profile edit affecting every Track;
- any identity mutation advancing account semantics and therefore invalidating every Track;
- an active Auto-submit authorization that must become `needs_review` immediately;
- a workspace read-back that rejects, times out, or returns only after the caller would otherwise
  show success;
- a legacy authority, missing receipt, zero generation, malformed digest, head mismatch, review
  reason, or source-resume mismatch;
- all active Tracks approved versus a mixed ready/stale workspace;
- a pre-mutation refresh completing after a write, overlapping Settings writes, stale read-back
  completion, and stale persisted `active` presentation;
- missing or mismatched exact Track/application-identity authority;
- preview behavior without an external request; and
- a new Track with a nonempty client UUID that must still say `started`, not `updated`.

## How to Test

Observed on the latest focused portal checkpoint:

```bash
(cd jobs && npm run test --workspace @bluey/jobs-portal -- \
  src/App.test.ts src/api.test.ts src/lib/canonical-taxonomy.test.ts \
  src/lib/track-policy-authority.test.ts src/lib/command-center.test.ts \
  src/views/SettingsView.test.ts)
# PASS: 70 tests across 6 files

(cd jobs && npm run typecheck --workspace @bluey/jobs-portal)
# PASS

(cd jobs && npm run test --workspace @bluey/jobs-portal)
# PASS: 349 tests across 28 files; the App regression group passed 20 tests
```

The current portal production build processed 2,299 modules with only the greater-than-500-kB
advisory, and the aggregate Vitest result was 1,847 passing with one conditional Playwright skip.
The clean all-target Rust command also passed 1,517 tests with zero failures or ignored tests.
Exact-tip CI remains pending.

## Known Limitations

- The portal predicate is presentation defense in depth; the Rust server and database remain the
  only queue, Auto-submit, and execution authority.
- The reconciler intentionally makes state temporarily conservative while `/workspace` reloads.
- This fix does not authorize an OAuth provider, send a communication, submit an application,
  deploy the portal, or change a production flag.
