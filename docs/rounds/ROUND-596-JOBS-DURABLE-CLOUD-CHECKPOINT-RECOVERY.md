# Round 596 - Jobs Durable Cloud Checkpoint Recovery

**Date:** 2026-08-03

**Branch:** `feat/phase-jobs-full-autonomy-20260802`

**Status:** Implemented and verified; Browser distribution remains off

## Outcome

Bluey can now recover a cloud Browser run after runner-process restart without
guessing whether an employer-facing action happened.

The runner persists a versioned checkpoint before acknowledging work. Recovery
submits that checkpoint to the server and removes it locally only after the
server durably accepts the result. Version 2 checkpoints include the opaque
lease token; version 1 remains readable only for bounded upgrade recovery.

## Immutable Binding

Every recovery request is checked against the exact:

- Bluey account;
- application and run;
- browser profile;
- cloud lease owner;
- fencing number;
- v2 lease token; and
- checkpoint phase.

Claiming a cloud lease also atomically binds the existing application-attempt
reservation to the cloud runner. A missing attempt, local-runner attempt, or
already-conflicting attempt cannot be claimed.

## Recovery Policy

```text
prepared / needs_input / provider_review
  -> release lease, browser session and attempt together

final_submit_started / final_submit_activated / side_effect_unknown
  -> preserve application as side_effect_unknown
  -> never retry automatically

submitted checkpoint
  -> accept only when server-owned application state, run binding,
     submitted_at, receipt fingerprint and stored evidence all agree
```

Both outcomes are replay-safe. A repeated valid reconciliation returns the
stored result; a stale owner, fence, token or cross-account identifier fails.

## Storage Transactions

SQLite uses an immediate transaction and PostgreSQL uses row locks. The
application, browser session, execution lease and attempt reservation either
move together or all roll back. The runner never treats local checkpoint
deletion as the source of truth.

## Verification

```text
Runner test files:                         8 passed
Runner tests:                             54 passed
Runner strict TypeScript:                 passed
Server unit tests:                       826 passed
Server HTTP integration tests:            80 passed
Runner entitlement matrix:                 1 passed
PostgreSQL schema compatibility:            1 passed
Checkpoint-focused server tests:            4 passed
Rust strict Clippy:                       passed
Rust formatting:                          passed
git diff --check:                         passed
```

The code contains equivalent SQLite and PostgreSQL transactions. A live
PostgreSQL test URL was not configured locally, so the database-specific live
PostgreSQL recovery matrix remains part of the deployment gate.

## Production Boundary

No production deployment or feature-flag change is part of this round. Model
generation, local Browser distribution, cloud Browser distribution and mailbox
sync remain disabled. This closes one cloud-recovery prerequisite; it does not
claim that the cloud Browser pool is launch-ready.
