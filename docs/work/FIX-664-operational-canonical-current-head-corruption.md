# FIX-664: Reject Corrupt Current Heads on Every Authority Surface

> **Codex preflight:** Loaded `$bluey-ops` and verified the finding against the
> current Phase 606 worktree. No archive, live database, provider, or production
> system was used.

## Issue

A corrupt current held event failed closed during evaluation but could be
advanced through a later mutation into a valid released head, restoring new
work without first proving the integrity of the authority being released. A
relational head changed to `released` could also evade an active-only query even
when its canonical event remained `held`, making admission and readiness report
false safety.

## Root Cause

Mutation and listing trusted selected relational head fields without
reconstructing and validating the exact canonical current event and all of its
stored projections. That made it possible for corruption in an event ref,
canonical payload, ancestry, actor, timestamp, or head/event join to be hidden
by a valid successor. Some read paths filtered on relational state before
canonical validation, and by-reference resolution joined too narrowly to
distinguish a corrupt head from an ordinary compare-and-swap conflict.

## Fix Summary

One canonical current-head validator now checks the joined head and current
event before a successor append, list projection, or evaluation. It verifies
schema, bounded identifiers, normalized scope and reason data,
revision/timestamp bounds, ancestry shape, keyed refs, canonical bytes and
SHA-256, and every relational head/event projection. Exact event replay also
recomputes and verifies its stored ref, digest, and canonical bytes. Append and
list use a left join so a missing current event is an explicit storage failure.
Admission evaluates and validates every exact matching held or released head;
active listing and readiness validate the complete head set before filtering or
counting. By-reference resolution selects the complete head/event projection
before resolving opaque refs. A corrupt head cannot be hidden, advanced into a
release, or reported as ready; no successor event is written, and the prior
event chain remains available for incident recovery.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/operational_holds.rs` | Validate the canonical head before successors/list/evaluation and verify exact event replay identity. |

## Edge Cases Handled

- Missing current event, mismatched head state, revision, actor, or timestamp.
- Mismatched stored scope/event refs and canonical event digest.
- Invalid predecessor, first-event ancestry, reason code, or canonical time.
- Relational reason, transition, scope, capability, or event identity drift,
  including a relational `released` state over a canonical `held` event.
- Exact replay against a corrupt persisted event conflicts without changing
  history.
- Admission, readiness, active listing, raw release, and by-reference release
  all fail closed on the same corrupt head.
- Release against a corrupt held event returns storage failure and writes no
  release event.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  mismatched_persisted_event_reference_blocks_replay_and_release
cargo test --manifest-path server/Cargo.toml \
  held_head_without_exact_canonical_event_fails_closed
cargo test --manifest-path server/Cargo.toml \
  malformed_canonical_event_blocks_exact_replay_and_every_release_path
cargo test --manifest-path server/Cargo.toml \
  released_relational_head_with_held_canonical_event_fails_every_read_path
cargo test --manifest-path server/Cargo.toml \
  operational_hold_chain_is_replay_safe_and_requires_exact_cas
```

## Known Limitations

- Database immutability triggers normally reject direct event mutation; the
  corruption test disables that protection only to prove defense in depth.
- Authorized live PostgreSQL corruption/recovery rehearsal and backup-restore
  validation remain external production gates.
