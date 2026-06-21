# Postgres Runtime Adapter Foundation

Date: 2026-06-21

## Why This Round Exists

The user explicitly asked to stop treating Postgres as a future-only plan. The
previous rounds had migrations, readiness gates, and deployment notes, but the
server runtime still could not safely open a Postgres backend. That was a real
gap because moving later would only become harder once paid users and billing
records exist.

This round adds the first real Postgres runtime adapter foundation while keeping
SQLite as the default/local backend until managed Postgres provisioning,
backfill, and paid smoke are complete.

## What Changed

- Added Postgres runtime dependencies to `server/Cargo.toml`.
- Added `BLUEY_DATABASE_URL` to server config.
- Reworked `server/src/db/mod.rs` so `DbPool` can be either:
  - `Sqlite(SqliteDbPool)`
  - `Postgres(PostgresDbPool)`
- Added `open_postgres_pool` and Postgres migration dispatch against
  `infra/postgres/server-runtime`.
- Updated `server/src/main.rs` so `BLUEY_SERVER_DB_BACKEND=postgres` opens a
  real Postgres pool instead of refusing startup.
- Added Postgres branches behind the existing server DB boundary for:
  - accounts and wallet/billing state
  - auth tokens, refresh tokens, signup OTPs, device codes, and link codes
  - usage events and usage summaries
  - request idempotency
  - webhook event storage
  - STT reservations/accounting
  - cloud sync sessions, transcript turns, answer records, context artifacts,
    and cloud RAG chunks
  - metrics snapshots
  - account export and hard delete
- Cloud RAG writes now populate `cloud_rag_chunks.embedding vector(1536)` when
  the embedding has the supported dimension, while preserving `embedding_json`
  for compatibility.
- RAG retrieval filters out tombstoned/deleted cloud sessions on both SQLite
  and Postgres paths.
- Account export now includes stored Square billing identifiers/card metadata
  in addition to existing Stripe identifiers.

## What Is True Now

The server code now has a real selectable Postgres runtime path for the major
paid-alpha surfaces: auth, account, billing ledger primitives, STT accounting,
managed usage, idempotency, cloud sync/RAG, metrics, export, and delete.

This is no longer just a schema or preflight plan.

## What Is Still Not Done

Production is not cut over yet.

Remaining required work before `BLUEY_SERVER_DB_BACKEND=postgres` becomes the
live production setting:

1. Provision managed Postgres 16+ with pgvector.
2. Run `scripts/bluey-postgres-migrate.sh` against the managed database.
3. Backfill any existing SQLite production data while preserving account ids,
   payment ids, request ids, STT session ids, cloud session ids, RAG chunk ids,
   and deletion tombstones.
4. Run parity checks between SQLite and Postgres for account balances, usage,
   webhooks, sync sessions, and exports.
5. Run paid smoke on a staging/postgres env:
   signup, login, add credits, Square webhook crediting, Listen, Answer,
   Screen, Docs, RAG recall, export/delete, and low-balance behavior.
6. Only then flip production env to:

```bash
BLUEY_SERVER_DB_BACKEND=postgres
BLUEY_DATABASE_URL=postgres://...
```

## Most Likely Risk Areas

- SQL behavior differences: SQLite and Postgres do not share identical time,
  JSON, transaction, and conflict behavior.
- Cloud RAG vector scoring: Postgres pgvector score ordering is now native, but
  must be smoke-tested with real embeddings.
- Billing/idempotency races: adapter branches compile, but live concurrent
  paid traffic needs focused load tests before wider alpha.
- Backfill correctness: no live cutover should happen without row-count and
  ledger-total comparisons.

## Verification

Completed locally:

```bash
cargo check --manifest-path server/Cargo.toml
cargo fmt --all --check
cargo test --manifest-path server/Cargo.toml --lib
bash -n scripts/bluey-cloud-preflight.sh scripts/bluey-postgres-migrate.sh \
  scripts/bluey-scalable-readiness.sh scripts/check-server-sqlite-boundary.sh
```

Still required before live cutover:

```bash
scripts/bluey-postgres-migrate.sh /path/to/postgres.env
BLUEY_PREFLIGHT_PROFILE=postgres-cutover \
  BLUEY_SERVER_DB_BACKEND=postgres \
  scripts/bluey-cloud-preflight.sh /path/to/postgres.env
```

## Operational Guidance

Do not tell the team or users that production is Postgres-backed until the
cutover is actually deployed and verified. The correct wording is:

> Postgres runtime support has landed in the server code. Production remains on
> the current database until managed Postgres provisioning, migration/backfill,
> parity checks, and paid smoke pass.
