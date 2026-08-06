# FIX-620: Local Phase-B response loss was treated as retryable

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

A local certified submit could commit server Phase B and then lose or reject
the response before writing its local irreversible marker. The browser reported
that state as an ordinary expired launch and could discard recovery context.

## Root Cause

`authorizeFinalSubmit` mapped transport loss, timeout, malformed successful
responses, and invalid certified response bodies to `launch_expired`. The local
failure classifier treats that code as definitively failed when no marker is
present, even though the server may already have consumed the one-use binding
and reserved canary capacity.

## Fix Summary

Ambiguity after the Phase-B request is now `submit_outcome_unknown`. A bounded
4xx response remains an explicit pre-marker denial; 5xx and other non-4xx
responses remain ambiguous because an intermediary may emit them after the
server commits Phase B. Once a successful Phase-B body is received,
capability-expiry or marker-write failure is also terminal uncertainty. This
preserves the page and checkpoint, prevents blind retry, and routes
reconciliation through the server's frozen terminal authority.

## Files Modified

| File | Change |
|------|--------|
| `jobs/browser/src/authorized-final-submit.ts` | Separates explicit denial from ambiguous Phase-B outcomes and retains returned certified receipt authority before local marker work. |
| `jobs/browser/tests/authorized-final-submit.test.ts` | Covers response loss, timeout, malformed success, post-commit expiry, and marker-write failure as non-retryable uncertainty. |

## Edge Cases Handled

- A definite 4xx authorization denial still produces no marker and remains a
  normal launch failure.
- HTTP 5xx and non-4xx failures preserve the page and recovery checkpoint and
  cannot create automatic retry authority.
- Invalid success JSON and a malformed certified receipt projection cannot
  grant a click, but they also cannot manufacture retry authority.
- Failure to create the local marker after server success preserves the
  recovery classification even when the run directory disappeared.

## How to Test

```bash
npm test --workspace @bluey/jobs-browser -- authorized-final-submit.test.ts
```

## Known Limitations

- Exact server commit state after transport loss remains intentionally unknown;
  reconciliation, rather than automatic retry, resolves it.
