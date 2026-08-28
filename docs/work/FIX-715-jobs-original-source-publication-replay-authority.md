# FIX-715: Original-Source Terminal Publication And Replay Authority Was Incomplete

> **Codex preflight:** Load `$bluey-ops` before diagnosis, implementation, or review and reconcile
> this record with Round 614 and the final branch diff.

**Status:** Implemented; public SQLite lifecycle/replay regressions green, live PostgreSQL pending

## Issue

A verifier terminal request could be accepted without rechecking every current server-owned
authority named by its lease, and PostgreSQL exact replay could lose a race: two identical terminal
requests could both miss the receipt before locking, after which the loser observed an idle
assignment and returned lease loss instead of the original immutable result. Changed-byte replay
also had to quarantine and invalidate any earlier positive head rather than merely recording a
conflict beside still-consumable authority.

## Root Cause

Terminal handling mixed three concerns—lease validation, immutable evidence publication, and replay
recovery—without one closed transaction contract. Some current release/runtime/source/hold checks
were inherited from lease issue instead of repeated at publication, and the PostgreSQL replay read
occurred before the contended assignment row was acquired.

## Fix Summary

- Bind immutable observations, receipts, transitions, and heads to exact assignment, attempt,
  generation, fence, replay key, release, activation, manifest, runtime, protocol, provider target,
  canonical job, employer, destination, and current managed-authority bytes.
- Recheck current source, runtime/release, operational hold, lease, and fence authority at lease,
  heartbeat, and first terminal publication as applicable.
- Return an authenticated existing byte-identical terminal result without reminting time,
  freshness, or head generation, including after a concurrent PostgreSQL waiter acquires the
  assignment row or after later release/runtime authority loss. This is response recovery, not a
  new publication.
- Treat the same replay identity with different canonical bytes as an integrity conflict, append a
  quarantine event, and set the assignment state to `quarantined`. Do not mint a second receipt or
  head. Positive projection then rejects the previous immutable head because consumption requires
  the exact assignment to remain `idle`.
- Keep failure request codes separate from the canonical receipt-status vocabulary.
- Start readiness only after the private verifier listener is actually bound.

## Verification

Observed focused checkpoints:

```text
Rust original-source authority                25 / 25; normal-parallel twice
Projection/effect regressions                  2 / 2
Schema parity                                 95 tables / 79 indexes per dialect
SQLite migration/hold replay                   focused pass
Rust all-target check and strict Clippy        passed
```

The 25-test Rust suite invokes the public SQLite lease, heartbeat, completion, and failure APIs. It
proves exact heartbeat replay; positive completion; byte-identical terminal response-loss replay
without reminting rows or freshness; changed-byte conflict quarantine and old-head rejection; exact
terminal replay after later runtime revocation while a new request is denied; retryable failure and
its exact replay; reclaimed-lease rejection of stale heartbeat/complete/fail calls; and
publication-time hold, source, and runtime denial with no receipt/head mutation. A static
PostgreSQL regression separately pins heartbeat, first terminal publication, and changed-byte
quarantine to `H -> M -> D -> assignment`.

PostgreSQL code and schema compile and match the paired source, but no authorized disposable
PostgreSQL URL existed; a real concurrent waiter, migration replay, response-loss replay, and
failure-injection run remain unproven.

## Limits

Receipt digests are server-derived audit bindings; they are not provider signatures, TLS
notarization, or proof that a provider remained unchanged after the recorded observation.
