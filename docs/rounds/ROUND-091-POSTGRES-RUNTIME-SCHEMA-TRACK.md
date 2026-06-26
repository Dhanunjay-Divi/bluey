# Round 091 - Postgres Runtime Schema Track

Date: 2026-06-21

## Why This Round Exists

The user asked to stop waiting on Postgres/Valkey because Bluey has no users yet.
The important distinction at the start of this round was that provisioning
Postgres was safe, but the server code still used the SQLite adapter. The
previous schema in
`infra/migrations/001_initial_cloud_schema.sql` was a future normalized model,
not a compatible target for the current runtime tables.

Update: the follow-up runtime adapter foundation is now tracked in
`docs/rounds/POSTGRES-RUNTIME-ADAPTER-FOUNDATION-2026-06-21.md`. This document
is the schema/readiness precursor.

## What Changed

- Added `infra/postgres/server-runtime/001_server_runtime_compat.sql`.
  - Mirrors the active server tables: accounts, credit batches, auth tokens,
    device codes, usage events, request idempotency, email/password tokens,
    browser link codes, cloud sessions, cloud transcript/cue/context records,
    cloud RAG chunks, STT sessions, and signup OTPs.
  - Adds `cloud_rag_chunks.embedding vector(1536)` and a pgvector index, while
    keeping `embedding_json` for compatibility during adapter/backfill work.

- Updated `scripts/bluey-postgres-migrate.sh`.
  - Defaults to `infra/postgres/server-runtime`.
  - Verifies pgvector and either `cloud_rag_chunks.embedding` or the future
    `memory_chunks.embedding`.

- Updated `scripts/bluey-cloud-preflight.sh`.
  - Reads profile/backend variables after sourcing the env file.
  - Accepts the runtime `cloud_rag_chunks` vector table as a valid pgvector
    readiness signal.

- Added an initial server fail-fast guard for the schema-only phase.
  - This guard has since been replaced by the runtime adapter foundation: a
    Postgres-capable build now requires `BLUEY_DATABASE_URL` and opens a
    Postgres pool when `BLUEY_SERVER_DB_BACKEND=postgres`.

## What Is Now In Place

- Managed Postgres can be provisioned and migrated to a schema that matches the
  runtime Bluey server domain.
- Managed Redis/Valkey remains wired through `BLUEY_REDIS_URL`; the preflight
  forces it for `multi-server` and `postgres-cutover`.
- The cutover target is explicit. We no longer conflate the future normalized
  cloud schema with the active runtime schema.

## What Is Still Not Done

The actual Rust Postgres adapter foundation has since landed, but production is
not cut over yet. Remaining work is now operational rather than schema-only:

- provision managed Postgres and run the server-runtime migrations there
- backfill any existing SQLite data before flipping production
- run parity/live smoke for billing, STT reservations, streaming answers,
  cloud sync/RAG, export/delete, and Square webhooks

Until those pass, the deployed production service should continue on its known
SQLite database.

## Verification

Local tooling added for verification only:

- `brew install pgvector`
- `brew install postgresql@17`

Checks run:

```bash
bash -n scripts/bluey-cloud-preflight.sh scripts/bluey-postgres-migrate.sh
git diff --check
```

Disposable Postgres 17 + pgvector dry run:

```text
ok: Postgres migrations complete (1 applied, 0 non-Postgres file(s) skipped)
accounts|cloud_rag_chunks|vector
```

Disposable preflight with fake non-secret provider/R2 values:

```text
ok: BLUEY_SERVER_DB_BACKEND=postgres
ok: Postgres connection succeeded
ok: pgvector extension installed
ok: cloud RAG table present (cloud_rag_chunks)
ok: cloud RAG embedding column uses pgvector
preflight passed: 5 warning(s)
```

The warnings were expected in the disposable test: no real Redis URL, no real
R2 bucket, no local health server, and no hosted signed manifest check.

## Manual Deploy

After review, the server build containing the original Postgres backend guard was
manually deployed to the production droplet without GitHub Actions:

```text
commit: 4937a4c
binary: /usr/local/bin/bluey-server
service: bluey-api
```

Deployment checks:

```text
systemd: active
local health: 200
public health: 200
health commit: 4937a4c
```

Production preflight after deploy:

```text
ok: BLUEY_REDIS_URL set
ok: Redis/Valkey ping succeeded
ok: R2/S3 backup destination reachable
ok: health endpoint reachable
ok: signed update manifest files reachable
preflight passed: 3 warning(s)
```

The warnings are intentional for the current single-server alpha:

- `BLUEY_DATABASE_URL` was unset because the deployed runtime at that moment used
  SQLite. The later adapter foundation makes Postgres selectable, but production
  still needs provisioning/backfill/smoke before the env flip.
- Redis points to the droplet-local ledger, which is valid only for one server.
- Redis strict mode is disabled so a local Redis outage does not take the
  single-server alpha offline.
