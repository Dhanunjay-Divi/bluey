#!/usr/bin/env bash
set -euo pipefail

# Apply Bluey's cloud Postgres/pgvector migrations.
#
# Usage:
#   scripts/bluey-postgres-migrate.sh /etc/bluey-api/bluey-api.env
#
# This script intentionally applies only SQL files that declare
# "Target: Postgres" in their header. The infra/migrations directory also
# contains local SQLite migrations for the desktop/runtime cache.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${1:-}"
MIGRATIONS_DIR="${BLUEY_POSTGRES_MIGRATIONS_DIR:-$ROOT/infra/migrations}"
MIGRATION_TABLE="${BLUEY_POSTGRES_MIGRATION_TABLE:-bluey_schema_migrations}"

if [ -n "$ENV_FILE" ]; then
  if [ ! -f "$ENV_FILE" ]; then
    echo "fatal: env file not found: $ENV_FILE" >&2
    exit 2
  fi
  set -a
  # shellcheck disable=SC1090
  . "$ENV_FILE"
  set +a
fi

if [ -z "${BLUEY_DATABASE_URL:-}" ]; then
  echo "fatal: BLUEY_DATABASE_URL is required" >&2
  exit 2
fi

if ! command -v psql >/dev/null 2>&1; then
  echo "fatal: psql is required" >&2
  exit 2
fi

if ! [[ "$MIGRATION_TABLE" =~ ^[a-zA-Z_][a-zA-Z0-9_]*$ ]]; then
  echo "fatal: BLUEY_POSTGRES_MIGRATION_TABLE must be a simple SQL identifier" >&2
  exit 2
fi

echo "Bluey Postgres migration"
echo "env: ${ENV_FILE:-current shell}"
echo "migrations: $MIGRATIONS_DIR"
echo "database: configured BLUEY_DATABASE_URL (redacted)"

psql "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc "select 1" >/dev/null

psql "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -q <<SQL
CREATE TABLE IF NOT EXISTS ${MIGRATION_TABLE} (
  version TEXT PRIMARY KEY,
  applied_at TIMESTAMPTZ NOT NULL DEFAULT now()
);
SQL

applied=0
skipped=0

for migration in "$MIGRATIONS_DIR"/*.sql; do
  [ -f "$migration" ] || continue
  if ! sed -n '1,12p' "$migration" | grep -qi 'Target:[[:space:]]*Postgres'; then
    skipped=$((skipped + 1))
    continue
  fi

  version="$(basename "$migration")"
  already="$(psql "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc "select 1 from ${MIGRATION_TABLE} where version = '$version'")"
  if [ "$already" = "1" ]; then
    echo "skip: $version already applied"
    continue
  fi

  echo "apply: $version"
  psql "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 --single-transaction -f "$migration" >/dev/null
  psql "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc "insert into ${MIGRATION_TABLE}(version) values ('$version')"
  applied=$((applied + 1))
done

if ! psql "$BLUEY_DATABASE_URL" -Atqc "select 1 from pg_extension where extname = 'vector'" | grep -q 1; then
  echo "fatal: pgvector extension is missing after migrations" >&2
  exit 1
fi

if ! psql "$BLUEY_DATABASE_URL" -Atqc "select to_regclass('public.memory_chunks')" | grep -q memory_chunks; then
  echo "fatal: memory_chunks table missing after migrations" >&2
  exit 1
fi

embedding_type="$(psql "$BLUEY_DATABASE_URL" -Atqc "select udt_name from information_schema.columns where table_schema = 'public' and table_name = 'memory_chunks' and column_name = 'embedding'")"
if [ "$embedding_type" != "vector" ]; then
  echo "fatal: memory_chunks.embedding is not pgvector (found: ${embedding_type:-missing})" >&2
  exit 1
fi

echo "ok: Postgres migrations complete ($applied applied, $skipped non-Postgres file(s) skipped)"
