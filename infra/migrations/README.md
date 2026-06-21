# Bluey Migrations

This directory currently contains two kinds of SQL files:

- `001_initial_cloud_schema.sql` targets **Postgres 16+ with pgvector** for the
  future Bluey cloud source of truth.
- `002_*.sql` and later files currently target **local SQLite** runtime/session
  storage.

Do not run every file in this directory against Postgres. Use:

```bash
scripts/bluey-postgres-migrate.sh /etc/bluey-api/bluey-api.env
```

The migration runner applies only files whose header declares
`Target: Postgres`, records applied versions in `bluey_schema_migrations`, and
verifies `pgvector` plus the `memory_chunks.embedding vector` column.

Current production note: `bluey-server` remains SQLite-backed until the runtime
SQL backend adapter/backfill cutover lands. `BLUEY_DATABASE_URL` is a
provisioning/cutover input, not a magic switch for the current binary.
