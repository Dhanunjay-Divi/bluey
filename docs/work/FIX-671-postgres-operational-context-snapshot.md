# FIX-671: Fence PostgreSQL Operational Context as One Snapshot

> **Codex preflight:** Loaded `$bluey-ops` and verified the concurrency finding
> in the current Phase 606 worktree. No archive, live PostgreSQL URL, tenant,
> provider, or production system was used.

## Issue

Application and job admission could evaluate a posting, Track, discovery
membership, or source scope that changed before the protected authority commit.
An attempted retrofit also exposed inconsistent parent/child lock order.

## Root Cause

Operational-hold context readers did not share the discovery-account advisory
fence already used by most posting/source/membership writers. Some admissions
also reached account or child rows before acquiring that fence.

## Fix Summary

PostgreSQL context readers now take the shared discovery-account advisory lock,
lock and validate application/posting projections, and lock memberships and
sources in deterministic order. Every relevant writer takes the exclusive
form, including global materialization. Protected admissions acquire the
shared fence before account or child locks; Track mutation takes exclusive
fence, parent account key-share, then source rows.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs.rs` | Add the shared discovery-account advisory fence. |
| `server/src/db/jobs/operational_holds.rs` | Build application/job scope from one fenced, locked projection. |
| Jobs admission and materialization modules | Normalize advisory, parent, and child lock order. |
| PostgreSQL and structural tests | Assert writer coverage, lock order, and admission serialization. |

## Edge Cases Handled

- Application-to-job binding changes during context construction.
- Posting, membership, source, or Track changes race admission.
- Account deletion races source/Track child locks.
- Global materialization participates in the same account fence.
- Deterministic source/membership ordering avoids cross-candidate inversion.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  postgres_operational_context_account_fence_covers_scope_snapshot_and_writers
cargo test --manifest-path server/Cargo.toml \
  postgres_operational_context_holds_account_fence_through_admission_commit
```

## Known Limitations

- The two-connection PostgreSQL regression self-skips unless an authorized
  `BLUEY_TEST_POSTGRES_URL` is configured.
