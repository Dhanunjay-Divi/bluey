#!/usr/bin/env bash
set -euo pipefail

# Apply Bluey's Postgres/pgvector migrations.
#
# Usage:
#   scripts/bluey-postgres-migrate.sh /etc/bluey-api/bluey-api.env
#
# By default this applies the server-runtime compatibility schema under
# infra/postgres/server-runtime. That schema mirrors the current bluey-server
# SQLite tables so a real Postgres cutover has an exact target. Future
# normalized cloud schemas must be run by explicitly setting
# BLUEY_POSTGRES_MIGRATIONS_DIR.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ENV_FILE="${1:-}"

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

MIGRATIONS_DIR="${BLUEY_POSTGRES_MIGRATIONS_DIR:-$ROOT/infra/postgres/server-runtime}"
MIGRATION_TABLE="${BLUEY_POSTGRES_MIGRATION_TABLE:-bluey_schema_migrations}"

if [ -z "${BLUEY_DATABASE_URL:-}" ]; then
  echo "fatal: BLUEY_DATABASE_URL is required" >&2
  exit 2
fi

if ! command -v psql >/dev/null 2>&1; then
  echo "fatal: psql is required" >&2
  exit 2
fi

LC_ALL=C
export LC_ALL
if ! [[ "$MIGRATION_TABLE" =~ ^[a-zA-Z_][a-zA-Z0-9_]*$ ]]; then
  echo "fatal: BLUEY_POSTGRES_MIGRATION_TABLE must be a simple SQL identifier" >&2
  exit 2
fi
MIGRATION_TABLE="$(printf '%s' "$MIGRATION_TABLE" | LC_ALL=C tr 'A-Z' 'a-z')"
if [ "${#MIGRATION_TABLE}" -gt 63 ]; then
  echo "fatal: BLUEY_POSTGRES_MIGRATION_TABLE exceeds 63 bytes" >&2
  exit 2
fi

echo "Bluey Postgres migration"
echo "env: ${ENV_FILE:-current shell}"
echo "migrations: $MIGRATIONS_DIR"
echo "database: configured BLUEY_DATABASE_URL (redacted)"

# Startup files must not redirect a migration connection or alter its session.
# This applies to both owner-wide and system-wide psqlrc files.
psql -X "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc "select 1" >/dev/null

# The unreleased standalone ATS branch used 045 before provisioning reserved
# 045/046. Never reinterpret its history as the integrated 047 migration.
# Check the normal ledger even when an operator selects another ledger table.
# These are reads only; no ledger/schema repair or creation precedes refusal.
for lineage_table in bluey_schema_migrations "$MIGRATION_TABLE"; do
  lineage_exists="$(psql -X "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc \
    "select case when to_regclass('\"${lineage_table}\"') is null then '0' else '1' end")"
  case "$lineage_exists" in
    0) continue ;;
    1) ;;
    *) echo "fatal: migration lineage inspection was not canonical" >&2; exit 1 ;;
  esac
  legacy_ats="$(psql -X "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc \
    "select case when exists(select 1 from \"${lineage_table}\" where version = '045_jobs_ats_storage_fences.sql') then '1' else '0' end")"
  case "$legacy_ats" in
    0) ;;
    1) echo "fatal: unreleased ATS migration lineage requires explicit upgrade review" >&2; exit 1 ;;
    *) echo "fatal: migration lineage inspection was not canonical" >&2; exit 1 ;;
  esac
done

psql -X "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -q <<SQL
CREATE TABLE IF NOT EXISTS "${MIGRATION_TABLE}" (
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
  already="$(psql -X "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc "select 1 from \"${MIGRATION_TABLE}\" where version = '$version'")"
  if [ "$already" = "1" ]; then
    echo "skip: $version already applied"
    continue
  fi

  echo "apply: $version"
  psql -X "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 --single-transaction \
    -c "SET bluey.migration_table = '$MIGRATION_TABLE'" -f "$migration" >/dev/null
  # Some self-contained migrations record this same version in their owned
  # transaction. Preserve that row and its original timestamp on exact replay.
  psql -X "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc "insert into \"${MIGRATION_TABLE}\"(version) values ('$version') on conflict (version) do nothing"
  applied=$((applied + 1))
done

if ! psql -X "$BLUEY_DATABASE_URL" -Atqc "select 1 from pg_extension where extname = 'vector'" | grep -q 1; then
  echo "fatal: pgvector extension is missing after migrations" >&2
  exit 1
fi

if ! psql -X "$BLUEY_DATABASE_URL" -Atqc "select coalesce(to_regclass('public.cloud_rag_chunks')::text, to_regclass('public.memory_chunks')::text, '')" | grep -Eq 'cloud_rag_chunks|memory_chunks'; then
  echo "fatal: no pgvector-backed RAG table found after migrations" >&2
  exit 1
fi

embedding_type="$(psql -X "$BLUEY_DATABASE_URL" -Atqc "select udt_name from information_schema.columns where table_schema = 'public' and table_name = 'cloud_rag_chunks' and column_name = 'embedding' union all select udt_name from information_schema.columns where table_schema = 'public' and table_name = 'memory_chunks' and column_name = 'embedding' limit 1")"
if [ "$embedding_type" != "vector" ]; then
  echo "fatal: RAG embedding column is not pgvector (found: ${embedding_type:-missing})" >&2
  exit 1
fi

echo "ok: Postgres migrations complete ($applied applied, $skipped non-Postgres file(s) skipped)"
