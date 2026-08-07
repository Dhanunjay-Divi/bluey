# FIX-666: Recover Exact Generation Request Markers Before Later Holds

> **Codex preflight:** Loaded `$bluey-ops` and verified the finding against the
> current Phase 606 worktree. No archive, model-provider credential, request,
> or production system was used.

## Issue

After a provider cost marker was durably written, a later generation hold could
block the exact retry used to recover an ambiguous provider dispatch. Treating
that retry as new authority could either strand spend reconciliation or tempt a
caller to mint another request after an outcome that may already exist.

## Root Cause

Generation hold evaluation occurred before lookup of the exact existing
provider request marker. The transaction therefore could not distinguish a new
provider reservation, which a hold must stop, from recovery of the already
durable request identity, which must remain available.

## Fix Summary

Both SQLite and PostgreSQL reservation paths first resolve the exact opaque
request scope inside the existing spend-accounting transaction. A matching
`held` marker returns `RecoveredAmbiguous` with the original reservation token
even when a later generation hold is active. Only a genuinely new request
reaches operational-hold evaluation and provider-cost insertion.

The replay remains identity strict: account, generation scope, provider, model,
and projected cost must all match the durable marker. A changed identity still
fails as a collision, and recovery never writes a second cost-hold row.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs_provider_cost_holds.rs` | Resolve and verify an exact durable provider marker before applying a later generation hold. |

## Edge Cases Handled

- A new provider reservation remains denied while the generation hold is
  active.
- An exact ambiguous retry returns the original token without a second marker.
- A changed model, provider, generation scope, account, or projected cost does
  not borrow the old recovery authority.
- A terminal provider marker remains terminal and cannot be reopened.
- PostgreSQL retains the operational-hold-first transaction lock order before
  spend and generation row locks.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  recovered_hold_precedes_later_generation_hold_without_weakening_identity
cargo test --manifest-path server/Cargo.toml \
  jobs_generation_hold_blocks_provider_reservation_until_explicit_release
```

## Known Limitations

- These regressions use local database fixtures and do not send a model request.
- Provider timeout/cancellation canaries, credentials, live spend-ledger
  monitoring, and production model-generation enablement remain parked.
