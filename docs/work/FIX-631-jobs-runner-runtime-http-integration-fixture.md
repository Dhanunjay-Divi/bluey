# FIX-631: Bind the Runner Runtime in the HTTP Integration Fixture

> **Codex preflight:** Loaded `$bluey-ops` and diagnosed the stale fixture from
> the final Round 604 server gate; no external runner or live system was used.

## Issue

The full server integration suite received HTTP 422 from the runner-volume
instance-claim route because its Phase 602 fixture omitted Round 604's required
`runtimeGrant` object. The test therefore stopped before exercising the signed
execution-lease and submit fence it was intended to verify.

## Root Cause

`server/tests/integration_e2e.rs::setup_signed_runner_volume` still signed an
empty instance-claim payload and sent only the volume proof. Round 604 requires
the claim to bind a server-issued one-time process-runtime grant, its token
string digest, and the complete runtime digest.

## Fix Summary

- Keep the existing unattested offline-volume fixture volume-only.
- Add a separate execution-runtime fixture backed by the real process-runtime
  grant persistence API.
- Sign the instance claim over the exact grant ID, SHA-256 of the base64url
  token string bytes, and runtime digest, then send `{ proof, runtimeGrant }`
  through the authenticated HTTP route.
- Bind the same grant ID and digest into the execution-lease proof/body and
  assert the trusted values returned by both endpoints.
- Preserve the concurrent-clone conflict test with the same exact grant.

## Files Modified

| File | Change |
|------|--------|
| `server/tests/integration_e2e.rs` | Update the signed HTTP fixture to use exact Round 604 process-runtime authority. |
| `docs/work/FIX-631-jobs-runner-runtime-http-integration-fixture.md` | Record the stale fixture and correction. |

## Edge Cases Handled

- Missing runtime authority remains an HTTP 422 validation failure.
- A runtime grant is not manufactured for offline deletion fixtures.
- The instance-claim and execution-lease canonical payloads bind the same
  runtime identity.
- A concurrent cloned instance remains fenced with HTTP 409.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --test integration_e2e \
  jobs_execution_lease_routes_require_worker_auth_and_fence_submit -- --nocapture
cargo test --manifest-path server/Cargo.toml --test integration_e2e
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
```

## Known Limitations

- This is local HTTP integration evidence; authenticated production fleet and
  physical/cloud runtime canaries remain separately authorized gates.
