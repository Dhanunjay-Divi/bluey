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

echo "Bluey cloud preflight"
echo "env: ${ENV_FILE:-current shell}"

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
  warn "BLUEY_DATABASE_URL is set. Current bluey-server is SQLite-backed; use this only with the Postgres backend cutover build."
  if command -v psql >/dev/null 2>&1; then
    if psql "$BLUEY_DATABASE_URL" -Atqc "select 1" >/dev/null 2>&1; then
      ok "Postgres connection succeeded"
      if psql "$BLUEY_DATABASE_URL" -Atqc "select 1 from pg_extension where extname = 'vector'" | grep -q 1; then
        ok "pgvector extension installed"
      else
        fail "pgvector extension missing"
      fi
    else
      fail "Postgres connection failed"
    fi
  else
    warn "psql not installed; skipped Postgres connectivity check"
  fi
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
  warn "BLUEY_REDIS_URL unset; safe only for single-server alpha"
fi
ok "BLUEY_REDIS_NAMESPACE=${BLUEY_REDIS_NAMESPACE:-bluey}"
if [ "${BLUEY_RATE_LIMIT_REDIS_STRICT:-0}" = "1" ]; then
  ok "Redis strict mode enabled"
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
