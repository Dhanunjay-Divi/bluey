# FIX-738: Jobs Integrity Frozen Receipt Field Names

**Severity:** P2 frozen-contract mismatch

**Status:** Implemented; mapped strict receipt regression present; aggregate evidence pending

## Issue

The authority projection serialized shortened authorization field names rather than the exact
frozen `employerIdentityAuthorizationSha256` and `jobRiskAuthorizationSha256` receipt keys.

## Required Fix

- Emit the exact frozen receipt names.
- Reject unknown/missing receipt fields at comparison boundaries.
- Add exact serialized-key and receipt-roundtrip assertions.

## Implementation

`server/src/db/jobs/job_integrity_composition.rs` emits the frozen
`employerIdentityAuthorizationSha256` and `jobRiskAuthorizationSha256` keys and performs an exact,
strict application-receipt comparison.

## Evidence

Mapped coverage:

```text
receipt_is_exact_strict_and_drift_sensitive                  PRESENT
node_and_rust_share_job_integrity_canonical_and_strict_signature_vectors PRESENT
Frozen-source focused and aggregate execution                PENDING
```
