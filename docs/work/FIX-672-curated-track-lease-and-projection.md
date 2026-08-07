# FIX-672: Freeze and Validate Curated Discovery Tracks

> **Codex preflight:** Loaded `$bluey-ops` and verified the findings in the
> current Phase 606 worktree. No archive, external feed, live database, tenant,
> or production system was used.

## Issue

A Career Track could change after curated discovery admission, an inactive
Track could still be selected during completion, and relational `active` drift
from `track_json.active` could omit a hold scope while leaving the Track
eligible elsewhere.

## Root Cause

Track writes did not fence active discovery leases. Curated selection did not
filter inactive Tracks. Operational context filtered on relational `active`
without validating the JSON projection that later selection trusted.

## Fix Summary

Every Track insert, update, and delete now serializes through the
discovery-account fence and rejects changes while a relevant unexpired source
lease exists. Curated selection considers active Tracks only. Discovery context
reads every account Track, validates relational/JSON ID and active parity, and
adds account-wide scopes only for active rows. A source explicitly bound to an
inactive Track retains its Career Track and Region scopes.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/profile_postings.rs` | Fence Track mutations and reject them during relevant discovery leases. |
| `server/src/db/jobs/discovery.rs` | Filter curated selection and validate Track projections without dropping bound scopes. |
| `server/src/db/jobs/tests.rs` | Cover lease freeze, terminal release, inactive selection, projection drift, and inactive-bound Region holds. |

## Edge Cases Handled

- New, updated, or deleted Track during a curated lease.
- Track mutation after successful or failed terminal completion.
- Held inactive Track cannot enter admission or completion selection.
- Both directions of relational/JSON active mismatch fail closed.
- An inactive directly bound Track still matches its Region hold.
- PostgreSQL missing-source phantoms are blocked by the account advisory fence.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml curated_discovery_lease_freezes_track_mutations
cargo test --manifest-path server/Cargo.toml inactive_held_track_is_not_admitted
cargo test --manifest-path server/Cargo.toml discovery_track_active_projection_mismatch
cargo test --manifest-path server/Cargo.toml inactive_bound_track_keeps_its_region_hold_scope
```

## Known Limitations

- Live PostgreSQL concurrency evidence remains parked without an authorized
  test URL.
