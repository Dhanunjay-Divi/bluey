#!/usr/bin/env bash
set +x
set -euo pipefail

# Bluey production architecture preflight.
#
# Usage:
#   scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env
#   scripts/bluey-cloud-preflight.sh /etc/bluey-api/bluey-api.env /etc/bluey-api/bluey-postgres.env /etc/bluey-api/bluey-valkey.env
#
# The script never prints secret values. Missing optional CLIs are warnings
# unless BLUEY_PREFLIGHT_STRICT=1 is set.

ENV_FILES=("$@")
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
FAILURES=0
WARNINGS=0

bootstrap_stat() {
  local gnu="$1" bsd="$2" path="$3"
  if stat "$gnu" -- "$path" >/dev/null 2>&1; then stat "$gnu" -- "$path"; else stat "$bsd" -- "$path"; fi
}
bootstrap_follow_identity() {
  local path="$1"
  if stat -L -c%i -- "$path" >/dev/null 2>&1; then
    stat -L -c%i -- "$path"
  else
    stat -L -f%i -- "$path"
  fi
}
trusted_env_chain() {
  local path="$1" current owner mode value
  current="$(cd -P -- "$(dirname "$path")" 2>/dev/null && pwd -P)" || return 1
  while :; do
    [ -d "$current" ] && [ ! -L "$current" ] || return 1
    owner="$(bootstrap_stat -c%u -f%u "$current")" || return 1
    { [ "$owner" = 0 ] || [ "$owner" = "$EUID" ]; } || return 1
    mode="$(bootstrap_stat -c%a -f%Lp "$current")" || return 1
    case "$mode" in ''|*[!0-9]*) return 1 ;; esac
    value=$((8#$mode))
    if [ $((value & 8#022)) -ne 0 ] &&
       ! { [ "$owner" = 0 ] && [ $((value & 8#1000)) -ne 0 ]; }; then return 1; fi
    [ "$current" = / ] && break
    current="$(dirname "$current")"
  done
}
load_trusted_env() {
  local path="$1" owner mode value path_id fd_id env_fd
  [ -f "$path" ] && [ ! -L "$path" ] && trusted_env_chain "$path" || return 1
  owner="$(bootstrap_stat -c%u -f%u "$path")" || return 1
  { [ "$owner" = 0 ] || [ "$owner" = "$EUID" ]; } || return 1
  mode="$(bootstrap_stat -c%a -f%Lp "$path")" || return 1
  case "$mode" in ''|*[!0-9]*) return 1 ;; esac
  value=$((8#$mode)); [ $((value & 8#022)) -eq 0 ] || return 1
  path_id="$(bootstrap_stat -c%i -f%i "$path")" || return 1
  exec 9<"$path"
  env_fd=9
  fd_id="$(bootstrap_follow_identity "/dev/fd/$env_fd")" || return 1
  [ "$path_id" = "$fd_id" ] || return 1
  # shellcheck disable=SC1090
  . "/dev/fd/$env_fd"
  exec 9<&-
}

if [ "${#ENV_FILES[@]}" -gt 0 ]; then
  for env_file in "${ENV_FILES[@]}"; do
    if [ ! -f "$env_file" ]; then
      echo "fatal: env file not found: $env_file" >&2
      exit 2
    fi
    load_trusted_env "$env_file" || {
      echo "fatal: env file or its parent chain is not trusted: $env_file" >&2
      exit 2
    }
  done
fi
unset -f bootstrap_stat bootstrap_follow_identity trusted_env_chain load_trusted_env

primary_env_file="${ENV_FILES[0]:-}"

while IFS= read -r inherited_name; do
  case "$inherited_name" in
    *KEY*|*TOKEN*|*SECRET*|*PASSWORD*|*DATABASE_URL*|*DSN*|*WEBHOOK*|*CREDENTIAL*|*COOKIE*|*AUTH*)
      export -n "$inherited_name" 2>/dev/null || true
      ;;
  esac
done < <(compgen -e)
unset inherited_name

env_label() {
  if [ "${#ENV_FILES[@]}" -eq 0 ]; then
    printf 'current shell'
  else
    local joined=""
    local env_file
    for env_file in "${ENV_FILES[@]}"; do
      if [ -n "$joined" ]; then
        joined="${joined}, "
      fi
      joined="${joined}${env_file}"
    done
    printf '%s' "$joined"
  fi
}

STRICT="${BLUEY_PREFLIGHT_STRICT:-0}"
PROFILE="${BLUEY_PREFLIGHT_PROFILE:-single-server-alpha}"
REQUIRE_POSTGRES="${BLUEY_REQUIRE_POSTGRES:-0}"
REQUIRE_MANAGED_REDIS="${BLUEY_REQUIRE_MANAGED_REDIS:-0}"
REQUIRE_OBJECT_STORAGE="${BLUEY_REQUIRE_OBJECT_STORAGE:-0}"
SERVER_DB_BACKEND="${BLUEY_SERVER_DB_BACKEND:-sqlite}"
REQUIRE_DISK_GUARD="${BLUEY_PREFLIGHT_REQUIRE_DISK_GUARD:-0}"
DISK_GUARD_SCRIPT="${BLUEY_PREFLIGHT_DISK_GUARD_SCRIPT:-/usr/local/sbin/bluey-disk-guard.sh}"
REQUIRE_RESTORE_DEADMAN_PROVIDER="${BLUEY_PREFLIGHT_REQUIRE_RESTORE_DEADMAN_PROVIDER:-0}"
PREFLIGHT_TIMEOUT_SECONDS="${BLUEY_PREFLIGHT_COMMAND_TIMEOUT_SECONDS:-20}"
PREFLIGHT_KILL_GRACE_SECONDS="${BLUEY_PREFLIGHT_TIMEOUT_KILL_GRACE_SECONDS:-5}"
case "$PREFLIGHT_TIMEOUT_SECONDS" in ''|*[!0-9]*|0) echo "fatal: BLUEY_PREFLIGHT_COMMAND_TIMEOUT_SECONDS must be positive" >&2; exit 2 ;; esac
case "$PREFLIGHT_KILL_GRACE_SECONDS" in ''|*[!0-9]*|0) echo "fatal: BLUEY_PREFLIGHT_TIMEOUT_KILL_GRACE_SECONDS must be positive" >&2; exit 2 ;; esac

run_bounded() {
  local timeout_bin
  timeout_bin="$(command -v timeout 2>/dev/null || command -v gtimeout 2>/dev/null || true)"
  [ -n "$timeout_bin" ] || return 124
  "$timeout_bin" --signal=TERM --kill-after="$PREFLIGHT_KILL_GRACE_SECONDS" "$PREFLIGHT_TIMEOUT_SECONDS" "$@"
}

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

normalize_route_policy() {
  local raw="$1"
  raw="$(printf '%s' "$raw" | tr '[:upper:]' '[:lower:]' | sed 's/[- ]/_/g')"
  case "$raw" in
    cost|cost_first|cost_optimized|cheap|glm|deepseek)
      printf 'cost_optimized'
      ;;
    quality|quality_first|static|legacy)
      printf 'quality_first'
      ;;
    mix|mixed|provider_mix|balanced_mix|anti_429|capacity_mix|"")
      printf 'provider_mix'
      ;;
    *)
      printf 'unknown'
      ;;
  esac
}

truthy_env() {
  case "$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')" in
    1|true|yes|on)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

falsey_env() {
  case "$(printf '%s' "$1" | tr '[:upper:]' '[:lower:]')" in
    0|false|no|off)
      return 0
      ;;
    *)
      return 1
      ;;
  esac
}

