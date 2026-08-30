# FIX-736: Jobs Integrity Revoked Negative Head

**Severity:** P2 denial-state accuracy

**Status:** Implemented; mapped negative-head revocation regression present; aggregate evidence pending

## Issue

An old-policy negative head returned `blocked`, `mismatch`, or review before current revocations
were evaluated. A later authorized revocation could not downgrade that head to the required
revoked Review-first result.

## Required Fix

- Resolve the historical signing policy and current relevant revocations before returning a
  negative-head result.
- Prove attestation, policy, evidence, and signing-key revocations produce `revoked` Review-first.
- Never fall back to an older positive.

## Implementation

`server/src/db/jobs/job_integrity_authority.rs` loads the historical signing policy and current
relevant revocations before returning a nonpositive head, then projects revoked material as
Review-first without falling back to an older positive.

## Evidence

Mapped coverage:

```text
revoked_old_policy_negative_head_degrades_to_review_required                  PRESENT
positive_revoked_material_cannot_publish_and_revoked_negative_never_falls_back PRESENT
Frozen-source focused and aggregate execution                                 PENDING
```
