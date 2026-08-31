# FIX-735: Jobs Integrity Authorization ID Collision

**Severity:** P2 immutable-identity contract

**Status:** 🟡 Implemented; collision matrix and configured PostgreSQL evidence incomplete

## Issue

Verified authorization IDs were parsed but not persisted or collision-checked. Reusing one
role-scoped ID with changed signed bytes could therefore publish distinct authority objects.

## Required Fix

- Persist root, employer-identity, job-risk, and revocation authorization IDs within the existing
  seven-table model.
- Enforce role-scoped changed-byte conflict with zero mutation.
- Preserve exact authenticated replay.

## Implementation

The paired migrations persist root, employer-identity, job-risk, and revocation authorization IDs
with immutable/unique constraints. Import code in
`server/src/db/jobs/job_integrity_authority.rs` also checks role-scoped identity before publishing
changed bytes.

## Evidence

Mapped coverage:

```text
authorization_ids_are_role_scoped_persisted_and_collision_checked      PRESENT (SQLite)
SQLite/PostgreSQL migration authorization-ID constraints                PRESENT
Root/revocation changed-byte collision behavioral matrix                PENDING
Configured PostgreSQL collision/replay behavior                          PENDING
```

The strongest behavioral regression currently covers the disjoint employer-identity/job-risk
authorization IDs. Source and schema coverage are broader than that test, so this FIX remains
yellow rather than claiming complete behavioral evidence.