is_uint() {
  case "$1" in
    ''|*[!0-9]*)
      return 1
      ;;
    *)
      [ "$1" -gt 0 ]
      ;;
  esac
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
echo "env: $(env_label)"
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
    REQUIRE_OBJECT_STORAGE=1
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

turnstile_site="${BLUEY_TURNSTILE_SITE_KEY:-${TURNSTILE_SITE_KEY:-}}"
turnstile_secret="${BLUEY_TURNSTILE_SECRET_KEY:-${TURNSTILE_SECRET_KEY:-}}"
require_turnstile="${BLUEY_REQUIRE_TURNSTILE:-0}"
if [ "${SQUARE_ENVIRONMENT:-sandbox}" = "production" ] || [ "$require_turnstile" = "1" ]; then
  require_turnstile=1
fi
if [ -n "$turnstile_site" ] || [ -n "$turnstile_secret" ]; then
  if [ -n "$turnstile_site" ] && ! is_placeholder "$turnstile_site"; then
    ok "Turnstile site key set"
  else
    fail "Turnstile site key missing or placeholder while captcha is partly configured"
  fi
  if [ -n "$turnstile_secret" ] && ! is_placeholder "$turnstile_secret"; then
    ok "Turnstile secret set"
  else
    fail "Turnstile secret missing or placeholder while captcha is partly configured"
  fi
