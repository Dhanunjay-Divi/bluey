# FIX-742: Jobs Workflow Request-start Integrity

**Severity:** P1 external-effect drift

**Status:** 🟡 Implemented in request-start wiring; behavioral drift matrix pending

## Issue

A fresh workflow request could enter `request_started`/`delivering` after source or integrity
authority changed because only managed-cloud admission was re-resolved.

## Required Fix

- Re-resolve and compare composed application authority before fresh request-start mutation.
- Preserve exact immutable replay once request-start/delivery has crossed.
- Add revocation, expiry, destination-drift, zero-mutation, and replay tests.

## Implementation

`server/src/db/jobs/workflow_commands.rs` revalidates current composed application authority before
fresh managed request-start mutation. The exact replay branch uses stored authority after the
irreversible request-start boundary instead of retroactively invalidating immutable history.

## Evidence

Mapped coverage:

```text
fresh_request_start_rechecks_authority_and_exact_replay_uses_stored_authority PRESENT (STATIC)
cloud_queue_and_request_start_never_downgrade_managed_authority              PRESENT (STATIC)
postgres_request_start_locks_command_and_binding_before_attempt_and_final_time PRESENT (STATIC)
Revocation/expiry/destination-drift zero-mutation behavioral matrix          PENDING
Frozen-source aggregate execution                                             PENDING
```

The wiring and replay split are mapped, but no direct behavioral matrix yet proves all requested
drift classes leave fresh request-start state unchanged. The verdict remains yellow.
