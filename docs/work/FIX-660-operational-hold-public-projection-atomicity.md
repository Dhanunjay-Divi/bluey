# FIX-660: Keep Operational-Hold Mutation and Public Projection Atomic

> **Codex preflight:** Loaded `$bluey-ops` and verified the finding against the
> current Phase 606 worktree. No archive, live database, or production system
> was used.

## Issue

An operational hold transition could become durable even when construction of
its redacted API result failed, leaving the caller with an error after the
authority had already changed.

## Root Cause

The mutation transaction and the keyed public-reference projection were
separate success boundaries. Committing before `scopeRef` and
`currentEventRef` were derived made a projection failure observationally
ambiguous: the API could report failure even though the event and head had
advanced.

## Fix Summary

Both SQLite and PostgreSQL append paths now build the complete
`OperationalHoldAppendResult`, including its redacted keyed references, before
committing. Any canonicalization, hashing, or public-projection failure rolls
the transaction back. An injected projection failure verifies that a release
does not add history or move the held head.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/operational_holds.rs` | Project the public state inside the mutation transaction and add rollback coverage. |

## Edge Cases Handled

- A failed projection of a release leaves the prior held revision unchanged.
- No successor event remains when projection fails.
- Exact replay still returns the original redacted result without new history.
- Public results still omit raw scope, event, reason-reference, and actor data.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  public_reference_failure_rolls_back_the_hold_transition
cargo test --manifest-path server/Cargo.toml public_state_redacts_scope_reason_and_actor
```

## Known Limitations

- The injected failure regression runs locally on SQLite. The PostgreSQL path
  has the same pre-commit ordering in source, but authorized live PostgreSQL
  execution remains an external gate.
