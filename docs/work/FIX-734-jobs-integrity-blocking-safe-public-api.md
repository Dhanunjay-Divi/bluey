# FIX-734: Jobs Integrity Blocking-safe Public API

**Severity:** P1 PostgreSQL runtime failure

**Status:** 🟡 Implemented; one public-wrapper regression gap remains

## Issue

The public Phase 614B import and resolver functions acquired PostgreSQL connections directly, so
async Tokio callers could trip the repository's blocking-context guard.

## Required Fix

- Enter `run_blocking_db` inside every public pool-based import and resolve function.
- Retain transaction-aware variants for already-prelocked callers.
- Add async PostgreSQL guard regressions without weakening the guard.

## Implementation

The public pool-based trust-policy import, attestation import, revocation import, and current
resolver in `server/src/db/jobs/job_integrity_authority.rs` enter `run_blocking_db`; transaction
variants remain available to callers that already own the relevant locks.

## Evidence

Mapped coverage:

```text
postgres_public_integrity_apis_enter_blocking_boundary_when_configured  PRESENT; ENV-GATED
Source mapping of all four public run_blocking_db wrappers               COMPLETE
Frozen-source check/Clippy/full test gates                               PENDING
```

The configured async regression invokes attestation import, resolver, and revocation import, but
its fixture creates the trust policy inside an existing blocking closure. It therefore does not
directly exercise the public trust-policy wrapper from an async caller; keep the evidence posture
yellow until that case is added and run.
