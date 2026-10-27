# FIX-793: Preserve exact startup replay after a replacement boot

> Codex preflight: `bluey-ops`; Jobs-only Phase 628 local correction.

## Issue

An authenticated exact retry of a committed startup result failed after a
replacement boot advanced the runtime's current generation.

## Root Cause

Both database paths in `record_source_verifier_boot_startup` checked the current
boot head before looking up the immutable startup request. That applied a
fresh-publication fence to a response-only retry, contradicting exact-token,
exact-payload idempotency. Existing tests retried only before head advancement.

## Fix Summary

Keep runtime/session authentication and active release/protocol validation first.
Look up the stored request next and reject changed payloads by digest. Return an
exact committed result without mutation. Apply the current boot-head requirement
before every fresh probe/publication. Neither stored responses nor their replay
can authorize a stale boot to lease work.

## Files Modified

| File | Change |
|---|---|
| `server/src/db/jobs/source_verifier_boot.rs` | SQLite/PostgreSQL ordering and real-runtime regression |

## Edge Cases Handled

- Replacement registration advances current generation while the old ready
  heartbeat remains fresh: exact response replay succeeds, old work stays fenced.
- Same request ID with different payload conflicts; a new request ID from the
  stale boot cannot publish readiness.
- Wrong runtime session cannot retrieve the stored response.
- Snapshots of heads, registrations, startup authorities, heartbeat bindings and
  runtime heartbeats remain identical across response-only replay and rejects.

## How to Test

Run the `source_verifier_boot` library filter through the isolated launcher, with
both test-support features, then the full server gates. The new case is
`startup_retry_after_replacement_boot_is_read_only_and_keeps_old_work_fenced`.
Observed results belong in the Phase 628 IMPL continuation ledger, not this plan.

## Known Limitations

Live PostgreSQL concurrency is still unverified without an approved dedicated
non-production database. Active release revocation still fails closed before
replay; this fix does not turn historical results into execution authority.
