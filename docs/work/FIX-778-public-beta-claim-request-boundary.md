# FIX-778: Local distribution claim exceeded the strict function-argument boundary

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against the Phase 621/622
> worktree. No deployment, production flag, provider, credential, or hosted state was changed.

## Issue

The exact all-target strict Clippy gate rejected the new local Browser distribution-claim wrapper
because it accepted eight arguments, above the repository's seven-argument limit.

## Root Cause

Phase 622 added the raw distribution-enabled decision to the existing release-bound claim inputs
without grouping the complete caller-owned claim context. Runtime behavior and tests were correct,
but the expanded positional interface was harder to audit and failed the required warning-free
source gate.

## Fix Summary

Introduced a typed `BrowserLocalRunDistributionClaim` input containing the run, ticket, nonce,
verified build descriptor, server release, and distribution decision. The production API and the
one direct regression fixture now construct that input explicitly before calling the claim helper.
No lint suppression was added, and the inner transaction, replay ordering, authorization checks,
response construction, persistence, and wire contract are unchanged.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/browser_release_authority.rs` | Replace the eight-position wrapper interface with one typed claim request |
| `server/src/api/jobs.rs` | Construct the typed distribution claim at the HTTP boundary |
| `server/src/db/jobs/tests.rs` | Construct the same typed input in the direct distribution-disabled regression |
| `docs/work/FIX-778-public-beta-claim-request-boundary.md` | Record the strict-source failure and bounded refactor |
| `CHANGELOG.md` | Record the typed claim-boundary correction under Unreleased |

## Edge Cases Handled

- Run, ticket, nonce, verified descriptor, server release, and distribution decision remain
  visible as one auditable value at the call boundary.
- Exact replay still reaches database authority before mutable distribution and fleet readiness.
- A disabled fresh distribution remains `DistributionUnavailable` without state mutation.
- The change adds no serialized input, public field, logging dimension, or external effect.

## How to Test

```bash
cargo fmt --all -- --check
cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --lib \
  local_release_claim_is_atomic_exactly_replayable_and_revocation_fences_submit
cargo clippy --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --all-targets -- -D warnings
```

## Known Limitations

- This refactor closes the strict local source gate; hosted runner and provider canaries remain
  separate release requirements.
