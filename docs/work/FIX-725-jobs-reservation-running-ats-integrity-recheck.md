# FIX-725: Reservation And Running ATS/Integrity Recheck

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and verify its memory
> against the final Phase 614 record, Round 614B, and the current repository state.

**Status:** Implemented; mapped focused regressions present; frozen-source aggregate evidence pending

## Issue

Before this correction, application-attempt reservation and transition to `running` used
PostgreSQL authority order `H -> M -> D` and rechecked original-source/discovery authority, but did
not independently re-resolve current ATS plus signed job-integrity authority before consuming
capacity.

The historical existing-reservation branches also mutated reservation state before completing
current-authority revalidation. That ordering made a later fail-closed denial insufficient to
prove zero mutation.

Later claim and final-effect gates already failed closed, so this gap did not by itself authorize
an external provider effect. It could nevertheless create a stale reservation or running
transition, consume bounded runner capacity, and make the earlier state imply more current
authority than was actually checked.

## Root Cause

Phase 614 correctly composed `H -> M -> ATS -> D` for application save/queue and final-effect
paths, while its reservation/running scope remained limited to source/discovery revalidation. The
historical reservation mutation path therefore retained a partial authority snapshot instead of
using one shared composed source, integrity, and ATS resolver.

## Required Fix

The implemented correction:

- resolve operational holds, managed release, ATS certification, account/deletion state, current
  Phase 614 source authority, and current Phase 614B integrity authority before reservation or
  running mutation;
- use PostgreSQL order
  `H -> M -> ATS -> D -> integrity control FOR SHARE -> exact integrity head FOR SHARE`;
- revalidate the exact source subject/material, integrity head, policy, role authorizations,
  revocations, and freshness inside the authoritative transaction;
- preserve equivalent SQLite snapshot and zero-mutation semantics;
- return the resolver's typed `ReviewRequired` or `Blocked` outcome without reserving capacity;
- move every existing-reservation mutation after the complete current-authority recheck;
- require the exact current running capability before either creating or advancing a reservation;
- reuse the same composed resolver at later claim, dispatch, and pre-Submit boundaries; and
- avoid adding a new `JobIntegrity` operational-hold capability.

## Files Modified

| File                                                         | Change                                                                    |
| ------------------------------------------------------------ | ------------------------------------------------------------------------- |
| `server/src/db/jobs/eligibility.rs`                          | Recheck composed authority and runner capability before reserve/update    |
| `server/src/db/jobs/applications.rs`                         | Route queued/running writes through current composed application authority |
| `server/src/db/jobs/tests.rs`                                | Cover ATS-head replacement before reserve and running mutation            |
| `server/src/db/jobs/managed_cloud_release_authority.rs`      | Pin reservation/running participation in the common PostgreSQL order      |
| This FIX                                                     | Reconcile the original finding with the implemented source and tests       |

## Edge Cases To Handle

- ATS authority expires after approval but before reservation.
- Integrity policy, delegated key, attestation, evidence, or source receipt expires before running.
- Revocation lands between preparation and reservation.
- A signed negative or successor head replaces a previously positive head.
- Source material, provider target, destination, employer, or ATS tenant changes.
- Two workers race to reserve while one authority snapshot becomes stale.
- Exact idempotent reservation replay observes the same current authority without double capacity.
- An existing reservation whose running capability was revoked remains byte-for-byte unchanged.
- PostgreSQL failure or CAS loss leaves no partial reservation, running state, or capacity debit.

## How To Test

Mapped focused evidence in the current source:

```text
reservation_and_running_mutations_follow_complete_current_authority     PRESENT
queued_and_running_updates_delegate_to_the_current_composed_save_gate   PRESENT
ats_head_replacement_cannot_reserve_frozen_auto_submit_capacity         PRESENT
ats_head_replacement_cannot_start_or_persist_a_frozen_auto_submit_run   PRESENT
postgres_ats_head_replacement_cannot_reserve_or_start_when_configured   PRESENT; ENV-GATED
production_signed_manual_approval_reserves_and_queues_without_legacy_sanitization PRESENT
Frozen-source full Rust/fmt/check/Clippy/diff gates                     PENDING
```

`PRESENT` records repository test coverage, not a new pass claim from this documentation-only
refresh. The exact final run counts and configured PostgreSQL result belong in the Phase 614B
IMPL/REVIEW after the source manifest freezes.

## Known Limitations

- This fix does not generate employer-identity or risk evidence; it consumes the separately signed
  Phase 614B authority.
- The configured PostgreSQL test is not portable evidence unless it is run against the final
  source/binary and recorded with its database manifest.
- It does not deploy, enable flags, contact providers, or authorize an application submission.
