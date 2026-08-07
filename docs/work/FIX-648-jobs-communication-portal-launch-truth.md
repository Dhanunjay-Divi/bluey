# FIX-648: Portal Had No Reviewed Communication Launch Truth

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

Reviewed communication drafts had public APIs but no portal review controls and
no server-authoritative indication that provider execution was unavailable.
Inferring readiness from a connected inbox would overstate a read-only grant.

## Root Cause

The portal had no action contracts, decoders, API client, status model, or exact
draft review surface; public action responses omitted execution readiness.

## Fix Summary

Add privacy-safe summaries, on-demand strict detail decoding, immutable reply and
calendar review, explicit acknowledgement, truthful approval/cancel controls,
unknown-outcome warnings, and server-owned availability/reason fields. Approval
never says “send now” and fails closed while release flags or grants are absent.

## Files Modified

| File | Change |
|------|--------|
| `jobs/portal/src/{types,api}.ts` | Strict action contracts and endpoints |
| `jobs/portal/src/lib/communication-actions.ts` | Decoder and presentation policy |
| `jobs/portal/src/views/ApplicationsView.tsx` | Reviewed action UI |
| Portal tests/styles/bundle | Interaction and launch-truth coverage |

## Edge Cases Handled

- Unknown fields/status/provider, summary/detail mismatch, missing time zone,
  unavailable worker, needs-input reapproval, unknown outcome, terminal actions.

## How to Test

```bash
npm test --prefix jobs --workspace @bluey/jobs-portal
npm run typecheck --prefix jobs --workspace @bluey/jobs-portal
npm run build --prefix jobs --workspace @bluey/jobs-portal
```

## Known Limitations

- The explicit write-consent controls are present but fail closed while the
  server-side write-consent release gate is disabled. Provider certification,
  approved redirect URIs, and live canaries remain external work.
