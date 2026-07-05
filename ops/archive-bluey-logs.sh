#!/usr/bin/env bash
# Archive Bluey production operational logs off-host and keep only a short
# local hot cache on the API droplet.
#
# Install:
#   cp ops/archive-bluey-logs.sh /usr/local/sbin/archive-bluey-logs.sh
#   chmod 750 /usr/local/sbin/archive-bluey-logs.sh
#
# Cron:
#   17 * * * * root /usr/local/sbin/archive-bluey-logs.sh
#
# This intentionally archives server operational diagnostics only. Desktop
# logs/support bundles remain redacted, user-initiated exports.

set -euo pipefail
umask 077

BLUEY_ENV_FILE="${BLUEY_ENV_FILE:-/etc/bluey-api/bluey-api.env}"

load_env_file() {
    local env_file="$1"
    if [ -r "$env_file" ]; then
        set -a
        # shellcheck disable=SC1090
        . "$env_file"
        set +a
    fi
}

load_env_file "$BLUEY_ENV_FILE"
if [ -n "${BLUEY_EXTRA_ENV_FILES:-}" ]; then
    # shellcheck disable=SC2086
    for env_file in $BLUEY_EXTRA_ENV_FILES; do
        load_env_file "$env_file"
    done
else
    load_env_file /etc/bluey-api/bluey-postgres.env
fi

ARCHIVE_ROOT="${BLUEY_LOG_ARCHIVE_LOCAL_DIR:-/var/backups/bluey-api/logs}"
WORK_ROOT="${BLUEY_LOG_ARCHIVE_WORK_DIR:-/var/tmp/bluey-log-archive}"
LOG_DIRS="${BLUEY_LOG_DIRS:-/var/log/bluey-api /opt/bluey-api/logs}"
SERVICES="${BLUEY_LOG_ARCHIVE_SERVICES:-bluey-api caddy}"
SINCE="${BLUEY_LOG_ARCHIVE_SINCE:-24 hours ago}"
LOCAL_RETENTION_DAYS="${BLUEY_LOG_LOCAL_RETENTION_DAYS:-7}"
ARCHIVE_LOCAL_RETENTION_DAYS="${BLUEY_LOG_ARCHIVE_LOCAL_RETENTION_DAYS:-7}"
LOG_DIR_MAX_BYTES="${BLUEY_LOG_DIR_MAX_BYTES:-536870912}"
LOG_ROOT_MAX_BYTES="${BLUEY_LOG_ROOT_MAX_BYTES:-2147483648}"
ARCHIVE_MAX_FILES="${BLUEY_LOG_ARCHIVE_MAX_FILES:-160}"
REQUIRE_OFFHOST="${BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST:-0}"
STORAGE_PREFIX="${BLUEY_LOG_STORAGE_PREFIX:-prod}"
DIAGNOSTIC_RETENTION_DAYS="${BLUEY_UPLOAD_LOG_RETENTION_DAYS:-${BLUEY_LOG_RETENTION_DAYS:-180}}"
UPLOADED_STORAGE=""
UPLOADED_OBJECT_KEY=""
UPLOADED_LOCAL_PATH=""
PRUNE_ONLY=0

case "${1:-}" in
    --prune-only)
        PRUNE_ONLY=1
        ;;
    "" )
        ;;
    * )
        echo "usage: $0 [--prune-only]" >&2
        exit 2
        ;;
esac

is_uint() {
    case "$1" in
        ""|*[!0-9]*)
            return 1
            ;;
        *)
            return 0
            ;;
    esac
}

dir_bytes() {
    local path="$1"
    if [ ! -e "$path" ]; then
        echo 0
        return
    fi
    if du -sb "$path" >/dev/null 2>&1; then
        du -sb "$path" | awk '{print $1}'
    else
        du -sk "$path" | awk '{print $1 * 1024}'
    fi
}

supports_find_printf() {
    find "${1:-.}" -maxdepth 0 -printf '' >/dev/null 2>&1
}

list_files_newest() {
    local dir="$1"
    shift
    if supports_find_printf "$dir"; then
        find "$dir" -type f "$@" -printf '%T@ %p\n' 2>/dev/null | sort -rn | cut -d' ' -f2-
    else
        find "$dir" -type f "$@" -exec stat -f '%m %N' {} \; 2>/dev/null | sort -rn | cut -d' ' -f2-
    fi
}

list_files_oldest() {
    local dir="$1"
    shift
    if supports_find_printf "$dir"; then
        find "$dir" -type f "$@" -printf '%T@ %p\n' 2>/dev/null | sort -n | cut -d' ' -f2-
    else
        find "$dir" -type f "$@" -exec stat -f '%m %N' {} \; 2>/dev/null | sort -n | cut -d' ' -f2-
    fi
}

