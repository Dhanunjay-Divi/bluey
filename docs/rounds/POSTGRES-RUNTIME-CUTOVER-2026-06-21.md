# Postgres Runtime Cutover Plan

Date: 2026-06-21

## Why We Should Do This Now

Bluey has no real customer data yet, so this is the cheapest time to move the
server runtime away from SQLite. If we wait until paid users accumulate account
balances, payment events, STT reservations, sessions, and RAG records, the same
cutover becomes a higher-risk dual-write/backfill project.

The important truth is that Postgres is not a config flip today. The server
schema track exists under `infra/postgres/server-runtime`, but Rust runtime code
still treats the database as a SQLite pool. Many auth, billing, STT, usage, sync,
admin, metrics, and account-export paths call `rusqlite` directly.

## Current State

Already done:

- Postgres/pgvector runtime-compatible schema:
  `infra/postgres/server-runtime/001_server_runtime_compat.sql`
- Postgres migration runner:
  `scripts/bluey-postgres-migrate.sh`
- Cloud preflight profiles:
  `single-server-alpha`, `multi-server`, and `postgres-cutover`
- Runtime guard:
  `BLUEY_SERVER_DB_BACKEND=postgres` fails on the current SQLite-backed binary
  instead of pretending Postgres is active.
- SQLite leakage inventory:
  `scripts/check-server-sqlite-boundary.sh`
- Runtime boundary drain started:
  - refresh-token storage moved to `server/src/db/refresh_tokens.rs`
  - signup OTP/device-code/account password updates moved to `server/src/db/**`
  - STT reservation/claim/settlement moved to `server/src/db/stt_accounting.rs`
  - boundary inventory dropped from 89 direct SQLite-bound lines outside
    `server/src/db/**` to 41.

Still missing:

- A real database boundary in Rust.
- A Postgres implementation for each server storage path.
- Backfill/parity tooling.
- Postgres-mode smoke tests for money/auth/STT/RAG paths.
- Remaining direct SQLite islands:
  - account dashboard/export
  - billing webhook/card metadata
  - admin customer list
  - metrics counters

## Why It Cannot Be Blindly Flipped

The current type alias is effectively:

```rust
pub type DbPool = Pool<SqliteConnectionManager>;
```

That means most call sites expect a synchronous SQLite connection and SQLite SQL
semantics. Postgres needs different connection types, placeholders, returning
behavior, transactions, timestamp types, error classification, and lock/update
semantics. The riskiest paths are also the money paths:

- credit batches and balance deduction
- STT reserve/settle/refund
- Square/Stripe webhook idempotency
- request idempotency for streamed answers
- refresh tokens and device/link login
- cloud sync deletion/tombstones and RAG chunks

## Cutover Strategy

Because there are no real users yet, prefer a direct controlled cutover over a
long dual-write period:

1. Freeze new SQLite leakage.
   - Run `scripts/check-server-sqlite-boundary.sh`.
   - During adapter work, run it with `BLUEY_SQLITE_BOUNDARY_STRICT=1`.
   - New code should land in `server/src/db/**` or backend-specific modules, not
     in API handlers.

2. Introduce a server database boundary.
   - Keep the API/router shape stable.
   - Move direct SQL out of `server/src/api/**` and `server/src/auth/**`.
   - Expose domain functions for accounts, auth tokens, balance, usage,
     idempotency, billing events, STT sessions, cloud sync, and RAG.

3. Implement Postgres backend in the same domain order:
   - accounts + refresh/auth/device/link codes
   - balance + credit batches + usage
   - idempotency + streaming billing
   - billing webhooks + auto reload metadata
   - STT session accounting
   - cloud sessions/transcripts/context/RAG
   - account export/admin/metrics

4. Build backfill/parity.
   - Export SQLite rows with stable IDs.
   - Import into Postgres in foreign-key order.
   - Compare row counts and money totals:
     accounts, balances, reserved cents, credit batches, usage spend, STT
     reservations, webhook event ids, request idempotency ids, and RAG chunks.

5. Staging cutover.
   - Apply Postgres migrations.
   - Run backfill from a SQLite copy.
   - Start server with:
     `BLUEY_SERVER_DB_BACKEND=postgres`
     and `BLUEY_PREFLIGHT_PROFILE=postgres-cutover`.
   - Smoke: signup/login, Square checkout/webhook, manual reload, auto reload,
     managed answer streaming, Deepgram STT reserve/settle, docs/sync/RAG,
     delete/export, and admin views.

6. Production cutover before paid users.
   - Stop API briefly.
   - Take SQLite + R2 backup.
   - Backfill Postgres.
   - Run parity checks.
   - Flip env and restart.
   - Keep SQLite backup read-only for rollback.

## When Dual-Write Becomes Required

If we gain real users before the adapter is complete, do not do a blind offline
cutover. Add dual-write and reconciliation first:

- write every money/auth/session mutation to SQLite and Postgres
- compare totals continuously
- switch reads to Postgres only after parity is stable
- keep SQLite as rollback until webhook, STT, and answer billing paths prove
  stable

## Verification Commands

```bash
scripts/check-server-sqlite-boundary.sh

scripts/bluey-postgres-migrate.sh /etc/bluey-api/bluey-api.env

BLUEY_PREFLIGHT_PROFILE=postgres-cutover \
  BLUEY_SERVER_DB_BACKEND=postgres \
  scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env
```

## Decision

Yes: we should get the Postgres runtime adapter done before wider paid alpha.
Provisioning alone is not enough, and waiting makes the migration more
dangerous. The cleanest path is to finish the adapter now, while production
data can still be recreated or backfilled with low risk.
