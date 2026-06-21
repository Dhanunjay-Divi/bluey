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

Use a different track only when the caller explicitly sets
`BLUEY_POSTGRES_MIGRATIONS_DIR`.
