# ROUND-383 - Trial Grant Reservation

Date: 2026-07-05  
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Why

The owner asked to copy Pinky's trial discipline into Bluey. Bluey already had
the 15-minute trial, Try Us endpoint, Turnstile support, hashed trial-abuse
signals, and embed/RAG trial metering in this working tree. The remaining gap
was ordering: Bluey checked the trial ledger before account creation, but only
recorded the grant after account creation. A fast duplicate request could pass
the check before the first grant row existed.

## What Changed

- Added `reserve_trial_grant` to `server/src/db/trial_abuse.rs`.
- The reservation does an atomic-ish check-plus-insert:
  - SQLite uses `BEGIN IMMEDIATE`.
  - Postgres locks `trial_grants` briefly while it evaluates and inserts.
- Added `attach_grant_account` so the reserved grant is tied to the created
  account after account creation succeeds.
- Added `release_reserved_grant` so failed account creation does not strand a
  real user behind a stale trial reservation.
- Updated Try Us and confirmed signup to reserve first, create second, attach
  third.
- Added a regression test proving a reserved grant blocks a second same-device
  grant even before the account row is attached.

## Privacy Boundary

The trial ledger continues to store hashed signals only. It does not store raw
IP addresses, user agents, device fingerprints, prompts, transcripts, files, or
screen content.

## Verification

```bash
cargo test --manifest-path server/Cargo.toml trial_abuse --lib -- --nocapture
cargo test --manifest-path server/Cargo.toml --test integration_e2e trial_start_creates_temporary_account_with_fifteen_minutes -- --nocapture
cargo test --manifest-path server/Cargo.toml --test integration_e2e router_embed_consumes_trial_seconds_and_records_bluey_cost -- --nocapture
cargo check --manifest-path server/Cargo.toml
```

All passed.

## Notes

No deploy was performed in this round. The current branch has unrelated
uncommitted work in several of the same files, so this round intentionally
documents the narrow trial-reservation slice.
