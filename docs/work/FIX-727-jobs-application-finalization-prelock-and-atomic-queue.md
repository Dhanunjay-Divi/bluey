# FIX-727: Application Finalization Prelock And Atomic Queue

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and verify its memory
> against the final Phase 614 record, Round 614B, and the current repository state.

**Severity:** P1

**Status:** Implemented; mapped focused regressions present; frozen-source aggregate evidence pending

## Issue

Before this correction, the PostgreSQL `commit_prepared_application` path read posting/source
authority under `M -> D`, acquired ATS only later, and never acquired the required operational-hold
fence `H`. Protected reads could therefore occur outside the canonical cross-authority order and
race hold, managed-release, ATS, account, or integrity changes.

A second historical P1 gap split finalization across transactions:
`finalize_prepared_application_kit` committed an Auto-submit application as `queued`, and only then
did the API freeze `approved_execution`. A failure between those operations could leave a queued
application without the approval receipt that authorized it, while the direct finalization path
bypassed the authoritative `save_application` queue gate.

## Root Cause

Prepared-kit finalization predates the composed operational-hold, managed-release, ATS, account,
and signed-integrity admission contract. Its transaction accumulated authority checks locally
instead of entering through one common prelock, while queue state and approval receipt were split
across two commits in API order rather than one authoritative state transition.

## Required Fix

The implemented correction:

- make `H -> M -> ATS -> D -> integrity control FOR SHARE -> exact integrity head FOR SHARE` the
  first protected PostgreSQL acquisition before posting, source, approval, or queue-authority
  reads;
- preserve an equivalent single-snapshot, zero-mutation transaction in SQLite;
- freeze the exact `approved_execution` receipt and transition to `queued` atomically in one
  authoritative transaction; or finalize to a nonqueued Review-first state and invoke the normal
  authoritative queue transition afterward;
- prohibit `finalize_prepared_application_kit` from directly creating a queued row that bypasses
  current queue admission;
- ensure any approval-freeze, source/integrity, ATS, hold, entitlement, or persistence failure
  rolls back both approval and queue state; and
- preserve Review-first finalization as `awaiting_review` with no queue capability or reservation.

The selected architecture freezes `approved_execution` and the queued transition in the same
database transaction. It does not use an API-level compensating write as its consistency mechanism.

## Files Modified

| File                                             | Change                                                                  |
| ------------------------------------------------ | ----------------------------------------------------------------------- |
| `server/src/db/jobs/applications.rs`             | Prelock complete authority; atomically persist approval and queue state  |
| `server/src/api/jobs.rs`                         | Use current composed authority for approval and queue transitions        |
| `server/src/db/jobs/tests.rs`                    | Cover typed finalization denial and configured PostgreSQL behavior       |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | Cover live lock-order edges on configured PostgreSQL                |

## Edge Cases To Handle

- A hold, managed release, ATS certification, source receipt, or integrity head changes while the
  prepared kit is being finalized.
- Approval receipt persistence fails after all validation but before queue state would change.
- Queue persistence fails after approval bytes are prepared.
- Concurrent finalizers race with approval revocation or a newer application revision.
- An idempotent replay observes the already committed exact approval and queue transition without
  reminting authority.
- Review-first finalization never becomes queued merely because the requested mode was Auto-submit.

## How To Test

Mapped focused evidence in the current source:

```text
prepared_auto_submit_approval_and_queue_share_one_transaction              PRESENT
postgres_finalization_uses_lock_first_read_committed_authority             PRESENT
postgres_prepared_finalization_prelocks_exact_evidence_before_authority_time PRESENT
prepared_auto_submit_finalization_preserves_typed_employer_hold_and_mutates_nothing PRESENT
postgres_prepared_auto_submit_finalization_preserves_typed_employer_hold_when_configured PRESENT; ENV-GATED
postgres_evidence_wait_expiry_leaves_prepared_rows_unmodified               PRESENT; ENV-GATED
production_signed_manual_approval_reserves_and_queues_without_legacy_sanitization PRESENT
Frozen-source aggregate Rust and configured PostgreSQL manifest             PENDING
```

The named tests are mapped coverage, not newly observed executions. Final exact counts and the
configured PostgreSQL manifest remain Phase 614B evidence work.

## Known Limitations

- Live PostgreSQL contention and interruption remain unproven until actually exercised.
- This fix consumes Phase 614/614B authority; it does not generate source, employer, or risk
  evidence.
- It does not enable Auto-submit, deploy, change flags, or perform an employer-facing effect.
