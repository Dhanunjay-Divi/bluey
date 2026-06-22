#!/usr/bin/env bash
set -euo pipefail

# Backfill Bluey's runtime SQLite database into the managed Postgres runtime
# schema. This is intended for the one-time production cutover while the
# SQLite service is stopped/frozen.
#
# Usage:
#   scripts/bluey-sqlite-to-postgres-backfill.sh /opt/bluey-api/bluey.db /etc/bluey-api/bluey-postgres.env
#
# Safety:
#   - refuses to import into a non-empty Postgres target unless
#     BLUEY_BACKFILL_ALLOW_NONEMPTY=1 is set.
#   - imports in a single Postgres transaction.
#   - creates temporary CSV files with mode 700 and removes them on exit.
#   - prints counts only; never prints row contents, tokens, or customer data.

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SQLITE_DB="${1:-${BLUEY_DB_PATH:-}}"
ENV_FILE="${2:-}"
ALLOW_NONEMPTY="${BLUEY_BACKFILL_ALLOW_NONEMPTY:-0}"

if [ -z "$SQLITE_DB" ]; then
  echo "fatal: SQLite DB path is required" >&2
  exit 2
fi

if [ ! -f "$SQLITE_DB" ]; then
  echo "fatal: SQLite DB not found: $SQLITE_DB" >&2
  exit 2
fi

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

if ! command -v sqlite3 >/dev/null 2>&1; then
  echo "fatal: sqlite3 is required" >&2
  exit 2
fi

if ! command -v psql >/dev/null 2>&1; then
  echo "fatal: psql is required" >&2
  exit 2
fi

TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/bluey-sqlite-backfill.XXXXXX")"
chmod 700 "$TMP_DIR"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

tables=(
  accounts
  credit_batches
  refresh_tokens
  device_codes
  usage_events
  stripe_webhook_events
  request_idempotency
  email_verification_tokens
  password_reset_tokens
  auth_link_codes
  cloud_sessions
  cloud_transcript_segments
  cloud_cue_responses
  cloud_context_artifacts
  cloud_rag_chunks
  signup_otps
  stt_sessions
)

columns_accounts="id,email,password_hash,email_verified_at,created_at,last_login_at,balance_cents,reserved_cents,trial_seconds_remaining,auto_topup_enabled,auto_topup_threshold_cents,auto_topup_amount_cents,stripe_customer_id,stripe_payment_method_id,square_customer_id,square_card_id,square_card_brand,square_card_last4,is_admin,billing_restricted,billing_restriction_reason,billing_restricted_at"
columns_credit_batches="id,account_id,amount_cents,remaining_cents,purchased_at,expires_at,stripe_charge_id,expired_at"
columns_refresh_tokens="token_hash,account_id,device_label,created_at,last_used_at,expires_at,revoked_at"
columns_device_codes="device_code,user_code,account_id,approved,created_at,expires_at"
columns_usage_events="id,account_id,request_id,ts,kind,task_type,lane,provider,model,input_tokens,output_tokens,latency_ms,cost_cents_to_bluey,cost_cents_to_customer,was_speculative,was_fallback"
columns_stripe_webhook_events="event_id,type,received_at,processed_at,body"
columns_request_idempotency="account_id,request_id,status,response_json,http_status,created_at,completed_at"
columns_email_verification_tokens="token_hash,account_id,created_at,expires_at,consumed_at"
columns_password_reset_tokens="token_hash,account_id,created_at,expires_at,consumed_at"
columns_auth_link_codes="code_hash,account_id,access_token,refresh_token,created_at,expires_at,consumed_at"
columns_cloud_sessions="account_id,session_id,title,status,created_at_ms,updated_at_ms,last_active_at_ms,answer_style,metadata_json,deleted_at_ms"
columns_cloud_transcript_segments="account_id,segment_id,session_id,speaker,source,text,start_ms,end_ms,ts_ms,is_final,metadata_json"
columns_cloud_cue_responses="account_id,response_id,session_id,kind,text,source_text,ts_ms,provider,model,lane,task_type,cost_cents,balance_cents_after,cost_label,artifact_type,artifact_body,artifact_confidence,metadata_json"
columns_cloud_context_artifacts="account_id,artifact_id,session_id,kind,title,note,source_uri,content_hash,text_preview,created_at_ms,metadata_json"
columns_cloud_rag_chunks="account_id,chunk_id,session_id,source_kind,source_id,chunk_index,text,embedding_json,embedding_model,token_count,content_hash,updated_at_ms,metadata_json"
columns_signup_otps="email,otp_hash,password_hash,attempts,created_at,expires_at"
columns_stt_sessions="session_token,account_id,bluey_session_id,provider,model,source,mode,max_seconds,created_at_ms,expires_at_ms,consumed_seconds,started_at_ms,ended_at_ms,relay_close_reason,reserved_cents,settled_cents,refunded_cents,reserved_trial_seconds,settled_trial_seconds,refunded_trial_seconds"

