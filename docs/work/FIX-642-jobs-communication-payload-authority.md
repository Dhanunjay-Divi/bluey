# FIX-642: Communication Payload Authority Was Not Closed

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

Communication drafts accepted any recognized kind and provider independently,
permitted unknown payload fields, and did not require an explicit calendar time
zone. An invalid pair or hidden extra field could therefore survive review.

## Root Cause

`server/src/db/jobs/communication_actions.rs` normalized kind, provider, and a
small required-field subset but had no closed per-provider payload contract.

## Fix Summary

Enforce exact reply/calendar provider pairs, closed bounded payload schemas,
header-injection rejection, explicit calendar time zone, and matching portal
decoding. Malformed legacy actions remain non-executable.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/communication_actions.rs` | Exact provider/payload validation |
| `jobs/portal/src/lib/communication-actions.ts` | Fail-closed public decoder |
| Focused server/portal tests | Positive and negative contract matrices |

## Edge Cases Handled

- Kind/provider mismatch, extra fields, CR/LF or Unicode display-control
  headers, invalid attendees, reversed or overlong time windows, unsafe
  surrogate/separator text, and missing/invalid time zone.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml communication_ --quiet
npm test --prefix jobs --workspace @bluey/jobs-portal -- communication-actions
```

## Known Limitations

- Existing malformed drafts are review-only and must be cancelled/recreated.
