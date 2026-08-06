# FIX-649: Provider Success Proof Did Not Close The Exact Source Boundary

> **Codex preflight:** Loaded `$bluey-ops` and verified its Jobs production
> invariants against the current repository state.

## Issue

A structurally valid provider object identifier could be accepted as delivery
success without proving the exact provider request marker, source thread or
conversation, payload, and attempt. Reconciliation evidence could also be
mistaken for evidence from another attempt.

## Root Cause

The first execution path used a provider-neutral evidence check. It did not
require each transport's deterministic marker fields or bind every observation
to the exact attempt and provider operation key.

## Fix Summary

Require provider-specific success proof for direct completion and lookup
reconciliation. Gmail verifies the sent message by exact read-back identity;
Outlook binds the immutable source and returned message identity; Google and
Microsoft calendar evidence binds their deterministic event or transaction
marker. Every observation commits hashes for the action, payload, and exact
attempt operation key, and raw operation keys are rejected.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/communication_actions.rs` | Exact provider proof and attempt-bound evidence |
| `server/src/jobs_communication_dispatch/` | Provider request, read-back, and lookup evidence |
| Communication dispatch/provider tests | Success, mismatch, ambiguity, and replay fixtures |

## Edge Cases Handled

- Wrong Gmail thread/message, Outlook conversation drift, calendar marker
  mismatch, stale attempt evidence, response loss, malformed success, and raw
  provider operation-key disclosure.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml jobs_communication_dispatch --quiet
cargo test --manifest-path server/Cargo.toml communication_ --quiet
```

## Known Limitations

- Authorized live-provider read-back and outage matrices remain external gates.
