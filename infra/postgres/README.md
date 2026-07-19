# Bluey Postgres Tracks

Bluey has two Postgres schema tracks:

- `server-runtime/`: the active cutover target. It mirrors the current
  `bluey-server` SQLite tables and adds pgvector-ready columns where the
  runtime needs them next.
- `../migrations/001_initial_cloud_schema.sql`: the future normalized cloud
  model. Keep it as architecture until the server adapter moves to that shape.

Use this for the current cutover target:

```bash
scripts/bluey-postgres-migrate.sh /etc/bluey-api/bluey-api.env
```

The operator runner and the server startup path consume the same numbered SQL
files. In particular, migration `009_jobs_discovery_board_owner.sql` installs
the unique public-board ownership boundary before migration 010 adds provider
usage provenance. Server startup replays these idempotent files to repair
schema drift, but deployment must run the operator command before replacing a
live binary.

Use a different track only when the caller explicitly sets
`BLUEY_POSTGRES_MIGRATIONS_DIR`.
