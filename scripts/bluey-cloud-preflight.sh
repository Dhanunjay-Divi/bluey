#!/usr/bin/env bash
set -euo pipefail

# Bluey production architecture preflight.
#
# Usage:
#   scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env
#
# The script never prints secret values. Missing optional CLIs are warnings
# unless BLUEY_PREFLIGHT_STRICT=1 is set.

ENV_FILE="${1:-}"
STRICT="${BLUEY_PREFLIGHT_STRICT:-0}"
PROFILE="${BLUEY_PREFLIGHT_PROFILE:-single-server-alpha}"
REQUIRE_POSTGRES="${BLUEY_REQUIRE_POSTGRES:-0}"
REQUIRE_MANAGED_REDIS="${BLUEY_REQUIRE_MANAGED_REDIS:-0}"
SERVER_DB_BACKEND="${BLUEY_SERVER_DB_BACKEND:-sqlite}"
FAILURES=0
WARNINGS=0

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

ok() {
  printf 'ok: %s\n' "$1"
}

warn() {
  WARNINGS=$((WARNINGS + 1))
  printf 'warn: %s\n' "$1" >&2
}

fail() {
  FAILURES=$((FAILURES + 1))
  printf 'fail: %s\n' "$1" >&2
}

is_placeholder() {
  case "$1" in
    ""|replace-*|replace_with_*|*replace-with*|*xxxxx*|*XXXX*|*:password@*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

need_env() {
  local name="$1"
  local purpose="$2"
  if [ -n "${!name:-}" ] && ! is_placeholder "${!name:-}"; then
    ok "$name set ($purpose)"
  elif [ -n "${!name:-}" ]; then
    fail "$name still has a placeholder value ($purpose)"
  else
    fail "$name missing ($purpose)"
  fi
}

optional_env() {
  local name="$1"
  local purpose="$2"
  if [ -n "${!name:-}" ] && ! is_placeholder "${!name:-}"; then
    ok "$name set ($purpose)"
  elif [ -n "${!name:-}" ]; then
    warn "$name still has a placeholder value ($purpose)"
  else
    warn "$name missing ($purpose)"
  fi
}

key_count() {
  local value="$1"
  awk -v raw="$value" 'BEGIN {
    n=split(raw, parts, ",");
    count=0;
    for (i=1; i<=n; i++) {
      gsub(/^[ \t]+|[ \t]+$/, "", parts[i]);
      if (parts[i] != "") count++;
    }
    print count;
  }'
}

is_local_redis_url() {
  case "$1" in
    redis://127.0.0.1:*|redis://localhost:*|rediss://127.0.0.1:*|rediss://localhost:*|unix:*)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

echo "Bluey cloud preflight"
echo "env: ${ENV_FILE:-current shell}"
echo "profile: $PROFILE"

case "$PROFILE" in
  single-server-alpha)
    ;;
  multi-server)
    REQUIRE_MANAGED_REDIS=1
    ;;
  postgres-cutover)
    REQUIRE_MANAGED_REDIS=1
    REQUIRE_POSTGRES=1
    ;;
  *)
    warn "unknown BLUEY_PREFLIGHT_PROFILE=$PROFILE; use single-server-alpha, multi-server, or postgres-cutover"
    ;;
esac

need_env BLUEY_PUBLIC_URL "public web/API origin"
need_env BLUEY_JWT_SECRET "JWT signing"
if [ -n "${BLUEY_JWT_SECRET:-}" ] && [ "${#BLUEY_JWT_SECRET}" -lt 32 ]; then
  fail "BLUEY_JWT_SECRET must be at least 32 characters"
fi

if [ -n "${BLUEY_DB_PATH:-}" ]; then
  db_parent="$(dirname "$BLUEY_DB_PATH")"
  if [ -d "$db_parent" ] && [ -w "$db_parent" ]; then
    ok "SQLite DB parent writable: $db_parent"
  else
    fail "SQLite DB parent is not writable: $db_parent"
  fi
