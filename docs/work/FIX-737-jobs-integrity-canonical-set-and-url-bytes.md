# FIX-737: Jobs Integrity Canonical Set And URL Bytes

**Severity:** P2 canonical-authority ambiguity

**Status:** Implemented; mapped canonical-vector regressions present; aggregate evidence pending

## Issue

Evidence arrays were deduplicated but not required to use deterministic order, and HTTPS values
were parsed without requiring exact canonical serialization. Equivalent meanings could therefore
have multiple accepted signed byte forms.

## Required Fix

- Require field-specific ordering for every set-like evidence array.
- Require exact normalized URL round-trip, including host case, default port, path, and encoding.
- Add permutation, duplicate, uppercase-host, default-port, dot-segment, and whitespace vectors.

## Implementation

`server/src/db/jobs/job_integrity_authority.rs` requires sorted unique field-specific arrays,
canonical JSON, strict canonical HTTPS serialization, bounded canonical percent encoding, and exact
domain agreement. The Node fixture independently checks the same frozen canonical bytes.

## Evidence

Mapped coverage:

```text
canonical_role_domain_and_numeric_validation_is_closed                    PRESENT
node_and_rust_share_job_integrity_canonical_and_strict_signature_vectors  PRESENT
job-integrity-authority-vectors.test.ts cross-runtime canonical bytes     PRESENT
Frozen-source focused and aggregate execution                             PENDING
```