elif [ "$require_turnstile" = "1" ]; then
  fail "Turnstile keys required for production signup abuse protection"
else
  warn "Turnstile keys unset; signup CAPTCHA is disabled"
fi

if [ "$SERVER_DB_BACKEND" = "postgres" ]; then
  ok "SQLite path check skipped in Postgres backend mode"
elif [ -n "${BLUEY_DB_PATH:-}" ]; then
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
    warn "BLUEY_DATABASE_URL is set but BLUEY_SERVER_DB_BACKEND=${SERVER_DB_BACKEND}; bluey-server will stay on SQLite unless postgres backend mode is explicitly enabled"
  fi
  if command -v psql >/dev/null 2>&1; then
    if PGDATABASE="$BLUEY_DATABASE_URL" run_bounded psql -Atqc "select 1" >/dev/null 2>&1; then
      ok "Postgres connection succeeded"
      if PGDATABASE="$BLUEY_DATABASE_URL" run_bounded psql -Atqc \
        "select 1 from pg_extension where extname = 'vector'" | grep -q 1; then
        ok "pgvector extension installed"
      else
        fail "pgvector extension missing"
      fi
      rag_table="$(PGDATABASE="$BLUEY_DATABASE_URL" run_bounded psql -Atqc "select coalesce(to_regclass('public.cloud_rag_chunks')::text, to_regclass('public.memory_chunks')::text, '')")"
      if printf '%s' "$rag_table" | grep -Eq 'cloud_rag_chunks|memory_chunks'; then
        ok "cloud RAG table present ($rag_table)"
        embedding_type="$(PGDATABASE="$BLUEY_DATABASE_URL" run_bounded psql -Atqc "select udt_name from information_schema.columns where table_schema = 'public' and table_name = 'cloud_rag_chunks' and column_name = 'embedding' union all select udt_name from information_schema.columns where table_schema = 'public' and table_name = 'memory_chunks' and column_name = 'embedding' limit 1")"
        if [ "$embedding_type" = "vector" ]; then
          ok "cloud RAG embedding column uses pgvector"
        else
          fail "cloud RAG embedding column is not pgvector (found: ${embedding_type:-missing})"
        fi
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
    need_env SQUARE_PRODUCTION_APPLICATION_ID "Square production application"
    need_env SQUARE_PRODUCTION_ACCESS_TOKEN "Square production API"
    need_env SQUARE_PRODUCTION_LOCATION_ID "Square production location"
    need_env SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY "Square production webhook verification"
  else
    need_env SQUARE_SANDBOX_APPLICATION_ID "Square sandbox application"
    need_env SQUARE_SANDBOX_ACCESS_TOKEN "Square sandbox API"
    need_env SQUARE_SANDBOX_LOCATION_ID "Square sandbox location"
    need_env SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY "Square sandbox webhook verification"
  fi
  if [ -x "$ROOT/scripts/bluey-square-branding.sh" ]; then
    brand_cmd=("$ROOT/scripts/bluey-square-branding.sh")
    if [ -n "$primary_env_file" ]; then
      brand_cmd+=("$primary_env_file")
    fi
    brand_cmd+=("--check")
    if brand_output="$("${brand_cmd[@]}" 2>&1)"; then
      ok "$brand_output"
    else
      fail "Square checkout branding is not Bluey: $brand_output"
    fi
  else
    warn "Square checkout branding check unavailable; missing scripts/bluey-square-branding.sh"
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

if [ -n "${BLUEY_ANSWER_PLAN_ROUTING:-}" ] && falsey_env "$BLUEY_ANSWER_PLAN_ROUTING"; then
  warn "BLUEY_ANSWER_PLAN_ROUTING is disabled; managed Auto will not promote coding/research/behavioral lanes before provider routing"
elif [ -n "${BLUEY_ANSWER_PLAN_ROUTING:-}" ] && ! truthy_env "$BLUEY_ANSWER_PLAN_ROUTING"; then
  warn "BLUEY_ANSWER_PLAN_ROUTING has an unrecognized value; server treats only 0/false/no/off as disabled"
else
  ok "AnswerPlan routing enabled (default-on; set BLUEY_ANSWER_PLAN_ROUTING=0 only for rollback)"
fi

if [ -n "${BLUEY_ANSWER_PLAN_AI_FALLBACK:-}" ] && falsey_env "$BLUEY_ANSWER_PLAN_AI_FALLBACK"; then
  ok "AnswerPlan AI fallback disabled; deterministic local rules handle ambiguous requests without an extra provider call"
