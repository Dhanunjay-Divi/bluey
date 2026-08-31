# FIX-768: Public-beta local-runner gate fixture reached JSON rejection

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The focused Phase 621 integration run expected the local-runner distribution
authority after enabling the Jobs master gate, but received HTTP 422 before the
handler ran.

## Root Cause

`public_beta_gate_preserves_local_runner_and_worker_authority_boundaries` sent
only a `ticket` to the claim route. `LocalRunClaimRequest` also requires the
camel-case `claimNonce` and `buildProof` fields. With the master gate off, route
middleware correctly returned 404 before request extraction. With the master
gate on, Axum correctly rejected the incomplete JSON during extraction, so the
fixture never exercised the independently disabled local-runner distribution
gate.

## Fix Summary

The fixture now sends a schema-valid claim envelope for both requests. Its
values remain deliberately unauthorized. The test proves two distinct closed
boundaries:

- Jobs master off returns 404 with the master-gate response; and
- Jobs master on with local distribution off reaches the handler and returns
  503 with the explicit local-runner pause response.

This changes no production route, status, or authority behavior.

## Files Modified

| File | Change |
|------|--------|
| `server/tests/integration_e2e.rs` | Use a schema-valid unauthorized claim and assert the existing distribution-paused contract |
| `docs/work/FIX-768-public-beta-local-runner-gate-fixture.md` | Record diagnosis and verification |

## Edge Cases Handled

- The same envelope is used on both sides of the master-flag transition, so
  request-shape drift cannot masquerade as a gate-ordering change.
- Invalid credentials and build proof are never evaluated while distribution
  is paused, preserving the intended fail-closed ordering.

## How to Test

```bash
CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo +1.95.0 test --manifest-path server/Cargo.toml \
  --no-default-features --features integration-test-support \
  --test integration_e2e \
  public_beta_gate_preserves_local_runner_and_worker_authority_boundaries \
  -- --nocapture
```

## Known Limitations

- This test proves local Axum composition. Hosted route behavior remains part
  of the dark-deploy and rollback smoke gate.
