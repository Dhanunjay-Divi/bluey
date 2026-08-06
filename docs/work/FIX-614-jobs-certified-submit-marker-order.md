# FIX-614: Authorize Certified Submit Before Durable Irreversible State

> **Codex preflight:** Loaded `$bluey-ops` before diagnosis and verified its
> operating memory against the current repository state.

## Issue

Local and cloud execution could persist an irreversible submit marker or
checkpoint before server Phase B had successfully authorized the one-use
employer-facing submit.

## Root Cause

The local browser acquired its durable submit marker before calling the live
`authorize-submit` endpoint. The cloud runner similarly wrote
`final_submit_started` before `ActiveExecutionLease.beforeFinalSubmit()`
returned. Cloud failure cleanup also treated `finalSubmitAttempted` as proof of
authorization, so a denied, malformed, unavailable, expired, or timed-out Phase
B response could be converted into a durable `side_effect_unknown` checkpoint.

## Fix Summary

The local path now validates and consumes live Phase B authority before writing
its durable marker, rechecks near-expiry authority after the response, and only
then permits provider activation. The cloud path uses one explicit boundary
that awaits fenced Phase B authorization before writing
`final_submit_started`. Cloud cleanup retains an irreversible checkpoint only
when `finalSubmitAuthorized` is true; `finalSubmitAttempted` remains single-shot
retry protection but no longer manufactures local durable proof of approval.

## Files Modified

| File | Change |
|------|--------|
| `jobs/browser/src/authorized-final-submit.ts` | Moved live authorization ahead of the durable marker and made timeout signaling injectable for deterministic regression coverage. |
| `jobs/browser/src/run-controller.ts` | Documented the server-first marker and checkpoint boundary. |
| `jobs/browser/tests/authorized-final-submit.test.ts` | Added denial, network, malformed-response, expiry, timeout, activation, and success-order assertions. |
| `jobs/runner/src/certified-final-submit.ts` | Added the shared authorize-before-checkpoint boundary and successful-authorization checkpoint predicate. |
| `jobs/runner/src/server.ts` | Applied the boundary to cloud submit and removed failed Phase B checkpoints instead of marking them irreversible. |
| `jobs/runner/tests/execution-lease.test.ts` | Added real lease-client regressions for Phase B denial, network failure, malformed response, expiry-like denial, timeout, and success order. |

## Edge Cases Handled

- Explicit Phase B denial leaves no durable irreversible marker/checkpoint and
  never reaches submit activation.
- Network loss, malformed JSON, expiry-like denial, and timeout fail closed with
  the same zero-marker/checkpoint and zero-activation behavior.
- A capability expiring while local authorization is in flight is rejected by
  the post-response expiry recheck before the marker or activation.
- Successful Phase B authorization is observed before the marker/checkpoint,
  and provider activation is observed only after durable local state exists.
- A lost or rejected Phase B attempt remains single-shot without being treated
  as affirmative checkpoint authority.

## How to Test

```bash
cd jobs/browser
npm test -- tests/authorized-final-submit.test.ts
# 1 file passed; 18 tests passed
npm run typecheck

cd ../runner
npm test -- tests/execution-lease.test.ts
# 1 file passed; 27 tests passed
npm run typecheck
```

## Known Limitations

- These tests use deterministic local HTTP/fetch doubles; they do not activate
  a live ATS submit control or exercise a deployed server.
- Existing ticket/lease Phase B fencing remains covered by the server suite.
  Exact ATS binding consume, live certification recheck, layout membership, and
  canary reservation are separate Round 604 server-authority gates and are not
  claimed by this ordering fix.
