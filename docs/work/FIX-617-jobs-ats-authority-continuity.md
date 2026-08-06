# FIX-617: Preserve ATS Authority Across Approval, Submit, and Revision

> **Codex preflight:** Loaded `$bluey-ops` and used only the active Round 604
> worktree and local handoff evidence.

## Issue

The partial ATS certification path could display current eligibility without
freezing that exact authority into an approved Auto-submit packet. Packet
finalization rebuilt only the generic eligibility decision, submitted receipts
were checked against client-visible proof but not the terminal database row,
and an intervention answer could revise a packet without invalidating its
preflight certification binding.

## Root Cause

- Packet finalization did not resolve ATS authority inside its SQLite or
  PostgreSQL transaction.
- Approval still emitted schema two for Auto-submit and silently accepted an
  already-approved schema-two packet.
- Receipt validation proved internal field consistency but did not require a
  byte-for-byte match with the single consumed or side-effect-unknown binding.
- Intervention reapproval released the runner and attempt reservation without
  atomically fencing an existing preflight ATS binding.

## Fix Summary

Finalization now resolves the exact posting/account certification in the same
database transaction and queued Auto-submit fails closed when authority is
missing, stale, revoked, suspended, drifted, or otherwise unusable. Approval
freezes the server-projected certification into schema three and binds it into
the packet checksum; legacy schema-two Auto-submit packets must be prepared and
approved again. Receipt persistence compares the presented schema-four
authority with the unique immutable terminal server record. Intervention
answers invalidate the matching preflight binding as `packet_changed` before
any lease, ticket, browser session, or application revision is committed.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/eligibility.rs` | Applies truthful active/inactive certification state and exact runner scope. |
| `server/src/db/jobs/applications.rs` | Re-resolves certification transactionally during packet finalization in both dialects. |
| `server/src/api/jobs.rs` | Freezes schema-three Auto-submit admission and requires the persisted terminal receipt authority. |
| `server/src/db/jobs/customer_data.rs` | Invalidates preflight authority atomically when an intervention changes the packet. |
| `server/src/db/jobs/tests.rs` | Covers active runner scoping and missing-binding fail-closed behavior. |

## Edge Cases Handled

- An authority expires or is revoked between match display and packet
  finalization.
- An active status projection cannot load its exact immutable binding.
- Certification authorizes Local but not Cloud, or Cloud but not Local.
- An older approved Auto-submit packet has no frozen certification object.
- A structurally valid receipt copies or mutates terminal authority fields.
- A response is lost after Phase B and the runner reuses the persisted terminal
  record without allocating a second canary or metering reservation.
- An answer revision races a preflight-to-consumed transition; the transaction
  permits exactly one winner and rolls back the other path.

## How to Test

```bash
cargo check --manifest-path server/Cargo.toml --lib
cargo test --manifest-path server/Cargo.toml --lib \
  active_ats_status_enables_only_certified_runners_and_requires_a_loaded_binding
cargo test --manifest-path server/Cargo.toml --lib \
  intervention_answer_revises_cloud_packet_and_requires_review
cargo test --manifest-path server/Cargo.toml --lib \
  certified_auto_packet_requires_and_transports_exact_schema_three_admission
```

## Known Limitations

- Live provider activation remains disabled until signed production authority,
  managed credentials, and the independent launch approvals exist.
- Physical-device and authorized live-tenant confirmation remain external
  launch gates; this fix performs no deployment or real submission.
