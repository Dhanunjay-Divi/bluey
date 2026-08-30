# FIX-731: Jobs Integrity Strict Ed25519 Verification

**Severity:** P1 trust-boundary weakness

**Status:** Implemented; mapped Rust/Node regressions present; aggregate evidence pending

## Issue

Phase 614B accepted decoded Ed25519 verification keys without rejecting weak/small-order keys and
used non-strict signature verification. That is weaker than the frozen malleability threat model.

## Required Fix

- Reject weak root and delegated verification keys at import.
- Use strict Ed25519 verification for every authority signature.
- Add malformed-key, weak-key, small-order `R`, and alternate-encoding rejection vectors.
- Preserve accepted canonical signatures and threshold behavior.

## Implementation

`server/src/db/jobs/job_integrity_authority.rs` rejects weak decoded verifying keys and uses
`verify_strict` for authority signatures. The shared canonical fixture in
`jobs/automation/tests/fixtures/job-integrity-authority-vectors.json` is consumed by both the Rust
and Node suites.

## Evidence

Mapped coverage:

```text
strict_ed25519_rejects_weak_keys_and_malleable_signature_vectors             PRESENT
node_and_rust_share_job_integrity_canonical_and_strict_signature_vectors     PRESENT
job-integrity-authority-vectors.test.ts canonical cross-runtime fixture      PRESENT
Frozen-source focused and aggregate execution                                PENDING
```

`PRESENT` records test/source existence; this documentation refresh did not rerun the tests.
