# FIX-733: Jobs Integrity Post-lock Database Time

**Severity:** P1 freshness race

**Status:** 🟡 Implemented; bounded contention evidence remains incomplete

## Issue

Policy, attestation, revocation, and resolver paths sampled database time before acquiring their
complete authoritative locks. A waiter could cross expiry or revocation effectiveness while still
using the earlier timestamp.

## Required Fix

- Acquire the complete control/head snapshot before sampling database time.
- Validate freshness, scheduled revocations, and transition timestamps using that post-lock time.
- Preserve SQLite one-transaction zero-mutation behavior.
- Add contention tests that hold locks across expiry/effectiveness boundaries.

## Implementation

Policy import and current-authority resolution in
`server/src/db/jobs/job_integrity_authority.rs` sample database time after their authoritative
SQLite/PostgreSQL lock boundary. The composed PostgreSQL resolver in
`server/src/db/jobs/job_integrity_composition.rs` likewise resolves freshness after its publication
fence.

## Evidence

Mapped coverage:

```text
sqlite_policy_waiter_samples_time_after_acquiring_the_writer_lock              PRESENT
postgres_lock_first_read_committed_observes_waited_writer_when_configured      PRESENT; ENV-GATED
Final exact PostgreSQL manifest and full aggregate run                         PENDING
```

The SQLite policy waiter and configured PostgreSQL composition waiter do not yet form a direct
contention matrix for every policy, attestation, revocation, and resolver entrypoint. The verdict
therefore remains yellow even though the source repair is present.