elif [ -n "${BLUEY_ANSWER_PLAN_AI_FALLBACK:-}" ] && ! truthy_env "$BLUEY_ANSWER_PLAN_AI_FALLBACK"; then
  warn "BLUEY_ANSWER_PLAN_AI_FALLBACK has an unrecognized value; server treats only 0/false/no/off as disabled"
elif [ -n "${BLUEY_ANSWER_PLAN_AI_FALLBACK:-}" ]; then
  warn "AnswerPlan AI fallback enabled; ambiguous requests add a metered provider call before the answer"
else
  ok "AnswerPlan AI fallback disabled by default; deterministic local rules are active"
fi

route_policy="$(normalize_route_policy "${BLUEY_ROUTE_POLICY:-${BLUEY_ROUTE_ORDER:-}}")"
case "$route_policy" in
  provider_mix)
    ok "BLUEY_ROUTE_POLICY=${BLUEY_ROUTE_POLICY:-provider_mix} (default anti-429 provider mix)"
    ;;
  quality_first)
    ok "BLUEY_ROUTE_POLICY=quality_first"
    ;;
  cost_optimized)
    ok "BLUEY_ROUTE_POLICY=cost_optimized"
    zai_ready=0
    deepseek_ready=0
    if { [ -n "${ZAI_API_KEYS:-}" ] && ! is_placeholder "${ZAI_API_KEYS:-}"; } || { [ -n "${ZAI_API_KEY:-}" ] && ! is_placeholder "${ZAI_API_KEY:-}"; }; then
      zai_ready=1
      ok "ZAI key pool set for cost-optimized GLM routes"
    else
      warn "cost_optimized selected but ZAI_API_KEYS/ZAI_API_KEY is missing; GLM routes will be skipped"
    fi
    if { [ -n "${DEEPSEEK_API_KEYS:-}" ] && ! is_placeholder "${DEEPSEEK_API_KEYS:-}"; } || { [ -n "${DEEPSEEK_API_KEY:-}" ] && ! is_placeholder "${DEEPSEEK_API_KEY:-}"; }; then
      deepseek_ready=1
      ok "DeepSeek key pool set for cost-optimized routes"
    else
      warn "cost_optimized selected but DEEPSEEK_API_KEYS/DEEPSEEK_API_KEY is missing; DeepSeek routes will be skipped"
    fi
    if [ "$zai_ready" = "0" ] && [ "$deepseek_ready" = "0" ]; then
      fail "BLUEY_ROUTE_POLICY=cost_optimized requires at least one configured ZAI or DeepSeek key pool"
    fi
    ;;
  *)
    fail "BLUEY_ROUTE_POLICY/BLUEY_ROUTE_ORDER has an unknown value: ${BLUEY_ROUTE_POLICY:-${BLUEY_ROUTE_ORDER:-unset}}"
    ;;
esac

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
    if run_bounded redis-cli -u "$BLUEY_REDIS_URL" PING 2>/dev/null | grep -q PONG; then
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

object_endpoint="${BLUEY_OBJECT_ENDPOINT_URL:-${BLUEY_R2_ENDPOINT_URL:-}}"
object_bucket="${BLUEY_OBJECT_BUCKET:-${BLUEY_R2_BUCKET:-}}"
object_access_key="${BLUEY_OBJECT_ACCESS_KEY_ID:-${BLUEY_R2_ACCESS_KEY_ID:-}}"
object_secret_key="${BLUEY_OBJECT_SECRET_ACCESS_KEY:-${BLUEY_R2_SECRET_ACCESS_KEY:-}}"
object_region="${BLUEY_OBJECT_REGION:-${BLUEY_R2_REGION:-auto}}"

object_missing=0
if [ -n "$object_endpoint" ] && ! is_placeholder "$object_endpoint"; then
  ok "object storage endpoint set"
else
  object_missing=1
fi
if [ -n "$object_bucket" ] && ! is_placeholder "$object_bucket"; then
  ok "object storage bucket set"
else
  object_missing=1
fi
if [ -n "$object_access_key" ] && ! is_placeholder "$object_access_key"; then
  ok "object storage access key set"
else
  object_missing=1
fi
if [ -n "$object_secret_key" ] && ! is_placeholder "$object_secret_key"; then
  ok "object storage secret key set"
else
  object_missing=1