columns_for() {
  local table="$1"
  local var="columns_${table}"
  printf '%s' "${!var}"
}

table_exists_sqlite() {
  local table="$1"
  sqlite3 -readonly "$SQLITE_DB" "select 1 from sqlite_master where type = 'table' and name = '$table';" | grep -qx 1
}

count_sqlite() {
  local table="$1"
  if table_exists_sqlite "$table"; then
    sqlite3 -readonly "$SQLITE_DB" "select count(*) from $table;"
  else
    printf '0\n'
  fi
}

count_postgres() {
  local table="$1"
  psql "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc "select count(*) from public.$table;"
}

quote_csv_path() {
  local raw="$1"
  printf "%s" "$raw" | sed "s/'/''/g"
}

echo "Bluey SQLite -> Postgres backfill"
echo "sqlite: $SQLITE_DB"
echo "database: configured BLUEY_DATABASE_URL (redacted)"
echo "repo: $ROOT"

sqlite3 -readonly "$SQLITE_DB" "pragma quick_check;" | grep -qx ok || {
  echo "fatal: SQLite quick_check failed" >&2
  exit 1
}
echo "ok: SQLite quick_check passed"

psql "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc "select 1" >/dev/null
echo "ok: Postgres connection passed"

missing_pg=0
for table in "${tables[@]}"; do
  exists="$(psql "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -Atqc "select coalesce(to_regclass('public.$table')::text, '')")"
  if [ "$exists" != "$table" ] && [ "$exists" != "public.$table" ]; then
    echo "fatal: Postgres table missing: $table" >&2
    missing_pg=1
  fi
done
if [ "$missing_pg" -ne 0 ]; then
  exit 1
fi

pg_existing_total=0
for table in "${tables[@]}"; do
  pg_count="$(count_postgres "$table")"
  pg_existing_total=$((pg_existing_total + pg_count))
done

if [ "$pg_existing_total" -ne 0 ] && [ "$ALLOW_NONEMPTY" != "1" ]; then
  echo "fatal: Postgres target already has $pg_existing_total runtime row(s); refusing to backfill" >&2
  echo "hint: set BLUEY_BACKFILL_ALLOW_NONEMPTY=1 only after manual reconciliation" >&2
  exit 1
fi
echo "ok: Postgres runtime target is empty"

import_sql="$TMP_DIR/import.sql"
{
  echo "\\set ON_ERROR_STOP on"
  echo "BEGIN;"
} >"$import_sql"

expected_counts=()
for table in "${tables[@]}"; do
  cols="$(columns_for "$table")"
  csv="$TMP_DIR/$table.csv"
  if table_exists_sqlite "$table"; then
    sqlite3 -readonly -header -csv "$SQLITE_DB" "select $cols from $table;" >"$csv"
  else
    # Header-only CSV lets the import stay idempotent across empty legacy DBs.
    printf '%s\n' "$cols" >"$csv"
  fi
  count="$(count_sqlite "$table")"
  expected_counts+=("$table:$count")
  printf "\\copy public.%s (%s) FROM '%s' WITH (FORMAT csv, HEADER true, NULL '')\n" \
    "$table" "$cols" "$(quote_csv_path "$csv")" >>"$import_sql"
done

echo "COMMIT;" >>"$import_sql"

echo "Backfill source counts:"
for entry in "${expected_counts[@]}"; do
  echo "  ${entry/:/: }"
done

psql "$BLUEY_DATABASE_URL" -v ON_ERROR_STOP=1 -f "$import_sql" >/dev/null
echo "ok: imported runtime data into Postgres"

mismatches=0
echo "Postgres parity counts:"
for entry in "${expected_counts[@]}"; do
  table="${entry%%:*}"
  expected="${entry#*:}"
  actual="$(count_postgres "$table")"
  echo "  $table: $actual"
  if [ "$actual" != "$expected" ]; then
    echo "fatal: count mismatch for $table (sqlite=$expected postgres=$actual)" >&2
    mismatches=$((mismatches + 1))
  fi
done

if [ "$mismatches" -ne 0 ]; then
  exit 1
fi

echo "ok: SQLite -> Postgres backfill parity passed"
