# Postgres / Valkey Readiness Gates

Date: 2026-06-21

## Why This Round Exists

Bluey has the right long-term architecture on paper:

- user laptop: local SQLite, files, and local RAG cache
- Bluey server: Postgres + pgvector, Redis/Valkey, R2/S3 object storage
- providers: OpenAI, Anthropic, Gemini, and Deepgram through Bluey server only

The gap was operational sharpness. Redis/Valkey already exists in server code,
and a Postgres schema already existed, but there was no safe migration runner
or preflight profile that forces the right managed services before we scale past
one server.

Update: the later runtime adapter foundation is documented in
`docs/rounds/POSTGRES-RUNTIME-ADAPTER-FOUNDATION-2026-06-21.md`. This readiness
round still defines the operational gates.

## What Changed

- `scripts/bluey-cloud-preflight.sh`
  - Adds `BLUEY_PREFLIGHT_PROFILE`.
  - `single-server-alpha`: SQLite plus optional local Redis is allowed.
  - `multi-server`: requires managed Redis/Valkey and strict Redis behavior.
  - `postgres-cutover`: requires managed Redis/Valkey plus `BLUEY_DATABASE_URL`.
  - `postgres-cutover` also requires `BLUEY_SERVER_DB_BACKEND=postgres` so a
    SQLite binary cannot accidentally pass the cutover gate.
  - Detects local Redis URLs and fails them for multi-server/postgres profiles.
  - Verifies the cloud Postgres schema has `memory_chunks` when
    `BLUEY_DATABASE_URL` is configured.

- `scripts/bluey-postgres-migrate.sh`
  - Loads the same env file style as the API service.
  - Requires `BLUEY_DATABASE_URL`.
  - Defaults to `infra/postgres/server-runtime`, not the future normalized
    schema.
  - Records applied versions in `bluey_schema_migrations`.
  - Verifies pgvector plus the active RAG vector column after migration.
  - Never prints the database URL.

- `infra/postgres/server-runtime/001_server_runtime_compat.sql`
  - Adds the managed Postgres schema that mirrors today’s server tables:
    accounts, credits, auth, idempotency, usage, cloud sessions, cloud RAG,
    and STT sessions.
  - Adds `cloud_rag_chunks.embedding vector(1536)` so pgvector is present
    without pretending the runtime adapter is already using it.

- `ops/bluey-api.env.example`
  - Documents preflight profiles.
  - Documents the managed Postgres migration command.

- `infra/migrations/README.md`
  - Clarifies that this folder is currently mixed Postgres and SQLite SQL.
  - Directs operators to the Postgres migration runner.

## Current Truth

Redis/Valkey:

- Runtime support is already real through `BLUEY_REDIS_URL`.
- Single-server alpha can use no Redis or local Redis.
- More than one server process must use managed Redis/Valkey with:

```bash
BLUEY_REDIS_URL=rediss://...
BLUEY_REDIS_NAMESPACE=bluey-prod
BLUEY_RATE_LIMIT_REDIS_STRICT=1
BLUEY_PREFLIGHT_PROFILE=multi-server
```

Postgres/pgvector:

- The active server-runtime schema is provisionable and migratable now.
- The older normalized cloud schema remains tracked as a future target, not
  the default runtime track.
- A Postgres runtime adapter foundation now exists for account/auth/billing,
  usage, idempotency, STT, sync/RAG, metrics, export, and delete paths.
- Do not flip production merely because the adapter compiles. Production still
  needs managed Postgres provisioning, SQLite-to-Postgres backfill if data
  exists, parity checks, and paid live smoke.
- Set `BLUEY_SERVER_DB_BACKEND=postgres` and `BLUEY_DATABASE_URL` only for a
  deliberate cutover/staging smoke, then run the `postgres-cutover` preflight
  profile before promotion.

## Operator Commands

Single-server alpha:

```bash
BLUEY_PREFLIGHT_PROFILE=single-server-alpha \
  scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env
```

Multi-server readiness:

```bash
BLUEY_PREFLIGHT_PROFILE=multi-server \
  scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env
```

Postgres provisioning:

```bash
scripts/bluey-postgres-migrate.sh /etc/bluey-api/bluey-api.env

BLUEY_PREFLIGHT_PROFILE=postgres-cutover \
  BLUEY_SERVER_DB_BACKEND=postgres \
  scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env
```

## Remaining Follow-Up

The real production cutover is still not a switch-flip:

1. Provision managed Postgres + pgvector and run the tracked migrations.
2. Backfill SQLite to Postgres while preserving account ids, processor payment
   ids, request ids, usage ids, STT reservations, cloud session ids, RAG chunk
   ids, and tombstones.
3. Run parity and paid smoke for Square webhooks, managed streaming, STT
   reservations, RAG, exports, and deletion on staging.
4. Promote `BLUEY_SERVER_DB_BACKEND=postgres` only after the above checks pass.

This round closes the dangerous ambiguity. The adapter foundation is now real;
live cutover remains a controlled deployment project.

## Verification

Run before committing this round:

```bash
bash -n scripts/bluey-cloud-preflight.sh scripts/bluey-postgres-migrate.sh
git diff --check
```
