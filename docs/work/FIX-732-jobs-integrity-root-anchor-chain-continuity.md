# FIX-732: Jobs Integrity Root Anchor Chain Continuity

**Severity:** P1 trust-domain separation

**Status:** Implemented; mapped rotation/history regression present; aggregate evidence pending

## Issue

Successor policies did not prove continuity with the v1 root anchor. Replacing the environment
anchor could allow historical root and delegated keys to cross trust roles.

## Required Fix

- Freeze the exact root-anchor digest across the v1 monotonic policy chain.
- Reject root replacement and historical root/delegated cross-role reuse.
- Preserve normal delegated-key and multi-generation policy rotation.
- Treat root rotation as a separately designed signed protocol, not an environment swap.

## Implementation

The trust-policy import in `server/src/db/jobs/job_integrity_authority.rs` freezes the v1 root
anchor digest and rejects historical root/delegated cross-role reuse. Paired migrations
`infra/sqlite/server-runtime/058_jobs_signed_job_integrity_authority.sql` and
`infra/postgres/server-runtime/036_jobs_signed_job_integrity_authority.sql` persist the anchor and
key-role history used by those checks.

## Evidence

Mapped coverage:

```text
policy_successors_freeze_root_and_delegated_role_history    PRESENT
authorization_threshold_role_and_expiry_are_fail_closed     PRESENT
Paired migration structure and aggregate execution          PENDING FINAL EVIDENCE
```

No root-rotation protocol is claimed; the implemented v1 behavior rejects an anchor swap.
