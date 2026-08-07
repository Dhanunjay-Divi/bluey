# FIX-674: Keep Unassigned Reservations Out of Runner-Kind Scope

> **Codex preflight:** Loaded `$bluey-ops` and reproduced the integration
> regression in the current Phase 606 worktree. No archive, runner, employer
> portal, live tenant, or production system was used.

## Issue

Application-attempt reservation returned an internal error before cloud/local
worker lease tests could reach their intended authority boundary.

## Root Cause

The reservation path passed its internal `unassigned` state into the closed
operational `runner_kind` scope, which accepts only `cloud` or `local`.

## Fix Summary

Reservation context now maps `cloud` and `local` to their matching scope,
omits runner scope for `unassigned`, and rejects any other internal value. The
later worker claim derives cloud/local context, evaluates its hold, and then
atomically persists the frozen runner binding.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/eligibility.rs` | Map only concrete runner assignments into operational context. |

## Edge Cases Handled

- Unassigned reservation does not invent a runner authority.
- Concrete cloud/local reservations remain scoped.
- Unknown internal runner values fail closed.
- Worker claim evaluates the final concrete runner kind independently.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --test integration_e2e \
  jobs_execution_lease_routes_require_worker_auth_and_fence_submit
```

## Known Limitations

- No live cloud or local runner was claimed.
