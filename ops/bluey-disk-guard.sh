#!/usr/bin/env bash
# Production disk guard for Bluey droplets.
#
# Default mode prints a deploy-safe storage report and exits non-zero when the
# root disk is too full. `--prune` also runs safe hot-cache cleanup steps.

set -euo pipefail

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

MODE="${1:-check}"
case "$MODE" in
    check|--check)
        PRUNE=0
        ;;
    --prune|prune)
        PRUNE=1
        ;;
    *)
        echo "usage: $0 [check|--prune]" >&2
        exit 2
        ;;
esac

ROOT_PATH="${BLUEY_DISK_GUARD_PATH:-/}"
MAX_USED_PCT="${BLUEY_DISK_MAX_USED_PCT:-80}"
MIN_FREE_GB="${BLUEY_DISK_MIN_FREE_GB:-8}"
API_ROOT="${BLUEY_API_ROOT:-/opt/bluey-api}"
WEB_ROOT="${BLUEY_WEB_ROOT:-/var/www/bluey}"
BACKUP_ROOT="${BLUEY_BACKUP_DIR:-/var/backups/bluey-api}"
LOG_ARCHIVE_SCRIPT="${BLUEY_LOG_ARCHIVE_SCRIPT:-/usr/local/sbin/archive-bluey-logs.sh}"
JOURNAL_VACUUM_TIME="${BLUEY_JOURNAL_VACUUM_TIME:-14d}"

failures=0

section() {
    printf '\n== %s ==\n' "$1"
}

dir_size() {
    local path="$1"
    if [ -e "$path" ]; then
        du -sh "$path" 2>/dev/null || true
    fi
}

section "disk"
df -h "$ROOT_PATH"

used_pct="$(df -Pk "$ROOT_PATH" | awk 'NR==2 {gsub("%","",$5); print $5}')"
free_kb="$(df -Pk "$ROOT_PATH" | awk 'NR==2 {print $4}')"
min_free_kb=$((MIN_FREE_GB * 1024 * 1024))

if [ "${used_pct:-0}" -ge "$MAX_USED_PCT" ]; then
    echo "fail: $ROOT_PATH is ${used_pct}% used; threshold is ${MAX_USED_PCT}%" >&2
    failures=$((failures + 1))
fi
if [ "${free_kb:-0}" -lt "$min_free_kb" ]; then
    echo "fail: $ROOT_PATH has less than ${MIN_FREE_GB}G free" >&2
    failures=$((failures + 1))
fi

section "bluey hot data"
dir_size "$API_ROOT"
dir_size "$BACKUP_ROOT"
dir_size /var/log/bluey-api
dir_size "$WEB_ROOT/releases"

section "journal"
journalctl --disk-usage 2>/dev/null || true

if [ "$PRUNE" = "1" ]; then
    section "safe prune"
    find /tmp -maxdepth 1 -type f -name 'bluey-*' -mtime +2 -print -delete 2>/dev/null || true
    if command -v journalctl >/dev/null 2>&1; then
        journalctl --vacuum-time="$JOURNAL_VACUUM_TIME" || true
    fi
    if command -v logrotate >/dev/null 2>&1; then
        logrotate -f /etc/logrotate.d/bluey-api 2>/dev/null || true
    fi
    if [ -x "$LOG_ARCHIVE_SCRIPT" ]; then
        "$LOG_ARCHIVE_SCRIPT" --prune-only || true
    fi
    section "disk after prune"
    df -h "$ROOT_PATH"
fi

section "recent storage errors"
journalctl -u bluey-api -u caddy --since '24 hours ago' --no-pager 2>/dev/null |
    grep -Ei 'database or disk is full|no space|AccessDenied|SignatureDoesNotMatch|backup failed|failed backup|backup upload|object delete|export failed|panic|log archive failed|archive failed|failed archive' |
    tail -80 || true

if [ "$failures" -gt 0 ]; then
    exit 1
fi

echo "ok: disk guard passed"