fi
if [ "$object_missing" = "1" ]; then
  if [ "$REQUIRE_OBJECT_STORAGE" = "1" ]; then
    fail "R2/S3 object storage env is incomplete; original document/image restore requires it"
  else
    warn "R2/S3 object storage env incomplete; cloud restore will fall back to text previews"
  fi
elif command -v aws >/dev/null 2>&1; then
  if AWS_ACCESS_KEY_ID="$object_access_key" \
     AWS_SECRET_ACCESS_KEY="$object_secret_key" \
     AWS_REGION="$object_region" \
     run_bounded aws --endpoint-url "$object_endpoint" s3api head-bucket --bucket "$object_bucket" >/dev/null 2>&1; then
    ok "object storage bucket reachable"
  else
    warn "object storage bucket not reachable from this machine; verify endpoint, bucket, and key policy"
  fi
else
  warn "aws CLI not installed; skipped object storage bucket reachability check"
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
        if AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-}" \
           AWS_SECRET_ACCESS_KEY="${AWS_SECRET_ACCESS_KEY:-}" \
           AWS_SESSION_TOKEN="${AWS_SESSION_TOKEN:-}" \
           AWS_DEFAULT_REGION="${AWS_DEFAULT_REGION:-${AWS_REGION:-auto}}" \
           run_bounded aws "${aws_args[@]}" s3 ls "$OFFSITE_DESTINATION" >/dev/null 2>&1; then
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

require_log_archive="${BLUEY_REQUIRE_LOG_ARCHIVE:-0}"
log_destination="${BLUEY_OPS_LOG_ARCHIVE_DESTINATION:-}"
log_bucket="${BLUEY_OPS_LOG_R2_BUCKET:-}"
log_endpoint="${BLUEY_OPS_LOG_R2_ENDPOINT_URL:-}"
log_access_key="${BLUEY_OPS_LOG_R2_ACCESS_KEY_ID:-}"
log_secret_key="${BLUEY_OPS_LOG_R2_SECRET_ACCESS_KEY:-}"
log_region="${BLUEY_OPS_LOG_R2_REGION:-auto}"
log_storage="${BLUEY_LOG_STORAGE:-}"
log_retention_days="${BLUEY_UPLOAD_LOG_RETENTION_DAYS:-${BLUEY_LOG_RETENTION_DAYS:-180}}"

if [ -n "$log_destination" ] && ! is_placeholder "$log_destination"; then
  ok "log archive destination set"
elif [ -n "$log_bucket" ] && ! is_placeholder "$log_bucket"; then
  ok "log archive bucket set; destination will use ${BLUEY_LOG_STORAGE_PREFIX:-prod}/logs/api"
else
  if [ "$require_log_archive" = "1" ]; then
    fail "log archive destination missing; set BLUEY_OPS_LOG_ARCHIVE_DESTINATION or BLUEY_OPS_LOG_R2_BUCKET"
  else
    warn "log archive destination missing; production logs will remain local only"
  fi
fi
if [ -n "$log_endpoint" ] && ! is_placeholder "$log_endpoint"; then
  ok "log archive R2/S3 endpoint set"
elif [ "$require_log_archive" = "1" ]; then
  fail "log archive endpoint missing"
else
  warn "log archive endpoint missing"
fi
if [ -n "$log_access_key" ] && ! is_placeholder "$log_access_key"; then
  ok "log archive access key set"
elif [ "$require_log_archive" = "1" ]; then
  fail "log archive access key missing"
else
  warn "log archive access key missing"
fi
if [ -n "$log_secret_key" ] && ! is_placeholder "$log_secret_key"; then
  ok "log archive secret key set"
elif [ "$require_log_archive" = "1" ]; then
  fail "log archive secret key missing"
else
  warn "log archive secret key missing"