else
  warn "BLUEY_DB_PATH unset; server will default to ./bluey-dev.db"
fi

if [ -n "${BLUEY_DATABASE_URL:-}" ]; then
  if [ "$SERVER_DB_BACKEND" = "postgres" ]; then
    ok "BLUEY_SERVER_DB_BACKEND=postgres"
  elif [ "$REQUIRE_POSTGRES" = "1" ]; then
    fail "BLUEY_SERVER_DB_BACKEND=postgres required for BLUEY_PREFLIGHT_PROFILE=$PROFILE"
  else
    warn "BLUEY_DATABASE_URL is set but BLUEY_SERVER_DB_BACKEND=${SERVER_DB_BACKEND}; current bluey-server is SQLite-backed unless a Postgres cutover build is deployed"
  fi
  if command -v psql >/dev/null 2>&1; then
    if psql "$BLUEY_DATABASE_URL" -Atqc "select 1" >/dev/null 2>&1; then
      ok "Postgres connection succeeded"
      if psql "$BLUEY_DATABASE_URL" -Atqc "select 1 from pg_extension where extname = 'vector'" | grep -q 1; then
        ok "pgvector extension installed"
      else
        fail "pgvector extension missing"
      fi
      if psql "$BLUEY_DATABASE_URL" -Atqc "select to_regclass('public.memory_chunks')" | grep -q memory_chunks; then
        ok "cloud memory_chunks table present"
      else
        fail "cloud Postgres schema missing; run scripts/bluey-postgres-migrate.sh"
      fi
    else
      fail "Postgres connection failed"
    fi
  else
    warn "psql not installed; skipped Postgres connectivity check"
  fi
elif [ "$REQUIRE_POSTGRES" = "1" ]; then
  fail "BLUEY_DATABASE_URL missing; required for BLUEY_PREFLIGHT_PROFILE=$PROFILE"
else
  warn "BLUEY_DATABASE_URL unset; SQLite-backed server is acceptable only for single-server alpha"
fi

need_env BLUEY_BILLING_PROVIDER "billing provider"
if [ "${BLUEY_BILLING_PROVIDER:-}" = "square" ]; then
  need_env SQUARE_ENVIRONMENT "Square sandbox/production selector"
  if [ "${SQUARE_ENVIRONMENT:-sandbox}" = "production" ]; then
    need_env SQUARE_PRODUCTION_ACCESS_TOKEN "Square production API"
    need_env SQUARE_PRODUCTION_LOCATION_ID "Square production location"
    need_env SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY "Square production webhook verification"
  else
    need_env SQUARE_SANDBOX_ACCESS_TOKEN "Square sandbox API"
    need_env SQUARE_SANDBOX_LOCATION_ID "Square sandbox location"
    need_env SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY "Square sandbox webhook verification"
  fi
fi

for provider in OPENAI_API_KEYS ANTHROPIC_API_KEYS GEMINI_API_KEYS DEEPGRAM_API_KEYS; do
  value="${!provider:-}"
  if [ -n "$value" ] && ! is_placeholder "$value"; then
    ok "$provider set ($(key_count "$value") key(s))"
  elif [ -n "$value" ]; then
    fail "$provider still has a placeholder value"
  else
    fail "$provider missing"
  fi
done

optional_env BLUEY_SMTP_HOST "transactional email"
optional_env BLUEY_SMTP_PASSWORD "transactional email secret"

if [ -n "${BLUEY_REDIS_URL:-}" ]; then
  ok "BLUEY_REDIS_URL set (shared capacity ledger)"
  if is_local_redis_url "$BLUEY_REDIS_URL"; then
    if [ "$REQUIRE_MANAGED_REDIS" = "1" ]; then
      fail "BLUEY_REDIS_URL points to local Redis/Valkey; managed Redis/Valkey is required for BLUEY_PREFLIGHT_PROFILE=$PROFILE"
    else
      warn "BLUEY_REDIS_URL points to local Redis/Valkey; safe only for one-server alpha"
    fi
  fi
  if command -v redis-cli >/dev/null 2>&1; then
    if redis-cli -u "$BLUEY_REDIS_URL" PING 2>/dev/null | grep -q PONG; then
      ok "Redis/Valkey ping succeeded"
    else
      fail "Redis/Valkey ping failed"
    fi
  else
    warn "redis-cli not installed; skipped Redis/Valkey ping"
  fi
