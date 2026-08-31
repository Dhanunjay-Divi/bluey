# FIX-776: Operations route tests retained the pre-hardening cache contract

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against the Phase 621/622
> worktree. No deployment, production flag, provider, credential, or hosted state was changed.

## Issue

The full server unit suite failed three Jobs operations-route tests after the public-beta response
hardening changed authenticated and administrative responses from `private, no-store` to the
stronger `private, no-store, max-age=0` contract with `Vary: Authorization`.

## Root Cause

FIX-771 correctly applied the stronger private-cache policy at the composed router middleware, but
the existing operations tests still asserted the earlier exact header string. Focused Phase 621/622
tests did not include those older route-composition cases, so the stale assertions were exposed only
by the full 1,611-test library run.

## Fix Summary

Updated only the affected route-level expectations to require the current exact cache-control
value. The administrative mutation journey also asserts `Vary: Authorization`. Production routing,
authorization, response bodies, status codes, database behavior, and cache middleware are unchanged.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs_operations.rs` | Align operations-route tests with the hardened private-cache response contract |
| `docs/work/FIX-776-public-beta-private-cache-test-contract.md` | Record the full-suite failure and bounded test correction |
| `CHANGELOG.md` | Record the test-contract correction under Unreleased |

## Edge Cases Handled

- Unauthenticated, forbidden, malformed, oversized, unknown-field, deletion-fenced, and successful
  administrative responses retain the exact hardened cache contract.
- The direct private-response helper remains covered by its own minimum `private, no-store` unit
  contract; composed production routes are required to add `max-age=0` and authorization variance.
- No response becomes cacheable and no private identifier is added to headers, bodies, logs, or
  metrics.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --lib \
  api::jobs_operations::tests::admin_routes_mutate_list_and_report_readiness_without_private_ids
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --lib \
  api::jobs_operations::tests::full_and_standalone_routers_protect_operations_and_metrics_routes
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --lib \
  api::jobs_operations::tests::full_and_standalone_routers_preserve_json_rejection_statuses
```

## Known Limitations

- This fix corrects stale tests; it does not substitute for the exact-tip full Rust suite or hosted
  response-header canary required before release.
