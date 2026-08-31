# FIX-717: Original-Source Operational Holds Lacked Safe Schema And Runtime Parity

> **Codex preflight:** Load `$bluey-ops` before diagnosis, implementation, or review and reconcile
> this record with Round 614 and the final branch diff.

**Status:** Implemented; SQLite replay and paired schema parity green, live PostgreSQL pending

## Issue

The operational-hold ledger's closed capability constraint did not include
`original_source_verification`. PostgreSQL can widen that constraint directly, but SQLite replays
historical migration SQL and cannot safely alter the inline `CHECK`; an unconditional rebuild in
the new migration would also replay destructively on every startup.

## Root Cause

The original hold schema encoded the capability vocabulary inline before the verifier existed.
There was no one-time normal-runner upgrade that preserved the append-only event/head ledger,
triggers, indexes, foreign keys, and later compatibility columns.

## Fix Summary

- Add the typed `original_source_verification` capability to Rust and PostgreSQL constraints.
- Perform a one-time SQLite end-of-runner widening only when the old schema is detected.
- Build staging event/head tables with the widened schema, copy rows in exact column order, prove
  both-direction row-set equality, swap transactionally, restore triggers/indexes, and run
  `foreign_key_check`.
- Make every later startup a no-op after the widened schema is present.
- Recheck the distinct verifier hold at lease, heartbeat, and terminal publication.

## Verification

Observed checkpoints:

```text
Legacy SQLite hold-ledger migration/replay          passed
Paired schema parity                                95 tables / 79 indexes per dialect
Rust all-target check and strict Clippy              passed
```

The SQLite path detects the predecessor constraint, performs one verified transactional rebuild,
restores triggers/indexes, checks foreign keys, and becomes a no-op after widening. PostgreSQL 035
contains the exact widened constraint. No authorized disposable/hosted PostgreSQL URL was available,
so live catalog, interruption, and rollback behavior remain unproven.

## Limits

This fix adds a local authority boundary only. It does not create an operated hold command, alert,
deployment, current activation, or production read-back.