else
  if [ "$REQUIRE_MANAGED_REDIS" = "1" ]; then
    fail "BLUEY_REDIS_URL unset; managed Redis/Valkey is required for BLUEY_PREFLIGHT_PROFILE=$PROFILE"
  else
    warn "BLUEY_REDIS_URL unset; safe only for single-server alpha"
  fi
fi
ok "BLUEY_REDIS_NAMESPACE=${BLUEY_REDIS_NAMESPACE:-bluey}"
if [ "${BLUEY_RATE_LIMIT_REDIS_STRICT:-0}" = "1" ]; then
  ok "Redis strict mode enabled"
elif [ "$REQUIRE_MANAGED_REDIS" = "1" ]; then
  fail "BLUEY_RATE_LIMIT_REDIS_STRICT=1 required for BLUEY_PREFLIGHT_PROFILE=$PROFILE"
else
  warn "Redis strict mode disabled; Redis failures fall back to local process state"
fi

if [ -n "${OFFSITE_DESTINATION:-}" ]; then
  ok "OFFSITE_DESTINATION set"
  case "$OFFSITE_DESTINATION" in
    s3://*)
      need_env BLUEY_BACKUP_S3_ENDPOINT_URL "R2/S3-compatible backup endpoint"
      need_env AWS_ACCESS_KEY_ID "R2/S3 backup access key"
      need_env AWS_SECRET_ACCESS_KEY "R2/S3 backup secret"
      if command -v aws >/dev/null 2>&1; then
        aws_args=()
        if [ -n "${BLUEY_BACKUP_S3_ENDPOINT_URL:-}" ]; then
          aws_args+=(--endpoint-url "$BLUEY_BACKUP_S3_ENDPOINT_URL")
        fi
        if aws "${aws_args[@]}" s3 ls "$OFFSITE_DESTINATION" >/dev/null 2>&1; then
          ok "R2/S3 backup destination reachable"
        else
          warn "R2/S3 backup destination not listable; verify bucket policy and prefix"
        fi
      else
        warn "aws CLI not installed; skipped R2/S3 backup check"
      fi
      ;;
    *)
      warn "OFFSITE_DESTINATION is not s3://; preflight cannot verify it generically"
      ;;
  esac
else
  fail "OFFSITE_DESTINATION missing; off-host backups are required before paid users"
fi

if [ -n "${BLUEY_PUBLIC_URL:-}" ] && command -v curl >/dev/null 2>&1; then
  if curl -fsS "${BLUEY_PUBLIC_URL%/}/health" >/dev/null 2>&1; then
    ok "health endpoint reachable"
  else
    warn "health endpoint not reachable from this machine"
  fi
  if curl -fsS "${BLUEY_PUBLIC_URL%/}/latest.json" >/dev/null 2>&1 &&
     curl -fsS "${BLUEY_PUBLIC_URL%/}/latest.json.sig" >/dev/null 2>&1; then
    ok "signed update manifest files reachable"
  else
    warn "signed update manifest files not both reachable"
  fi
fi

if [ "$STRICT" = "1" ] && [ "$WARNINGS" -gt 0 ]; then
  fail "strict mode treats warnings as failures ($WARNINGS warning(s))"
fi

if [ "$FAILURES" -gt 0 ]; then
  echo "preflight failed: $FAILURES failure(s), $WARNINGS warning(s)" >&2
  exit 1
fi

echo "preflight passed: $WARNINGS warning(s)"
