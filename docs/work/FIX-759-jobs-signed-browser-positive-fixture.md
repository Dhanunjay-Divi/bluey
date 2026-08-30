# FIX-759 — Signed Browser positive fixture

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B,
> the current local handoff, and the authoritative worktree. The SSD archive was not used.

**Severity:** P1 release-evidence authenticity blocker

**Status:** Implemented; the final certified sweep is green and final aggregate evidence is pending

## Issue

Certified local-run tests seeded a Browser descriptor or mutable registry rows directly. That could
prove downstream layout behavior while bypassing the signed trust-policy, manifest, build-proof,
activation, channel-assignment, claim-resolution, and runtime-attestation lifecycle used by the
real authority path.

The shared ATS fixture could also mint a successor whose evidence lifetime exceeded an already
installed trust policy, making long aggregate runs fail for fixture reasons rather than production
authority.

## Root Cause

The local positive fixture predated the complete Browser registry lifecycle. The later generic ATS
installer assumed it owned policy creation and used fixed successor lifetimes instead of respecting
the current compatible policy's expiry.

## Fix Summary

- Import the signed Browser trust policy and manifest through public registry APIs.
- Verify deterministic build proofs and apply a signed activation through the real head transition.
- Assign the account channel and resolve the exact claim-time Browser binding.
- Derive the ATS runtime target from the signed descriptor, manifest, artifact, automation bundle,
  Chromium, Playwright, and build-descriptor hashes.
- Use that runtime in certified local Phase A, production submit, and layout-drift tests.
- Reuse an existing ATS policy only when its complete delegated trust and certification
  requirements match the fixture policy.
- Cap evidence, layout, manifest, and activation lifetimes below the current policy expiry.

## Files Modified

| File | Change |
| --- | --- |
| `server/src/db/jobs/browser_release_registry.rs` | Add a full signed Browser fixture lifecycle. |
| `server/src/db/jobs/ats_certification_authority.rs` | Reuse compatible policy and cap lifetimes. |
| `server/src/db/jobs/tests.rs` | Route certified local tests through signed Browser authority. |

## Evidence

- **PASS:** final `certified_` sweep, 15 passed and 0 failed in 41.82 seconds.
- **PASS:** signed fixture successor lifetime regression.
- **PENDING:** final aggregate and strict Clippy evidence.

## How To Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  signed_fixture_installer_caps_successor_lifetime_to_current_policy -- --nocapture
cargo test --manifest-path server/Cargo.toml --lib certified_ -- \
  --nocapture --test-threads=1
```

## Known Limitations

- Deterministic signed fixtures do not prove artifact publication, immutable registry read-back,
  production key custody, notarization, Docker/Linux runtime, or hosted canaries.
- All Browser and Jobs production flags remain disabled.
