# Postgres Runtime Schema Track

Date: 2026-06-21

## Why This Round Exists

The user asked to stop waiting on Postgres/Valkey because Bluey has no users yet.
The important distinction was that provisioning Postgres is safe now, but the
server code still uses the SQLite adapter. The previous schema in
`infra/migrations/001_initial_cloud_schema.sql` was a future normalized model,
not a compatible target for the current runtime tables.

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

- Added a server fail-fast guard.
  - If `BLUEY_SERVER_DB_BACKEND=postgres` is set on the current SQLite-backed
    binary, startup fails loudly instead of silently opening SQLite.

## What Is Now In Place

- Managed Postgres can be provisioned and migrated to a schema that matches the
  runtime Bluey server domain.
- Managed Redis/Valkey remains wired through `BLUEY_REDIS_URL`; the preflight
  forces it for `multi-server` and `postgres-cutover`.
- The cutover target is explicit. We no longer conflate the future normalized
  cloud schema with the active runtime schema.

## What Is Still Not Done

The actual Rust Postgres adapter is still a code project. That means:

- current production server continues to run SQLite unless a Postgres adapter
  build is deployed
- no users are migrated yet
- no dual-write/backfill is active yet

This is intentional. The new fail-fast guard prevents accidental fake cutover.

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

After review, the server build containing the Postgres backend guard was
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

- `BLUEY_DATABASE_URL` is unset because the deployed runtime is still
  SQLite-backed until the Postgres adapter build exists.
- Redis points to the droplet-local ledger, which is valid only for one server.
- Redis strict mode is disabled so a local Redis outage does not take the
  single-server alpha offline.
