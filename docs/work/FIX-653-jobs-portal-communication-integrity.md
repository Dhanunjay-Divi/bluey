# FIX-653: Portal Review Could Drift From Server Communication Authority

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

The portal could render or transition a payload without independently proving
the canonical UTF-8 digest, accept stale/equal-version polling conflicts, infer
a reply recipient, or navigate to a loosely validated OAuth authorization URL.

## Root Cause

The first review surface trusted structurally decoded API fields but lacked a
cross-language canonical hash implementation, exact transition reconciliation,
server-provided reply target, and provider-specific authorization-URL policy.

## Fix Summary

Verify the exact canonical payload hash before render, approval, cancellation,
or reconciliation; require exact source reply target; use a monotonic action
revision for persisted state; accept only readiness changes at an equal
revision; reconcile poll/detail/cancel results; reject Unicode display-control
spoofing; and allow only exact Google/Microsoft authorization hosts, paths,
query fields, callbacks, state, PKCE, and scopes. Preview remains a zero-network
action. Rust and TypeScript consume the same Unicode/escape/array/integer hash
vectors. Approval and cancellation submit the exact reviewed action revision
and payload hash for a server-side transactional compare-and-swap.

## Files Modified

| File | Change |
|------|--------|
| `jobs/portal/src/lib/communication-actions.ts` | Canonical hashing, revision, readiness, and display-control policy |
| Shared communication hash fixture | Cross-language canonical bytes and digests |
| `jobs/portal/src/lib/mailbox-oauth.ts` | Exact provider authorization URL validation |
| `jobs/portal/src/{api,types}.ts` | Runtime envelope and source-target contracts |
| `jobs/portal/src/views/ApplicationsView.tsx` | Conflict-safe review and transitions |
| Portal tests | Unicode hashes, redirects, races, and zero-call preview |
| Jobs/release CI workflows | Fail when a clean portal build changes `web/jobs` |

## Edge Cases Handled

- Unicode/emoji payloads, escaped whitespace, arrays, large safe integers,
  wrong recipient, same-revision conflict, readiness-only changes, stale poll,
  loading lineage drift, bidi/display controls, lookalike OAuth host, userinfo,
  port, fragment, scope, callback, state, and PKCE tampering.

## How to Test

```bash
npm test --prefix jobs --workspace @bluey/jobs-portal
npm run typecheck --prefix jobs --workspace @bluey/jobs-portal
```

## Known Limitations

- Provider authorization remains disabled until its external release gate is
  separately approved.
