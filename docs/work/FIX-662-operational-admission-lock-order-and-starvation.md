# FIX-662: Serialize Holds Before Admission Without Starving Eligible Work

> **Codex preflight:** Loaded `$bluey-ops` and verified the finding against the
> current Phase 606 worktree. No archive, live database, provider, or production
> system was used.

## Issue

A hold could race with PostgreSQL admission when the admission transaction did
not acquire the matching advisory lock first. Separately, workers that always
rescanned the oldest held candidates could repeatedly return no work and starve
later eligible rows.

## Root Cause

The hold writer's exclusive advisory lock had no universal shared-lock partner
at protected admission boundaries, and several transactions acquired row or
ATS locks before evaluating the hold. Candidate selection also restarted at a
fixed queue prefix on every call, so a held prefix could consume the entire
scan budget indefinitely.

## Fix Summary

Every protected PostgreSQL admission now takes the shared operational-hold
advisory lock immediately after `BEGIN`, before any row or other authority lock.
The exclusive hold mutation therefore serializes against the whole admission
decision. SQLite keeps the equivalent immediate transaction boundary.

Direct discovery, global discovery, and communication dispatch scan at most
eight candidates and advance a queue-global compare-and-swap cursor when that
budget contains only held work. Mailbox batch claim uses a bounded scan budget
and durably defers held candidates for 30 seconds while updating both the
relational schedule and encrypted sync projection in the same transaction.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/operational_holds.rs` | Define the paired PostgreSQL shared/exclusive lock boundary. |
| `server/src/db/jobs/{browser_release_authority,discovery,global_discovery,local_runner,execution_leases,eligibility,mailbox_sync,communication_actions}.rs` | Acquire the hold lock first and enforce inside each protected transaction. |
| `server/src/db/jobs_provider_cost_holds.rs` | Serialize paid generation reservation with operational holds. |
| `server/src/db/jobs/tests.rs` and module tests | Assert lock order, bounded scans, progress, deferral, and post-marker replay. |
| `server/src/db/jobs/postgres_local_authority_tests.rs` | Add optional two-connection advisory-lock coverage. |

## Edge Cases Handled

- Local and cloud claim/reissue cannot use a replay to bypass a newer hold.
- Application reservation replay remains behind the hold boundary.
- Exact click-started and communication request-started replays remain recovery
  paths after a later hold.
- All-held queues stop after the explicit budget rather than scanning without
  bound.
- A queue with a held prefix reaches a later eligible candidate on a bounded
  subsequent call.
- Concurrent cursor updates use compare-and-swap and cannot grant authority.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  postgres_operational_hold_lock_is_first_in_every_protected_admission
cargo test --manifest-path server/Cargo.toml \
  discovery_held_candidate_scan_is_bounded_and_advances_on_the_next_call
cargo test --manifest-path server/Cargo.toml \
  global_discovery_held_candidate_scan_is_bounded_and_advances_on_the_next_call
cargo test --manifest-path server/Cargo.toml \
  communication_held_candidate_scan_is_bounded_and_advances_on_the_next_call
cargo test --manifest-path server/Cargo.toml all_held_mailbox_scan_stops_at_explicit_budget
```

## Known Limitations

- The discovery/global/communication scheduling cursor is process-local and
  resets on process restart; it is progress state, never durable authority.
  Mailbox held deferral is durable. Restart/churn behavior still requires an
  authorized production soak.
- The two-connection PostgreSQL test is opt-in through
  `BLUEY_TEST_POSTGRES_URL`; no authorized live URL was available locally.