fi
if [ -n "$log_destination" ] && [[ "$log_destination" == s3://* ]] && command -v aws >/dev/null 2>&1; then
  aws_log_args=()
  if [ -n "$log_endpoint" ]; then
    aws_log_args+=(--endpoint-url "$log_endpoint")
  fi
  if AWS_ACCESS_KEY_ID="$log_access_key" \
     AWS_SECRET_ACCESS_KEY="$log_secret_key" \
     AWS_DEFAULT_REGION="$log_region" \
     run_bounded aws "${aws_log_args[@]}" s3 ls "$log_destination" >/dev/null 2>&1; then
    ok "R2/S3 log archive destination reachable"
  else
    warn "R2/S3 log archive destination not listable; verify bucket policy and prefix"
  fi
elif [ -n "$log_bucket" ] && command -v aws >/dev/null 2>&1; then
  aws_log_args=()
  if [ -n "$log_endpoint" ]; then
    aws_log_args+=(--endpoint-url "$log_endpoint")
  fi
  if AWS_ACCESS_KEY_ID="$log_access_key" \
     AWS_SECRET_ACCESS_KEY="$log_secret_key" \
     AWS_DEFAULT_REGION="$log_region" \
     run_bounded aws "${aws_log_args[@]}" s3api head-bucket --bucket "$log_bucket" >/dev/null 2>&1; then
    ok "R2/S3 log archive bucket reachable"
  else
    warn "R2/S3 log archive bucket not reachable; verify endpoint, bucket, and key policy"
  fi
elif [ "$require_log_archive" = "1" ]; then
  warn "aws CLI not installed; skipped required log archive reachability check"
fi
if [ "$log_storage" = "r2" ] || [ "$log_storage" = "s3" ]; then
  ok "diagnostic log storage mode=$log_storage"
elif [ "$require_log_archive" = "1" ]; then
  warn "BLUEY_LOG_STORAGE is not r2/s3; server-side account diagnostic chunks may not have durable log storage"
else
  warn "diagnostic log storage mode not set"
fi
if is_uint "$log_retention_days" && [ "$log_retention_days" -le 180 ]; then
  ok "diagnostic log retention days=$log_retention_days"
else
  fail "diagnostic log retention must be a positive integer <= 180 days"
fi
if [ -n "${BLUEY_DATABASE_URL:-}" ] && command -v psql >/dev/null 2>&1; then
  ok "diagnostic log archive indexing can use Postgres via psql"
elif [ -n "${BLUEY_DATABASE_URL:-}" ]; then
  warn "psql missing; log archive upload will work but diagnostic_log_chunks indexing will be skipped"
fi
ok "log hot cache retention days=${BLUEY_LOG_LOCAL_RETENTION_DAYS:-7}"
ok "log dir max bytes=${BLUEY_LOG_DIR_MAX_BYTES:-536870912}"
ok "log archive root max bytes=${BLUEY_LOG_ROOT_MAX_BYTES:-2147483648}"

if [ -n "${BLUEY_PUBLIC_URL:-}" ] && command -v curl >/dev/null 2>&1; then
  if run_bounded curl -fsS "${BLUEY_PUBLIC_URL%/}/health" >/dev/null 2>&1; then
    ok "health endpoint reachable"
  else
    warn "health endpoint not reachable from this machine"
  fi
  if run_bounded curl -fsS "${BLUEY_PUBLIC_URL%/}/latest.json" >/dev/null 2>&1 &&
     run_bounded curl -fsS "${BLUEY_PUBLIC_URL%/}/latest.json.sig" >/dev/null 2>&1; then
    ok "signed update manifest files reachable"
  else
    warn "signed update manifest files not both reachable"
  fi
fi

case "$REQUIRE_DISK_GUARD" in
  0) ;;
  1)
    if [ ! -x "$DISK_GUARD_SCRIPT" ]; then
      fail "required disk guard is not executable: $DISK_GUARD_SCRIPT"
    elif run_bounded "$DISK_GUARD_SCRIPT" check >/dev/null; then
      ok "durable production disk/backup guard passed"
    else
      fail "durable production disk/backup guard failed"
    fi
    ;;
  *) fail "BLUEY_PREFLIGHT_REQUIRE_DISK_GUARD must be 0 or 1" ;;
esac

case "$REQUIRE_RESTORE_DEADMAN_PROVIDER" in
  0) ;;
  1)
    # Phase 622 deliberately has no generic/self-issued substitute for an
    # independent control-plane monitor that can remove an abandoned restore
    # target after this host is killed or lost. Keep the production profile
    # fail-closed until a separately reviewed provider integration replaces
    # this explicit stop line with live registration/status evidence.
    fail "external restore-drill dead-man provider is not implemented; production release remains blocked"
    ;;
  *) fail "BLUEY_PREFLIGHT_REQUIRE_RESTORE_DEADMAN_PROVIDER must be 0 or 1" ;;
esac

if [ "$STRICT" = "1" ] && [ "$WARNINGS" -gt 0 ]; then
  fail "strict mode treats warnings as failures ($WARNINGS warning(s))"
fi

if [ "$FAILURES" -gt 0 ]; then
  echo "preflight failed: $FAILURES failure(s), $WARNINGS warning(s)" >&2
  exit 1
fi

echo "preflight passed: $WARNINGS warning(s)"