redact_stream() {
    if command -v perl >/dev/null 2>&1; then
        perl -pe '
          s/(Authorization:\s*Bearer\s+)[A-Za-z0-9._~+\/=-]+/${1}<redacted>/ig;
          s/(api[_-]?key["=: ]+)[A-Za-z0-9._~+\/=-]+/${1}<redacted>/ig;
          s/(secret["=: ]+)[A-Za-z0-9._~+\/=-]+/${1}<redacted>/ig;
          s/sk-[A-Za-z0-9_-]{16,}/sk-<redacted>/g;
          s/(?:bluey|cue):\/\/[^\s"<>]+/bluey:\/\/<redacted>/g;
          s/[A-Za-z0-9._%+\-]+@[A-Za-z0-9.\-]+\.[A-Za-z]{2,}/<email>/g;
          s#/(Users|home)/[^/\s"<>]+/#/$1/<redacted>/#g;
        '
    else
        sed -E \
            -e 's/(Authorization:[[:space:]]*Bearer[[:space:]]+)[A-Za-z0-9._~+\/=-]+/\1<redacted>/Ig' \
            -e 's/sk-[A-Za-z0-9_-]{16,}/sk-<redacted>/g' \
            -e 's/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/<email>/g'
    fi
}

prune_archives() {
    mkdir -p "$ARCHIVE_ROOT"
    find "$ARCHIVE_ROOT" -type f \
        \( -name 'bluey-logs-*.tar.gz' -o -name 'bluey-logs-*.tar.gz.sha256' \) \
        -mtime +"$ARCHIVE_LOCAL_RETENTION_DAYS" -print -delete 2>/dev/null || true

    if is_uint "$LOG_ROOT_MAX_BYTES"; then
        local total
        total="$(dir_bytes "$ARCHIVE_ROOT")"
        while [ "$total" -gt "$LOG_ROOT_MAX_BYTES" ]; do
            local oldest
            oldest="$(list_files_oldest "$ARCHIVE_ROOT" -name 'bluey-logs-*' | head -1)"
            if [ -z "$oldest" ]; then
                break
            fi
            rm -f "$oldest"
            rm -f "${oldest}.sha256"
            total="$(dir_bytes "$ARCHIVE_ROOT")"
        done
    fi
}

prune_hot_logs() {
    local log_dir
    # Keep active .log files. Prune only old rotated/compressed files by age,
    # then by size if the directory still exceeds its cap.
    # shellcheck disable=SC2086
    for log_dir in $LOG_DIRS; do
        [ -d "$log_dir" ] || continue
        find "$log_dir" -type f \
            \( -name '*.gz' -o -name '*.log-*' -o -name '*.log.*' \) \
            -mtime +"$LOCAL_RETENTION_DAYS" -print -delete 2>/dev/null || true

        if is_uint "$LOG_DIR_MAX_BYTES"; then
            local total
            total="$(dir_bytes "$log_dir")"
            while [ "$total" -gt "$LOG_DIR_MAX_BYTES" ]; do
                local oldest
                oldest="$(list_files_oldest "$log_dir" \
                    \( -name '*.gz' -o -name '*.log-*' -o -name '*.log.*' \) | head -1)"
                if [ -z "$oldest" ]; then
                    echo "warn: $log_dir is above cap but only active logs remain" >&2
                    break
                fi
                rm -f "$oldest"
                total="$(dir_bytes "$log_dir")"
            done
        fi
    done
}

destination_base() {
    if [ -n "${BLUEY_LOG_ARCHIVE_DESTINATION:-}" ]; then
        printf '%s' "${BLUEY_LOG_ARCHIVE_DESTINATION%/}"
        return
    fi
    local bucket="${BLUEY_LOG_R2_BUCKET:-${BLUEY_OBJECT_BUCKET:-${BLUEY_R2_BUCKET:-}}}"
    if [ -n "$bucket" ]; then
        printf 's3://%s/%s/logs/api' "$bucket" "${STORAGE_PREFIX#/}"
    fi
}

aws_endpoint() {
    local endpoint="${BLUEY_LOG_R2_ENDPOINT_URL:-${BLUEY_LOG_R2_ENDPOINT:-${BLUEY_BACKUP_S3_ENDPOINT_URL:-${BLUEY_OBJECT_ENDPOINT_URL:-${BLUEY_R2_ENDPOINT_URL:-}}}}}"
    if [ -n "$endpoint" ]; then
        printf '%s' "$endpoint"
    fi
}

upload_archive() {
    local archive="$1"
    UPLOADED_STORAGE="local"
    UPLOADED_OBJECT_KEY=""
    UPLOADED_LOCAL_PATH="$archive"
    local destination
    destination="$(destination_base)"
    if [ -z "$destination" ]; then
        if [ "$REQUIRE_OFFHOST" = "1" ]; then
            echo "BLUEY_LOG_ARCHIVE_DESTINATION or BLUEY_LOG_R2_BUCKET is required" >&2
            exit 1
        fi
        echo "warn: log archive destination not configured; kept local archive only: $archive" >&2
        return
    fi
    case "$destination" in
        s3://*)
            if ! command -v aws >/dev/null 2>&1; then
                echo "aws CLI is required for $destination" >&2
                exit 1
            fi
            if [ -n "${BLUEY_LOG_R2_ACCESS_KEY_ID:-}" ]; then
                export AWS_ACCESS_KEY_ID="$BLUEY_LOG_R2_ACCESS_KEY_ID"
            fi
            if [ -n "${BLUEY_LOG_R2_SECRET_ACCESS_KEY:-}" ]; then
                export AWS_SECRET_ACCESS_KEY="$BLUEY_LOG_R2_SECRET_ACCESS_KEY"
            fi
            export AWS_DEFAULT_REGION="${BLUEY_LOG_R2_REGION:-${AWS_DEFAULT_REGION:-auto}}"
            local endpoint
            local -a extra_args
            extra_args=()
            endpoint="$(aws_endpoint)"
            if [ -n "$endpoint" ]; then
                extra_args+=(--endpoint-url "$endpoint")
            fi
            local date_prefix
            date_prefix="$(date -u +%Y/%m/%d)"
            local destination_path destination_prefix object_key
            destination_path="${destination#s3://}"
            destination_prefix=""
            if [ "$destination_path" != "${destination_path%%/*}" ]; then
                destination_prefix="${destination_path#*/}"
            fi
            object_key="${destination_prefix%/}/$date_prefix/$(basename "$archive")"
            object_key="${object_key#/}"
            aws "${extra_args[@]}" s3 cp "$archive" "$destination/$date_prefix/$(basename "$archive")" --quiet
            aws "${extra_args[@]}" s3 cp "${archive}.sha256" "$destination/$date_prefix/$(basename "$archive").sha256" --quiet
            UPLOADED_STORAGE="r2"
            UPLOADED_OBJECT_KEY="$object_key"
            UPLOADED_LOCAL_PATH=""
            ;;
        *)
            if command -v rsync >/dev/null 2>&1; then
                rsync -a --quiet "$archive" "${archive}.sha256" "$destination/"
                UPLOADED_STORAGE="filesystem"
                UPLOADED_OBJECT_KEY=""
                UPLOADED_LOCAL_PATH="$destination/$(basename "$archive")"
            else
                echo "non-s3 log archive destinations require rsync" >&2
                exit 1
            fi
            ;;
    esac
}

index_archive_metadata() {
    local archive="$1"
    if [ -z "${BLUEY_DATABASE_URL:-}" ]; then
        return
    fi
    if ! command -v psql >/dev/null 2>&1; then
        echo "warn: psql not installed; skipped diagnostic_log_chunks index insert" >&2
        return
    fi
    local retention_days
    retention_days="$DIAGNOSTIC_RETENTION_DAYS"
    if ! is_uint "$retention_days"; then
        retention_days=180
    fi
    if [ "$retention_days" -gt 180 ]; then
        retention_days=180
    fi
    if [ "$retention_days" -lt 1 ]; then
        retention_days=1
    fi

    local now_s created_ms expires_ms bytes sha id archive_name host
    now_s="$(date -u +%s)"
    created_ms=$((now_s * 1000))
    expires_ms=$(((now_s + retention_days * 86400) * 1000))
    bytes="$(wc -c < "$archive" | tr -d ' ')"
    sha="$(awk '{print $1}' "${archive}.sha256" 2>/dev/null || true)"
    archive_name="$(basename "$archive")"
    host="$(hostname -s 2>/dev/null || hostname || echo unknown-host)"
    if command -v uuidgen >/dev/null 2>&1; then
        id="$(uuidgen | tr '[:upper:]' '[:lower:]')"
    else
        id="diag-${now_s}-$$"
    fi

    if ! psql "$BLUEY_DATABASE_URL" \
        -v ON_ERROR_STOP=1 \
        -v id="$id" \
        -v storage="$UPLOADED_STORAGE" \
        -v object_key="$UPLOADED_OBJECT_KEY" \
        -v local_path="$UPLOADED_LOCAL_PATH" \
        -v bytes="$bytes" \
        -v sha256="$sha" \
        -v created_ms="$created_ms" \
        -v expires_ms="$expires_ms" \
        -v host="$host" \
        -v services="$SERVICES" \
        -v since="$SINCE" \
        -v archive_name="$archive_name" \
        >/dev/null <<'SQL'
CREATE TABLE IF NOT EXISTS diagnostic_log_chunks (
  id TEXT PRIMARY KEY,
  account_id TEXT REFERENCES accounts(id) ON DELETE CASCADE,
  workspace_id TEXT,
  session_id TEXT,
  session_code TEXT,
  kind TEXT NOT NULL,
  storage TEXT NOT NULL DEFAULT 'local',
  object_key TEXT,
  local_path TEXT,
  bytes BIGINT NOT NULL DEFAULT 0,
  sha256 TEXT,
  created_at_ms BIGINT NOT NULL,
  expires_at_ms BIGINT NOT NULL,
  metadata_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_diagnostic_log_chunks_account_created
  ON diagnostic_log_chunks(account_id, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_diagnostic_log_chunks_session
  ON diagnostic_log_chunks(account_id, session_id, created_at_ms DESC);
CREATE INDEX IF NOT EXISTS idx_diagnostic_log_chunks_expires
  ON diagnostic_log_chunks(expires_at_ms);
CREATE INDEX IF NOT EXISTS idx_diagnostic_log_chunks_kind_created
  ON diagnostic_log_chunks(kind, created_at_ms DESC);

INSERT INTO diagnostic_log_chunks
  (id, account_id, workspace_id, session_id, session_code, kind, storage,
   object_key, local_path, bytes, sha256, created_at_ms, expires_at_ms, metadata_json)
VALUES
  (:'id', NULL, NULL, NULL, NULL, 'server_operational_bundle', :'storage',
   NULLIF(:'object_key', ''), NULLIF(:'local_path', ''), :'bytes'::bigint,
   NULLIF(:'sha256', ''), :'created_ms'::bigint, :'expires_ms'::bigint,
   jsonb_build_object(
     'host', :'host',
     'services', :'services',
     'since', :'since',
     'archive_name', :'archive_name',
     'redacted', true,
     'contains_user_content', false,
     'backup_thread_id', '019e133e-d92a-7830-8df0-3a050a4e22f6'
   )::text)
ON CONFLICT (id) DO NOTHING;

DELETE FROM diagnostic_log_chunks
 WHERE account_id IS NULL
   AND expires_at_ms < :'created_ms'::bigint;
SQL
    then
        echo "warn: failed to index log archive metadata in diagnostic_log_chunks" >&2
    fi
}

archive_logs() {
    mkdir -p "$ARCHIVE_ROOT" "$WORK_ROOT"
    local ts host bundle bundle_name archive
    ts="$(date -u +%Y%m%dT%H%M%SZ)"
    host="$(hostname -s 2>/dev/null || hostname || echo unknown-host)"
    bundle_name="bluey-logs-${host}-${ts}"
    bundle="$WORK_ROOT/$bundle_name"
    archive="$ARCHIVE_ROOT/${bundle_name}.tar.gz"
    rm -rf "$bundle"
    mkdir -p "$bundle/journal" "$bundle/file-logs"

    {
        echo "created_at=$ts"
        echo "host=$host"
        echo "since=$SINCE"
        echo "services=$SERVICES"
        echo "log_dirs=$LOG_DIRS"
        echo "note=server operational logs only; user desktop logs remain redacted support exports"
    } > "$bundle/manifest.txt"

    local service
    # shellcheck disable=SC2086
    for service in $SERVICES; do
        if command -v journalctl >/dev/null 2>&1; then
            journalctl -u "$service" --since "$SINCE" --no-pager -o short-iso 2>/dev/null \
                | redact_stream > "$bundle/journal/${service}.log" || true
        fi
    done

    local log_dir count=0
    # shellcheck disable=SC2086
    for log_dir in $LOG_DIRS; do
        [ -d "$log_dir" ] || continue
        while IFS= read -r path; do
            count=$((count + 1))
            [ "$count" -le "$ARCHIVE_MAX_FILES" ] || break
            local safe_name dest
            safe_name="$(printf '%s' "$path" | sed 's#^/##; s#[^A-Za-z0-9._-]#_#g')"
            dest="$bundle/file-logs/$safe_name"
            case "$path" in
                *.gz)
                    if command -v gzip >/dev/null 2>&1; then
                        gzip -cd "$path" 2>/dev/null | redact_stream > "${dest%.gz}.log" || true
                    fi
                    ;;
                *)
                    redact_stream < "$path" > "$dest" || true
                    ;;
            esac
        done < <(list_files_newest "$log_dir" \
            \( -name '*.log' -o -name '*.log.*' -o -name '*.log-*' -o -name '*.jsonl' -o -name '*.gz' \))
    done

    tar -C "$WORK_ROOT" -czf "$archive" "$bundle_name"
    sha256sum "$archive" > "${archive}.sha256"
    upload_archive "$archive"
    index_archive_metadata "$archive"
    rm -rf "$bundle"
    echo "$(date -u +%FT%TZ) log archive ok: $archive"
}

prune_archives
prune_hot_logs

if [ "$PRUNE_ONLY" = "1" ]; then
    echo "$(date -u +%FT%TZ) log prune ok"
    exit 0
fi

archive_logs
