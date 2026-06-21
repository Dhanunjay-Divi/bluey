# Bluey Migrations

This directory contains the future normalized cloud schema plus legacy mixed
SQL history. It is not the default runtime Postgres migration track.

- `001_initial_cloud_schema.sql` targets **Postgres 16+ with pgvector** for the
  future normalized Bluey cloud source of truth.
- `002_*.sql` and later files currently target **local SQLite** runtime/session
  storage.

Do not run every file in this directory against Postgres.

For the active server-runtime cutover target, use:

```bash
scripts/bluey-postgres-migrate.sh /etc/bluey-api/bluey-api.env
```

That runner defaults to `infra/postgres/server-runtime`, records applied
versions in `bluey_schema_migrations`, and verifies `pgvector` plus a vector
embedding column on the active RAG table.

Only set `BLUEY_POSTGRES_MIGRATIONS_DIR=infra/migrations` when intentionally
testing the future normalized schema in isolation.

Current production note: `bluey-server` remains SQLite-backed until the runtime
SQL backend adapter/backfill cutover lands. `BLUEY_DATABASE_URL` is a
provisioning/cutover input, not a magic switch for the current binary.
