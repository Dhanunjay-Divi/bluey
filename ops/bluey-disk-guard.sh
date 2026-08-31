#!/usr/bin/env bash
# Production disk guard for Bluey hosts.
#
# Check mode mutates only durable status/alert state and the canonical,
# root-owned shared backup-lock inode.
# Prune mode removes only bounded temp/log caches, then recomputes capacity and
# evaluates the post-prune result. It never removes databases, backup pairs,
# release artifacts, rollback trees, or source/build directories.

set +x
set -euo pipefail
umask 077

BLUEY_ENV_FILE="${BLUEY_ENV_FILE:-/etc/bluey-api/bluey-api.env}"
BLUEY_STORAGE_ENV_FILE="${BLUEY_STORAGE_ENV_FILE:-/etc/bluey-api/bluey-storage.env}"

bootstrap_stat() {
    local format_gnu="$1" format_bsd="$2" path="$3"
    if stat "$format_gnu" -- "$path" >/dev/null 2>&1; then
        stat "$format_gnu" -- "$path"
    else
        stat "$format_bsd" -- "$path"
    fi
}

bootstrap_trusted_file() {
    local path="$1" owner mode mode_value
    [ -f "$path" ] && [ ! -L "$path" ] || return 1
    owner="$(bootstrap_stat -c%u -f%u "$path")" || return 1
    if [ "$EUID" = "0" ]; then
        [ "$owner" = "0" ] || return 1
    else
        { [ "$owner" = "0" ] || [ "$owner" = "$EUID" ]; } || return 1
    fi
    mode="$(bootstrap_stat -c%a -f%Lp "$path")" || return 1
    case "$mode" in ""|*[!0-9]*) return 1 ;; esac
    mode_value=$((8#$mode))
    [ $((mode_value & 8#022)) -eq 0 ]
}

bootstrap_trusted_parent_chain() {
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
bootstrap_identity() { bootstrap_stat -c%i -f%i "$1"; }

bootstrap_follow_identity() {
    if stat -L -c%i -- "$1" >/dev/null 2>&1; then
        stat -L -c%i -- "$1"
    else
        stat -L -f%i -- "$1"
    fi
}

load_env_file() {
    local env_file="$1" before after
    if [ -e "$env_file" ] || [ -L "$env_file" ]; then
        [ -r "$env_file" ] && bootstrap_trusted_parent_chain "$env_file" && bootstrap_trusted_file "$env_file" || {
            echo "disk guard failed: environment file is not trusted and write-protected" >&2
            exit 1
        }
        before="$(bootstrap_identity "$env_file")" || exit 1
        exec 9<"$env_file"; after="$(bootstrap_follow_identity /dev/fd/9)" || exit 1
        [ "$before" = "$after" ] || exit 1
        # shellcheck disable=SC1090
        . /dev/fd/9
        exec 9<&-
    fi
}

load_env_file "$BLUEY_ENV_FILE"
load_env_file "$BLUEY_STORAGE_ENV_FILE"
if [ -n "${BLUEY_EXTRA_ENV_FILES:-}" ]; then
    # shellcheck disable=SC2086
    for env_file in $BLUEY_EXTRA_ENV_FILES; do
        load_env_file "$env_file"
    done
else
    load_env_file /etc/bluey-api/bluey-postgres.env
fi
unset -f bootstrap_stat bootstrap_trusted_file bootstrap_trusted_parent_chain bootstrap_identity \
    bootstrap_follow_identity

MODE="${1:-check}"
case "$MODE" in
    check|--check) PRUNE=0 ;;
    --prune|prune) PRUNE=1 ;;
    --check-config) PRUNE=0 ;;
    *) echo "usage: $0 [check|--prune|--check-config]" >&2; exit 2 ;;
esac

ROOT_PATH="${BLUEY_DISK_GUARD_PATH-/}"
MAX_USED_PCT="${BLUEY_DISK_MAX_USED_PCT:-80}"
MIN_FREE_GB="${BLUEY_DISK_MIN_FREE_GB:-8}"
WARN_USED_PCT="${BLUEY_DISK_WARN_USED_PCT:-70}"
WARN_MIN_FREE_GB="${BLUEY_DISK_WARN_MIN_FREE_GB:-16}"
BACKUP_WARN_AGE_MINUTES="${BLUEY_DISK_BACKUP_WARN_AGE_MINUTES:-120}"
BACKUP_HARD_AGE_MINUTES="${BLUEY_DISK_BACKUP_HARD_AGE_MINUTES:-180}"
BACKUP_DAILY_WARN_AGE_MINUTES="${BLUEY_DISK_BACKUP_DAILY_WARN_AGE_MINUTES:-2160}"
BACKUP_DAILY_HARD_AGE_MINUTES="${BLUEY_DISK_BACKUP_DAILY_HARD_AGE_MINUTES:-2880}"
BACKUP_HEALTH_REQUIRED="${BLUEY_DISK_BACKUP_HEALTH_REQUIRED:-0}"
BACKUP_DAILY_HEALTH_REQUIRED="${BLUEY_DISK_BACKUP_DAILY_HEALTH_REQUIRED:-0}"
BACKUP_RUN_STATUS_REQUIRED="${BLUEY_DISK_BACKUP_RUN_STATUS_REQUIRED:-0}"
BACKUP_REQUIRE_ROOT="${BLUEY_DISK_BACKUP_REQUIRE_ROOT_OWNERSHIP:-1}"
BACKUP_WRITER_WARN_AGE_MINUTES="${BLUEY_DISK_BACKUP_WRITER_WARN_AGE_MINUTES:-45}"
BACKUP_WRITER_HARD_AGE_MINUTES="${BLUEY_DISK_BACKUP_WRITER_HARD_AGE_MINUTES:-90}"
LOG_ARCHIVE_HEALTH_REQUIRED="${BLUEY_DISK_LOG_ARCHIVE_HEALTH_REQUIRED:-0}"
LOG_ARCHIVE_WARN_AGE_MINUTES="${BLUEY_DISK_LOG_ARCHIVE_WARN_AGE_MINUTES:-120}"
LOG_ARCHIVE_HARD_AGE_MINUTES="${BLUEY_DISK_LOG_ARCHIVE_HARD_AGE_MINUTES:-180}"
LOG_ARCHIVE_WRITER_WARN_AGE_MINUTES="${BLUEY_DISK_LOG_ARCHIVE_WRITER_WARN_AGE_MINUTES:-45}"
LOG_ARCHIVE_WRITER_HARD_AGE_MINUTES="${BLUEY_DISK_LOG_ARCHIVE_WRITER_HARD_AGE_MINUTES:-90}"
BACKUP_REQUIRE_OFFSITE="${BLUEY_BACKUP_REQUIRE_OFFSITE:-0}"
OFFSITE_DESTINATION="${OFFSITE_DESTINATION:-}"
DB_BACKEND="$(printf '%s' "${BLUEY_SERVER_DB_BACKEND:-sqlite}" | tr '[:upper:]' '[:lower:]')"
API_ROOT="${BLUEY_API_ROOT-/opt/bluey-api}"
WEB_ROOT="${BLUEY_WEB_ROOT-/var/www/bluey}"
BACKUP_ROOT="${BLUEY_BACKUP_DIR-/var/backups/bluey-api}"
BACKUP_STATUS_FILE="${BLUEY_BACKUP_STATUS_FILE-$BACKUP_ROOT/.backup.status}"
BACKUP_LOCAL_MAX_BYTES="${BLUEY_BACKUP_LOCAL_MAX_BYTES:-12884901888}"
LOG_ROOT="${BLUEY_API_LOG_DIR-/var/log/bluey-api}"
LOG_WORK_ROOT="${BLUEY_LOG_ARCHIVE_WORK_DIR-/var/lib/bluey-ops/log-archive}"
BUILD_SCAN_ROOT="${BLUEY_DISK_BUILD_SCAN_ROOT-/opt}"
TMP_PRUNE_ROOT="${BLUEY_DISK_GUARD_TMP_ROOT-/var/lib/bluey-ops/tmp}"
LOG_ARCHIVE_SCRIPT="${BLUEY_LOG_ARCHIVE_SCRIPT:-/usr/local/sbin/archive-bluey-logs.sh}"
JOURNAL_VACUUM_TIME="${BLUEY_JOURNAL_VACUUM_TIME:-14d}"
STATUS_FILE="${BLUEY_DISK_GUARD_STATUS_FILE-/var/lib/bluey-ops/disk-guard.status}"
GUARD_LOCK_FILE="${BLUEY_DISK_GUARD_LOCK_FILE-$(dirname "$STATUS_FILE")/disk-guard.lock}"
OPS_STATE_ROOT="${BLUEY_OPS_STATE_ROOT-/var/lib/bluey-ops}"
LOG_ARCHIVE_STATUS_FILE="${BLUEY_LOG_ARCHIVE_STATUS_FILE-$OPS_STATE_ROOT/log-archive.status}"
REQUIRE_STATUS="${BLUEY_DISK_GUARD_REQUIRE_STATUS:-1}"
ALERT_WEBHOOK_URL="${BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL:-}"
# Values sourced from root-only policy remain shell variables but are removed
# from the inherited environment of every filesystem/journal/alert helper.
while IFS= read -r inherited_name; do
    case "$inherited_name" in
        *KEY*|*TOKEN*|*SECRET*|*PASSWORD*|*DATABASE_URL*|*DSN*|*WEBHOOK*|*CREDENTIAL*|*COOKIE*|*AUTH*)
            export -n "$inherited_name" 2>/dev/null || true
            ;;
    esac
done < <(compgen -e)
unset inherited_name
REQUIRE_ALERT="${BLUEY_DISK_GUARD_REQUIRE_ALERT:-0}"
ALERT_REPEAT_MINUTES="${BLUEY_DISK_GUARD_ALERT_REPEAT_MINUTES:-60}"
REQUIRE_TRUSTED_PATHS="${BLUEY_DISK_GUARD_REQUIRE_TRUSTED_PATHS:-1}"
WARN_INODE_USED_PCT="${BLUEY_DISK_WARN_INODE_USED_PCT:-70}"
MAX_INODE_USED_PCT="${BLUEY_DISK_MAX_INODE_USED_PCT:-80}"

failures=0
warnings=0
used_pct=0
free_kb=0
previous_status="unknown"
previous_last_alert_epoch=0
last_alert_epoch=0
previous_fingerprint="unknown"
reason_codes=""
inode_used_pct=0
backup_status=unknown
backup_name=none
backup_age_minutes=-1
backup_snapshot_bytes=0
backup_checksum_status=unknown
backup_offsite_status=unknown
backup_writer_active=0
backup_writer_age_minutes=-1
backup_reader_lock_held=0
backup_run_status=unknown
backup_unverified_count=0
backup_hot_bytes=0
backup_daily_status=unknown
backup_daily_name=none
backup_daily_age_minutes=-1
log_archive_status=unknown
log_archive_age_minutes=-1
log_archive_writer_age_minutes=-1
log_archive_run_status=unknown
log_archive_name=none

is_uint() {
    case "$1" in
        ""|*[!0-9]*) return 1 ;;
        *) return 0 ;;
    esac
}

add_reason() {
    local code="$1"
    case ",$reason_codes," in
        *,"$code",*) ;;
        *)
            if [ -n "$reason_codes" ]; then
                reason_codes="$reason_codes,$code"
            else
                reason_codes="$code"
            fi
            ;;
    esac
}

record_failure() {
    local code="$1"
    shift
    add_reason "$code"
    echo "fail: $*" >&2
    failures=$((failures + 1))
}

record_warning() {
    local code="$1"
    shift
    add_reason "$code"
    echo "warn: $*" >&2
    warnings=$((warnings + 1))
}

config_failure() {
    record_failure config "$*"
}

validate_safe_directory() {
    local name="$1" path="$2" minimum_components="$3"
    local remainder component component_count=0 canonical
    if [ -z "$path" ]; then
        config_failure "$name must not be empty"
        return
    fi
    case "$path" in
        /*) ;;
        *) config_failure "$name must be an absolute path"; return ;;
    esac
    remainder="${path#/}"
    while [ -n "$remainder" ]; do
        component="${remainder%%/*}"
        if [ "$remainder" = "$component" ]; then
            remainder=""
        else
            remainder="${remainder#*/}"
        fi
        case "$component" in
            "") ;;
            .|..) config_failure "$name contains an unsafe path segment"; return ;;
            *) component_count=$((component_count + 1)) ;;
        esac
    done
    if [ "$component_count" -lt "$minimum_components" ]; then
        config_failure "$name is an unsafe broad filesystem root"
        return
    fi
    if [ -L "$path" ]; then
        config_failure "$name must not be a symlink"
        return
    fi
    if [ -d "$path" ]; then
        canonical="$(cd -P -- "$path" 2>/dev/null && pwd -P)" || {
            config_failure "$name cannot be resolved"
            return
        }
        if [ -z "${canonical//\//}" ]; then
            config_failure "$name resolves to the filesystem root"
        fi
    fi
}

for setting_name in \
    MAX_USED_PCT MIN_FREE_GB WARN_USED_PCT WARN_MIN_FREE_GB ALERT_REPEAT_MINUTES \
    BACKUP_WARN_AGE_MINUTES BACKUP_HARD_AGE_MINUTES \
    BACKUP_DAILY_WARN_AGE_MINUTES BACKUP_DAILY_HARD_AGE_MINUTES \
    BACKUP_WRITER_WARN_AGE_MINUTES BACKUP_WRITER_HARD_AGE_MINUTES \
    LOG_ARCHIVE_WARN_AGE_MINUTES LOG_ARCHIVE_HARD_AGE_MINUTES \
    LOG_ARCHIVE_WRITER_WARN_AGE_MINUTES LOG_ARCHIVE_WRITER_HARD_AGE_MINUTES \
    BACKUP_LOCAL_MAX_BYTES WARN_INODE_USED_PCT MAX_INODE_USED_PCT; do
    eval "setting_value=\${$setting_name}"
    is_uint "$setting_value" || config_failure "$setting_name must be an unsigned integer"
done
is_uint "$MAX_USED_PCT" || MAX_USED_PCT=80
is_uint "$MIN_FREE_GB" || MIN_FREE_GB=8
is_uint "$WARN_USED_PCT" || WARN_USED_PCT=70
is_uint "$WARN_MIN_FREE_GB" || WARN_MIN_FREE_GB=16
is_uint "$ALERT_REPEAT_MINUTES" || ALERT_REPEAT_MINUTES=60
is_uint "$BACKUP_WARN_AGE_MINUTES" || BACKUP_WARN_AGE_MINUTES=120
is_uint "$BACKUP_HARD_AGE_MINUTES" || BACKUP_HARD_AGE_MINUTES=180
is_uint "$BACKUP_DAILY_WARN_AGE_MINUTES" || BACKUP_DAILY_WARN_AGE_MINUTES=2160
is_uint "$BACKUP_DAILY_HARD_AGE_MINUTES" || BACKUP_DAILY_HARD_AGE_MINUTES=2880
is_uint "$BACKUP_WRITER_WARN_AGE_MINUTES" || BACKUP_WRITER_WARN_AGE_MINUTES=45
is_uint "$BACKUP_WRITER_HARD_AGE_MINUTES" || BACKUP_WRITER_HARD_AGE_MINUTES=90
is_uint "$LOG_ARCHIVE_WARN_AGE_MINUTES" || LOG_ARCHIVE_WARN_AGE_MINUTES=120
is_uint "$LOG_ARCHIVE_HARD_AGE_MINUTES" || LOG_ARCHIVE_HARD_AGE_MINUTES=180
is_uint "$LOG_ARCHIVE_WRITER_WARN_AGE_MINUTES" || LOG_ARCHIVE_WRITER_WARN_AGE_MINUTES=45
is_uint "$LOG_ARCHIVE_WRITER_HARD_AGE_MINUTES" || LOG_ARCHIVE_WRITER_HARD_AGE_MINUTES=90
is_uint "$BACKUP_LOCAL_MAX_BYTES" || BACKUP_LOCAL_MAX_BYTES=12884901888
is_uint "$WARN_INODE_USED_PCT" || WARN_INODE_USED_PCT=70
is_uint "$MAX_INODE_USED_PCT" || MAX_INODE_USED_PCT=80
case "$REQUIRE_STATUS" in 0|1) ;; *) config_failure "BLUEY_DISK_GUARD_REQUIRE_STATUS must be 0 or 1" ;; esac
case "$REQUIRE_ALERT" in 0|1) ;; *) config_failure "BLUEY_DISK_GUARD_REQUIRE_ALERT must be 0 or 1" ;; esac
case "$BACKUP_HEALTH_REQUIRED" in 0|1) ;; *) config_failure "BLUEY_DISK_BACKUP_HEALTH_REQUIRED must be 0 or 1" ;; esac
case "$BACKUP_DAILY_HEALTH_REQUIRED" in 0|1) ;; *) config_failure "BLUEY_DISK_BACKUP_DAILY_HEALTH_REQUIRED must be 0 or 1" ;; esac
case "$BACKUP_RUN_STATUS_REQUIRED" in 0|1) ;; *) config_failure "BLUEY_DISK_BACKUP_RUN_STATUS_REQUIRED must be 0 or 1" ;; esac
case "$BACKUP_REQUIRE_ROOT" in 0|1) ;; *) config_failure "BLUEY_DISK_BACKUP_REQUIRE_ROOT_OWNERSHIP must be 0 or 1" ;; esac
case "$BACKUP_REQUIRE_OFFSITE" in 0|1) ;; *) config_failure "BLUEY_BACKUP_REQUIRE_OFFSITE must be 0 or 1" ;; esac
case "$LOG_ARCHIVE_HEALTH_REQUIRED" in 0|1) ;; *) config_failure "BLUEY_DISK_LOG_ARCHIVE_HEALTH_REQUIRED must be 0 or 1" ;; esac
case "$REQUIRE_TRUSTED_PATHS" in 0|1) ;; *) config_failure "BLUEY_DISK_GUARD_REQUIRE_TRUSTED_PATHS must be 0 or 1" ;; esac
if is_uint "$MAX_USED_PCT" && { [ "$MAX_USED_PCT" -lt 1 ] || [ "$MAX_USED_PCT" -gt 100 ]; }; then
    config_failure "BLUEY_DISK_MAX_USED_PCT must be between 1 and 100"
fi
if [ "$WARN_USED_PCT" -ge "$MAX_USED_PCT" ]; then
    config_failure "BLUEY_DISK_WARN_USED_PCT must be below the hard used-percent threshold"
fi
if [ "$WARN_MIN_FREE_GB" -le "$MIN_FREE_GB" ]; then
    config_failure "BLUEY_DISK_WARN_MIN_FREE_GB must exceed the hard free-space threshold"
fi
if [ "$BACKUP_WARN_AGE_MINUTES" -ge "$BACKUP_HARD_AGE_MINUTES" ]; then
    config_failure "backup warning age must be below the hard age"
fi
if [ "$BACKUP_DAILY_WARN_AGE_MINUTES" -ge "$BACKUP_DAILY_HARD_AGE_MINUTES" ]; then
    config_failure "daily backup warning age must be below the hard age"
fi
if [ "$BACKUP_WRITER_WARN_AGE_MINUTES" -ge "$BACKUP_WRITER_HARD_AGE_MINUTES" ]; then
    config_failure "backup writer warning age must be below the hard age"
fi
if [ "$LOG_ARCHIVE_WARN_AGE_MINUTES" -ge "$LOG_ARCHIVE_HARD_AGE_MINUTES" ]; then
    config_failure "log archive warning age must be below the hard age"
fi
if [ "$LOG_ARCHIVE_WRITER_WARN_AGE_MINUTES" -ge "$LOG_ARCHIVE_WRITER_HARD_AGE_MINUTES" ]; then
    config_failure "log archive writer warning age must be below the hard age"
fi
if [ "$WARN_INODE_USED_PCT" -ge "$MAX_INODE_USED_PCT" ] ||
    [ "$MAX_INODE_USED_PCT" -gt 100 ]; then
    config_failure "inode warning threshold must be below a hard threshold no greater than 100"
fi
if [ "$REQUIRE_ALERT" = "1" ] && [ -z "$ALERT_WEBHOOK_URL" ]; then
    config_failure "BLUEY_DISK_GUARD_REQUIRE_ALERT=1 requires an HTTPS alert webhook"
fi
if [ -n "$ALERT_WEBHOOK_URL" ]; then
    case "$ALERT_WEBHOOK_URL" in
        https://*) ;;
        *) config_failure "BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL must use HTTPS" ;;
    esac
    case "$ALERT_WEBHOOK_URL" in
        *replace-with*|*.invalid*)
            config_failure "BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL is still a placeholder"
            ;;
    esac
    if printf '%s' "$ALERT_WEBHOOK_URL" | LC_ALL=C grep -q '[[:cntrl:]]'; then
        config_failure "BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL contains control characters"
    fi
    command -v curl >/dev/null 2>&1 || config_failure "curl is required for disk alerts"
fi
validate_safe_directory BLUEY_BACKUP_DIR "$BACKUP_ROOT" 2
validate_safe_directory BLUEY_DISK_GUARD_TMP_ROOT "$TMP_PRUNE_ROOT" 2
validate_safe_directory BLUEY_DISK_GUARD_STATUS_ROOT "$(dirname "$STATUS_FILE")" 2
validate_safe_directory BLUEY_DISK_GUARD_LOCK_ROOT "$(dirname "$GUARD_LOCK_FILE")" 2
validate_safe_directory BLUEY_BACKUP_STATUS_ROOT "$(dirname "$BACKUP_STATUS_FILE")" 2
validate_safe_directory BLUEY_OPS_STATE_ROOT "$OPS_STATE_ROOT" 2
validate_safe_directory BLUEY_LOG_ARCHIVE_STATUS_ROOT "$(dirname "$LOG_ARCHIVE_STATUS_FILE")" 2
validate_safe_directory BLUEY_DISK_GUARD_STATUS_FILE "$STATUS_FILE" 3
validate_safe_directory BLUEY_DISK_GUARD_LOCK_FILE "$GUARD_LOCK_FILE" 3
validate_safe_directory BLUEY_BACKUP_STATUS_FILE "$BACKUP_STATUS_FILE" 3
validate_safe_directory BLUEY_LOG_ARCHIVE_STATUS_FILE "$LOG_ARCHIVE_STATUS_FILE" 3
case "$BACKUP_STATUS_FILE" in
    "$BACKUP_ROOT"/*) ;;
    *) config_failure "BLUEY_BACKUP_STATUS_FILE must be inside BLUEY_BACKUP_DIR" ;;
esac
case "$GUARD_LOCK_FILE" in
    "$(dirname "$STATUS_FILE")"/*) ;;
    *) config_failure "BLUEY_DISK_GUARD_LOCK_FILE must be inside the status directory" ;;
esac
case "$LOG_ARCHIVE_STATUS_FILE" in
    "$OPS_STATE_ROOT"/*) ;;
    *) config_failure "BLUEY_LOG_ARCHIVE_STATUS_FILE must be inside BLUEY_OPS_STATE_ROOT" ;;
esac
if [ "$(dirname "$LOG_ARCHIVE_STATUS_FILE")" != "$OPS_STATE_ROOT" ]; then
    config_failure "BLUEY_LOG_ARCHIVE_STATUS_FILE must be directly inside BLUEY_OPS_STATE_ROOT"
fi

# Configuration failures stop before any prune, directory creation, or status
# mutation. A typo must never broaden a root-owned deletion boundary.
[ "$failures" -eq 0 ] || exit 1
if [ "$MODE" = "--check-config" ]; then
    echo "Bluey disk guard configuration is safe."
    exit 0
fi

section() {
    printf '\n== %s ==\n' "$1"
}

dir_bytes() {
    local path="$1"
    if [ ! -e "$path" ]; then
        echo 0
    elif du -sb "$path" >/dev/null 2>&1; then
        du -sb "$path" | awk '{print $1}'
    else
        du -sk "$path" | awk '{print $1 * 1024}'
    fi
}

file_size_bytes() {
    if stat -c%s "$1" >/dev/null 2>&1; then
        stat -c%s "$1"
    else
        stat -f%z "$1"
    fi
}

file_mtime_epoch() {
    if stat -c%Y "$1" >/dev/null 2>&1; then
        stat -c%Y "$1"
    else
        stat -f%m "$1"
    fi
}

file_owner_uid() {
    if stat -c%u "$1" >/dev/null 2>&1; then
        stat -c%u "$1"
    else
        stat -f%u "$1"
    fi
}

file_mode() {
    if stat -c%a "$1" >/dev/null 2>&1; then
        stat -c%a "$1"
    else
        stat -f%Lp "$1"
    fi
}

trusted_directory_chain() {
    local path="$1" remainder component current="" owner mode mode_value effective_uid
    [ "$REQUIRE_TRUSTED_PATHS" = "1" ] || return 0
    effective_uid="$(id -u)"
    remainder="${path#/}"
    while [ -n "$remainder" ]; do
        component="${remainder%%/*}"
        if [ "$remainder" = "$component" ]; then remainder=""; else remainder="${remainder#*/}"; fi
        [ -n "$component" ] || continue
        current="$current/$component"
        [ -e "$current" ] || continue
        [ -d "$current" ] && [ ! -L "$current" ] || return 1
        owner="$(file_owner_uid "$current")"
        if [ "$effective_uid" = "0" ]; then
            [ "$owner" = "0" ] || return 1
        else
            { [ "$owner" = "0" ] || [ "$owner" = "$effective_uid" ]; } || return 1
        fi
        mode="$(file_mode "$current")"
        is_uint "$mode" || return 1
        mode_value=$((8#$mode))
        [ $((mode_value & 8#022)) -eq 0 ] || return 1
    done
}

trusted_regular_file() {
    local path="$1" mode mode_value owner effective_uid
    [ -L "$path" ] && return 1
    [ -e "$path" ] || return 0
    [ -f "$path" ] || return 1
    [ "$REQUIRE_TRUSTED_PATHS" = "1" ] || return 0
    owner="$(file_owner_uid "$path")"
    effective_uid="$(id -u)"
    if [ "$effective_uid" = "0" ]; then
        [ "$owner" = "0" ] || return 1
    else
        { [ "$owner" = "0" ] || [ "$owner" = "$effective_uid" ]; } || return 1
    fi
    mode="$(file_mode "$path")"
    is_uint "$mode" || return 1
    mode_value=$((8#$mode))
    [ $((mode_value & 8#022)) -eq 0 ]
}

root_owned_read_only_proof() {
    local path="$1" mode mode_value
    [ "$BACKUP_REQUIRE_ROOT" = "1" ] || return 0
    [ "$(file_owner_uid "$path")" = "0" ] || return 1
    mode="$(file_mode "$path")"
    is_uint "$mode" || return 1
    mode_value=$((8#$mode))
    [ $((mode_value & 8#022)) -eq 0 ]
}

supports_find_printf() {
    find "${1:-.}" -maxdepth 0 -printf '' >/dev/null 2>&1
}

backup_extension() {
    case "$DB_BACKEND" in
        sqlite|"") printf 'db\n' ;;
        postgres|postgresql) printf 'pgdump\n' ;;
        *) return 1 ;;
    esac
}

list_active_backups_newest() {
    local dir="$1" extension
    extension="$(backup_extension)" || return 1
    [ -d "$dir" ] || return 0
    if supports_find_printf "$dir"; then
        find "$dir" -maxdepth 1 -type f -name "*.${extension}" \
            -printf '%T@ %p\n' 2>/dev/null | sort -rn | cut -d' ' -f2-
    else
        find "$dir" -maxdepth 1 -type f -name "*.${extension}" \
            -exec stat -f '%m %N' {} \; 2>/dev/null | sort -rn | cut -d' ' -f2-
    fi
}

newest_active_backup_in() {
    list_active_backups_newest "$1" | head -1
}

expected_backup_destination() {
    local backup="$1" layout="${2:-class-prefix}" source_path source_name class
    [ -n "$OFFSITE_DESTINATION" ] || return 1
    [ -f "${backup}.sha256" ] || return 1
    source_path="$(awk 'NR == 1 {print $NF}' "${backup}.sha256")"
    source_name="$(basename "${source_path#\*}")"
    case "$source_name" in
        bluey-*.db|bluey-postgres-*.pgdump) ;;
        *) return 1 ;;
    esac
    [ "${source_name##*.}" = "${backup##*.}" ] || return 1
    if [ "$layout" = "legacy-flat" ]; then
        printf '%s/%s\n' "${OFFSITE_DESTINATION%/}" "$source_name"
        return
    fi
    [ "$layout" = "class-prefix" ] || return 1
    case "$backup" in
        "$BACKUP_ROOT/hourly"/*) class=hourly ;;
        "$BACKUP_ROOT/daily"/*) class=daily ;;
        *) return 1 ;;
    esac
    printf '%s/%s/%s\n' "${OFFSITE_DESTINATION%/}" "$class" "$source_name"
}

backup_metadata_complete() {
    local backup="$1" checksum marker bytes checksum_sha
    local marker_schema marker_sha marker_bytes marker_destination marker_layout expected_destination
    [ -f "$backup" ] || return 1
    checksum="${backup}.sha256"
    [ -f "$checksum" ] || return 1
    bytes="$(file_size_bytes "$backup")"
    checksum_sha="$(awk 'NR == 1 {print $1}' "$checksum")"
    printf '%s\n' "$checksum_sha" | grep -Eq '^[0-9a-f]{64}$' || return 1
    root_owned_read_only_proof "$backup" || return 1
    root_owned_read_only_proof "$checksum" || return 1
    if [ "$BACKUP_REQUIRE_OFFSITE" = "1" ] || [ -n "$OFFSITE_DESTINATION" ]; then
        marker="${backup}.offsite-verified"
        [ -f "$marker" ] && root_owned_read_only_proof "$marker" || return 1
        marker_layout="$(awk -F= '$1 == "layout" {print $2; exit}' "$marker")"
        [ -n "$marker_layout" ] || marker_layout=class-prefix
        expected_destination="$(expected_backup_destination "$backup" "$marker_layout")" || return 1
        marker_schema="$(awk -F= '$1 == "schema" {print $2; exit}' "$marker")"
        marker_sha="$(awk -F= '$1 == "sha256" {print $2; exit}' "$marker")"
        marker_bytes="$(awk -F= '$1 == "bytes" {print $2; exit}' "$marker")"
        marker_destination="$(awk -F= '$1 == "destination" {print $2; exit}' "$marker")"
        [ "$marker_schema" = "1" ] && [ "$marker_sha" = "$checksum_sha" ] &&
            [ "$marker_bytes" = "$bytes" ] &&
            [ "$marker_destination" = "$expected_destination" ] || return 1
    fi
}

newest_complete_backup_in() {
    local dir="$1" backup
    while IFS= read -r backup; do
        [ -n "$backup" ] || continue
        if backup_metadata_complete "$backup"; then
            printf '%s\n' "$backup"
            return 0
        fi
    done < <(list_active_backups_newest "$dir")
    return 1
}

backup_health_failure() {
    local code="$1"
    shift
    backup_status=fail
    record_failure "$code" "$*"
}

inspect_backup_writer() {
    local now_epoch="$1" lock_file="$BACKUP_ROOT/.backup.lock" lock_mtime
    if ! trusted_regular_file "$lock_file"; then
        backup_health_failure backup_lock_untrusted \
            "backup lock is not a trusted regular file"
        return
    fi
    command -v flock >/dev/null 2>&1 || {
        backup_health_failure backup_lock_unavailable "flock is required for backup health"
        return
    }
    # Open the canonical lock even on first install, then retain a successful
    # shared lock for the complete metadata scan. Releasing it here would leave
    # a race where a backup can finalize its payload/sidecar before its remote
    # proof marker and the guard can falsely inspect that in-progress snapshot.
    exec 7>>"$lock_file"
    chmod 0600 "$lock_file"
    if flock -sn 7; then
        backup_reader_lock_held=1
        return
    fi
    backup_writer_active=1
    lock_mtime="$(file_mtime_epoch "$lock_file")"
    if is_uint "$lock_mtime" && [ "$lock_mtime" -le "$now_epoch" ]; then
        backup_writer_age_minutes=$(((now_epoch - lock_mtime) / 60))
    fi
    if [ "$backup_writer_age_minutes" -ge "$BACKUP_WRITER_HARD_AGE_MINUTES" ]; then
        backup_health_failure backup_writer_stuck \
            "backup writer has held the lock for ${backup_writer_age_minutes} minutes"
    elif [ "$backup_writer_age_minutes" -ge "$BACKUP_WRITER_WARN_AGE_MINUTES" ]; then
        record_warning backup_writer_slow \
            "backup writer has held the lock for ${backup_writer_age_minutes} minutes"
    fi
}

inspect_backup_run_status() {
    local run_state schema
    if [ ! -f "$BACKUP_STATUS_FILE" ]; then
        backup_run_status=missing
        if [ "$BACKUP_RUN_STATUS_REQUIRED" = "1" ]; then
            backup_health_failure backup_run_status_missing "durable backup run status is missing"
        fi
        return
    fi
    if ! root_owned_read_only_proof "$BACKUP_STATUS_FILE"; then
        backup_run_status=untrusted
        backup_health_failure backup_run_status_untrusted "backup run status is not trusted"
        return
    fi
    schema="$(awk -F= '$1 == "schema" {print $2; exit}' "$BACKUP_STATUS_FILE")"
    run_state="$(awk -F= '$1 == "status" {print $2; exit}' "$BACKUP_STATUS_FILE")"
    [ "$schema" = "1" ] || run_state=invalid
    backup_run_status="$run_state"
    case "$run_state" in
        ok) ;;
        running)
            if [ "$backup_writer_active" != "1" ]; then
                backup_health_failure backup_run_interrupted \
                    "backup status is running but no writer holds the lock"
            fi
            ;;
        fail) backup_health_failure backup_run_failed "the last backup run failed" ;;
        *) backup_health_failure backup_run_status_invalid "backup run status is invalid" ;;
    esac
}

inspect_unverified_backlog() {
    local dir backup mtime lock_mtime=0
    if [ "$backup_writer_active" = "1" ] && [ -f "$BACKUP_ROOT/.backup.lock" ]; then
        lock_mtime="$(file_mtime_epoch "$BACKUP_ROOT/.backup.lock")"
        is_uint "$lock_mtime" || lock_mtime=0
    fi
    for dir in "$BACKUP_ROOT/hourly" "$BACKUP_ROOT/daily"; do
        while IFS= read -r backup; do
            [ -n "$backup" ] || continue
            backup_metadata_complete "$backup" && continue
            mtime="$(file_mtime_epoch "$backup")"
            if [ "$backup_writer_active" = "1" ] && is_uint "$mtime" &&
                [ "$mtime" -ge "$lock_mtime" ]; then
                continue
            fi
            backup_unverified_count=$((backup_unverified_count + 1))
        done < <(list_active_backups_newest "$dir")
    done
    if [ "$backup_unverified_count" -gt 0 ]; then
        backup_health_failure backup_unverified_backlog \
            "$backup_unverified_count finalized snapshot(s) lack current exact proof"
    fi
}

inspect_one_hourly_backup() {
    local now_epoch="$1" backup="$2" checksum marker mtime age_seconds
    local checksum_sha marker_sha marker_bytes marker_schema marker_destination marker_layout
    local expected_destination
    backup_name="$(basename "$backup")"
    backup_snapshot_bytes="$(file_size_bytes "$backup")"
    mtime="$(file_mtime_epoch "$backup")"
    if ! is_uint "$mtime" || [ "$mtime" -gt $((now_epoch + 300)) ]; then
        backup_health_failure backup_timestamp_invalid \
            "newest database snapshot has an invalid or future timestamp"
        return
    fi
    age_seconds=$((now_epoch - mtime))
    backup_age_minutes=$((age_seconds / 60))

    checksum="${backup}.sha256"
    if [ ! -f "$checksum" ]; then
        backup_checksum_status=missing
        backup_health_failure backup_checksum_missing "$backup_name has no checksum sidecar"
        return
    fi
    checksum_sha="$(awk 'NR == 1 {print $1}' "$checksum")"
    if ! printf '%s\n' "$checksum_sha" | grep -Eq '^[0-9a-f]{64}$'; then
        backup_checksum_status=invalid
        backup_health_failure backup_checksum_invalid "$backup_name has a malformed checksum sidecar"
        return
    fi
    if ! root_owned_read_only_proof "$backup" || ! root_owned_read_only_proof "$checksum"; then
        backup_checksum_status=untrusted
        backup_health_failure backup_checksum_untrusted \
            "$backup_name or its checksum is not root-owned and write-protected"
        return
    fi
    backup_checksum_status=ok

    if [ "$BACKUP_REQUIRE_OFFSITE" = "1" ] || [ -n "$OFFSITE_DESTINATION" ]; then
        marker="${backup}.offsite-verified"
        if [ ! -f "$marker" ]; then
            backup_offsite_status=missing
            backup_health_failure backup_offsite_missing \
                "$backup_name has no exact offsite proof marker"
            return
        fi
        if ! root_owned_read_only_proof "$marker"; then
            backup_offsite_status=untrusted
            backup_health_failure backup_offsite_untrusted \
                "$backup_name has an untrusted offsite proof marker"
            return
        fi
        marker_schema="$(awk -F= '$1 == "schema" {print $2; exit}' "$marker")"
        marker_sha="$(awk -F= '$1 == "sha256" {print $2; exit}' "$marker")"
        marker_bytes="$(awk -F= '$1 == "bytes" {print $2; exit}' "$marker")"
        marker_destination="$(awk -F= '$1 == "destination" {print $2; exit}' "$marker")"
        marker_layout="$(awk -F= '$1 == "layout" {print $2; exit}' "$marker")"
        [ -n "$marker_layout" ] || marker_layout=class-prefix
        expected_destination="$(expected_backup_destination "$backup" "$marker_layout" || true)"
        if [ "$marker_schema" != "1" ] || [ "$marker_sha" != "$checksum_sha" ] ||
            [ "$marker_bytes" != "$backup_snapshot_bytes" ] ||
            [ -z "$expected_destination" ] ||
            [ "$marker_destination" != "$expected_destination" ]; then
            backup_offsite_status=invalid
            backup_health_failure backup_offsite_invalid \
                "$backup_name offsite proof does not match stat/checksum/destination metadata"
            return
        fi
        backup_offsite_status=ok
    else
        backup_offsite_status=not_required
    fi

    if [ "$backup_age_minutes" -ge "$BACKUP_HARD_AGE_MINUTES" ]; then
        backup_health_failure backup_hourly_stale \
            "$backup_name is ${backup_age_minutes} minutes old; hard limit is ${BACKUP_HARD_AGE_MINUTES}"
    elif [ "$backup_age_minutes" -ge "$BACKUP_WARN_AGE_MINUTES" ]; then
        [ "$backup_status" = "fail" ] || backup_status=warn
        record_warning backup_hourly_aging \
            "$backup_name is ${backup_age_minutes} minutes old; warning limit is ${BACKUP_WARN_AGE_MINUTES}"
    elif [ "$backup_writer_active" = "1" ]; then
        [ "$backup_status" = "fail" ] || backup_status=in_progress
    else
        [ "$backup_status" = "fail" ] || backup_status=ok
    fi
}

inspect_daily_backup() {
    local now_epoch="$1" daily mtime
    [ "$BACKUP_DAILY_HEALTH_REQUIRED" = "1" ] || {
        backup_daily_status=disabled
        return
    }
    if [ "$backup_writer_active" = "1" ]; then
        daily="$(newest_complete_backup_in "$BACKUP_ROOT/daily" || true)"
    else
        daily="$(newest_active_backup_in "$BACKUP_ROOT/daily" || true)"
    fi
    if [ -z "$daily" ] || ! backup_metadata_complete "$daily"; then
        backup_daily_status=fail
        backup_health_failure backup_daily_missing "no complete daily database snapshot exists"
        return
    fi
    backup_daily_name="$(basename "$daily")"
    mtime="$(file_mtime_epoch "$daily")"
    if ! is_uint "$mtime" || [ "$mtime" -gt $((now_epoch + 300)) ]; then
        backup_daily_status=fail
        backup_health_failure backup_daily_timestamp_invalid \
            "daily database snapshot has an invalid timestamp"
        return
    fi
    backup_daily_age_minutes=$(((now_epoch - mtime) / 60))
    if [ "$backup_daily_age_minutes" -ge "$BACKUP_DAILY_HARD_AGE_MINUTES" ]; then
        backup_daily_status=fail
        backup_health_failure backup_daily_stale \
            "$backup_daily_name is ${backup_daily_age_minutes} minutes old"
    elif [ "$backup_daily_age_minutes" -ge "$BACKUP_DAILY_WARN_AGE_MINUTES" ]; then
        backup_daily_status=warn
        record_warning backup_daily_aging \
            "$backup_daily_name is ${backup_daily_age_minutes} minutes old"
    else
        backup_daily_status=ok
    fi
}

inspect_backup_health() {
    local now_epoch="$1" backup
    if [ "$BACKUP_HEALTH_REQUIRED" != "1" ]; then
        backup_status=disabled
        backup_checksum_status=not_checked
        backup_offsite_status=not_checked
        backup_daily_status=disabled
        return
    fi
    inspect_backup_writer "$now_epoch"
    inspect_backup_run_status
    # A writer owns the same canonical lock while snapshot payloads, sidecars,
    # and remote proof are being finalized. Never traverse that mutable tree
    # without the shared reader lock: even "complete-looking" files can be a
    # cross-process observation assembled from different instants.
    if [ "$backup_reader_lock_held" != "1" ]; then
        backup_hot_bytes=0
        backup_unverified_count=0
        backup_checksum_status=pending
        backup_offsite_status=pending
        backup_daily_status=pending
        if [ "$backup_status" != "fail" ]; then
            backup_status=in_progress
        fi
        return
    fi
    backup_hot_bytes="$(dir_bytes "$BACKUP_ROOT/hourly")"
    backup_hot_bytes=$((backup_hot_bytes + $(dir_bytes "$BACKUP_ROOT/daily") + \
        $(dir_bytes "$BACKUP_ROOT/.staging")))
    if [ "$BACKUP_LOCAL_MAX_BYTES" -gt 0 ] &&
        [ "$backup_hot_bytes" -gt "$BACKUP_LOCAL_MAX_BYTES" ]; then
        backup_health_failure backup_hot_cap_exceeded \
            "database hot storage exceeds BLUEY_BACKUP_LOCAL_MAX_BYTES"
    fi
    inspect_unverified_backlog
    if [ "$backup_writer_active" = "1" ]; then
        backup="$(newest_complete_backup_in "$BACKUP_ROOT/hourly" || true)"
    else
        backup="$(newest_active_backup_in "$BACKUP_ROOT/hourly" || true)"
    fi
    if [ -z "$backup" ]; then
        if [ "$backup_writer_active" = "1" ] &&
            [ "$backup_writer_age_minutes" -lt "$BACKUP_WRITER_HARD_AGE_MINUTES" ]; then
            backup_status=in_progress
            backup_checksum_status=pending
            backup_offsite_status=pending
        else
            backup_checksum_status=missing
            backup_offsite_status=missing
            backup_health_failure backup_hourly_missing \
                "no active $DB_BACKEND hourly database snapshot exists"
        fi
    else
        inspect_one_hourly_backup "$now_epoch" "$backup"
    fi
    inspect_daily_backup "$now_epoch"
    if [ "$backup_reader_lock_held" = "1" ]; then
        flock -u 7 || true
        backup_reader_lock_held=0
    fi
}

report_backup_health() {
    section "database backup freshness"
    printf 'backup_status=%s backup_name=%s age_minutes=%s bytes=%s checksum=%s offsite=%s writer_active=%s writer_age_minutes=%s run_status=%s unverified_count=%s hot_bytes=%s daily_status=%s daily_name=%s daily_age_minutes=%s\n' \
        "$backup_status" "$backup_name" "$backup_age_minutes" "$backup_snapshot_bytes" \
        "$backup_checksum_status" "$backup_offsite_status" "$backup_writer_active" \
        "$backup_writer_age_minutes" "$backup_run_status" "$backup_unverified_count" \
        "$backup_hot_bytes" "$backup_daily_status" "$backup_daily_name" \
        "$backup_daily_age_minutes"
}

log_archive_health_failure() {
    log_archive_status=fail
    record_failure "$@"
}

inspect_log_archive_health() {
    local now_epoch="$1" schema run_state started updated last_success exit_code
    if [ "$LOG_ARCHIVE_HEALTH_REQUIRED" != "1" ]; then
        log_archive_status=disabled
        log_archive_run_status=disabled
        return
    fi
    if [ ! -r "$LOG_ARCHIVE_STATUS_FILE" ]; then
        log_archive_run_status=missing
        log_archive_health_failure log_archive_status_missing \
            "durable log archive run status is missing"
        return
    fi
    if ! trusted_regular_file "$LOG_ARCHIVE_STATUS_FILE"; then
        log_archive_run_status=untrusted
        log_archive_health_failure log_archive_status_untrusted \
            "durable log archive run status is not a trusted regular file"
        return
    fi
    schema="$(awk -F= '$1 == "schema" {print $2; exit}' "$LOG_ARCHIVE_STATUS_FILE")"
    run_state="$(awk -F= '$1 == "status" {print $2; exit}' "$LOG_ARCHIVE_STATUS_FILE")"
    started="$(awk -F= '$1 == "started_at_epoch" {print $2; exit}' "$LOG_ARCHIVE_STATUS_FILE")"
    updated="$(awk -F= '$1 == "updated_at_epoch" {print $2; exit}' "$LOG_ARCHIVE_STATUS_FILE")"
    last_success="$(awk -F= '$1 == "last_success_epoch" {print $2; exit}' "$LOG_ARCHIVE_STATUS_FILE")"
    exit_code="$(awk -F= '$1 == "exit_code" {print $2; exit}' "$LOG_ARCHIVE_STATUS_FILE")"
    log_archive_name="$(awk -F= '$1 == "archive" {print $2; exit}' "$LOG_ARCHIVE_STATUS_FILE")"
    [ -n "$log_archive_name" ] || log_archive_name=none
    log_archive_run_status="$run_state"
    if [ "$schema" != "1" ] || ! is_uint "$started" || ! is_uint "$updated" ||
        ! is_uint "$last_success" || ! is_uint "$exit_code" ||
        [ "$updated" -gt $((now_epoch + 300)) ] ||
        { [ "$last_success" -gt 0 ] && [ "$last_success" -gt $((now_epoch + 300)) ]; }; then
        log_archive_health_failure log_archive_status_invalid \
            "durable log archive run status is malformed"
        return
    fi
    if [ "$last_success" -gt 0 ]; then
        log_archive_age_minutes=$(((now_epoch - last_success) / 60))
    fi
    case "$run_state" in
        ok)
            [ "$exit_code" = "0" ] ||
                log_archive_health_failure log_archive_status_invalid \
                    "successful log archive status has a nonzero exit code"
            ;;
        fail)
            log_archive_health_failure log_archive_run_failed \
                "the latest log archive run failed with exit code $exit_code"
            ;;
        running)
            if [ "$started" -gt $((now_epoch + 300)) ]; then
                log_archive_health_failure log_archive_status_invalid \
                    "log archive writer start time is invalid"
            else
                log_archive_writer_age_minutes=$(((now_epoch - started) / 60))
                if [ "$log_archive_writer_age_minutes" -ge \
                    "$LOG_ARCHIVE_WRITER_HARD_AGE_MINUTES" ]; then
                    log_archive_health_failure log_archive_writer_stuck \
                        "log archive writer is ${log_archive_writer_age_minutes} minutes old"
                elif [ "$log_archive_writer_age_minutes" -ge \
                    "$LOG_ARCHIVE_WRITER_WARN_AGE_MINUTES" ]; then
                    log_archive_status=warn
                    record_warning log_archive_writer_slow \
                        "log archive writer is ${log_archive_writer_age_minutes} minutes old"
                fi
            fi
            ;;
        *)
            log_archive_health_failure log_archive_status_invalid \
                "durable log archive run status has an unknown state"
            ;;
    esac
    if [ "$last_success" -eq 0 ]; then
        log_archive_health_failure log_archive_never_succeeded \
            "no successful offhost log archive is recorded"
    elif [ "$log_archive_age_minutes" -ge "$LOG_ARCHIVE_HARD_AGE_MINUTES" ]; then
        log_archive_health_failure log_archive_stale \
            "last successful log archive is ${log_archive_age_minutes} minutes old"
    elif [ "$log_archive_age_minutes" -ge "$LOG_ARCHIVE_WARN_AGE_MINUTES" ]; then
        [ "$log_archive_status" = "fail" ] || log_archive_status=warn
        record_warning log_archive_aging \
            "last successful log archive is ${log_archive_age_minutes} minutes old"
    elif [ "$log_archive_status" = "unknown" ]; then
        log_archive_status=ok
    fi
}

report_log_archive_health() {
    section "log archive freshness"
    printf 'log_archive_status=%s run_status=%s archive=%s age_minutes=%s writer_age_minutes=%s\n' \
        "$log_archive_status" "$log_archive_run_status" "$log_archive_name" \
        "$log_archive_age_minutes" "$log_archive_writer_age_minutes"
}

component_size() {
    local name="$1" path="$2" bytes
    bytes="$(dir_bytes "$path")"
    printf 'component=%s bytes=%s path=%s\n' "$name" "$bytes" "$path"
}

report_components() {
    section "Bluey capacity components"
    component_size api_root "$API_ROOT"
    component_size backups "$BACKUP_ROOT"
    component_size api_logs "$LOG_ROOT"
    component_size log_archive_work "$LOG_WORK_ROOT"
    component_size web_releases "$WEB_ROOT/releases"
    component_size release_sources "$BUILD_SCAN_ROOT/bluey-releases"
    local path
    for path in "$BUILD_SCAN_ROOT"/bluey-build-*; do
        [ -e "$path" ] || continue
        component_size stale_build_candidate "$path"
    done
}

capture_disk() {
    local disk_line inode_line
    disk_line="$(df -Pk "$ROOT_PATH" | awk 'NR == 2 {print}')"
    # Filesystem paths may contain spaces, but the first five POSIX df fields
    # remain fixed and are the only values used here.
    set -- $disk_line
    free_kb="${4:-0}"
    used_pct="${5:-100}"
    used_pct="${used_pct%%%}"
    is_uint "$used_pct" || used_pct=100
    is_uint "$free_kb" || free_kb=0
    inode_line="$(df -Pi "$ROOT_PATH" | awk 'NR == 2 {print}')"
    set -- $inode_line
    inode_used_pct="${5:-100}"
    inode_used_pct="${inode_used_pct%%%}"
    is_uint "$inode_used_pct" || inode_used_pct=100
}

report_disk() {
    section "$1"
    df -h "$ROOT_PATH"
    printf 'root_used_pct=%s root_free_kb=%s inode_used_pct=%s\n' \
        "$used_pct" "$free_kb" "$inode_used_pct"
}

evaluate_disk() {
    local min_free_kb warn_min_free_kb
    min_free_kb=$((MIN_FREE_GB * 1024 * 1024))
    warn_min_free_kb=$((WARN_MIN_FREE_GB * 1024 * 1024))
    if [ "$used_pct" -ge "$MAX_USED_PCT" ]; then
        record_failure disk_used_hard \
            "$ROOT_PATH is ${used_pct}% used; threshold is ${MAX_USED_PCT}%"
    elif [ "$used_pct" -ge "$WARN_USED_PCT" ]; then
        record_warning disk_used_warn \
            "$ROOT_PATH is ${used_pct}% used; early threshold is ${WARN_USED_PCT}%"
    fi
    if [ "$free_kb" -lt "$min_free_kb" ]; then
        record_failure disk_free_hard "$ROOT_PATH has less than ${MIN_FREE_GB}G free"
    elif [ "$free_kb" -lt "$warn_min_free_kb" ]; then
        record_warning disk_free_warn \
            "$ROOT_PATH has less than ${WARN_MIN_FREE_GB}G early-warning reserve"
    fi
    if [ "$inode_used_pct" -ge "$MAX_INODE_USED_PCT" ]; then
        record_failure inode_used_hard \
            "$ROOT_PATH inode use is ${inode_used_pct}%; threshold is ${MAX_INODE_USED_PCT}%"
    elif [ "$inode_used_pct" -ge "$WARN_INODE_USED_PCT" ]; then
        record_warning inode_used_warn \
            "$ROOT_PATH inode use is ${inode_used_pct}%; early threshold is ${WARN_INODE_USED_PCT}%"
    fi
}

read_previous_status() {
    [ -r "$STATUS_FILE" ] || return 0
    previous_status="$(awk -F= '$1 == "status" {print $2; exit}' "$STATUS_FILE")"
    previous_last_alert_epoch="$(awk -F= '$1 == "last_alert_epoch" {print $2; exit}' "$STATUS_FILE")"
    previous_fingerprint="$(awk -F= '$1 == "fingerprint" {print $2; exit}' "$STATUS_FILE")"
    [ -n "$previous_fingerprint" ] || previous_fingerprint=unknown
    is_uint "$previous_last_alert_epoch" || previous_last_alert_epoch=0
    last_alert_epoch="$previous_last_alert_epoch"
}

json_escape() {
    printf '%s' "$1" | sed 's/\\/\\\\/g; s/"/\\"/g'
}

send_alert() {
    local state="$1" now_epoch="$2" host payload escaped_webhook
    [ -n "$ALERT_WEBHOOK_URL" ] || return 1
    host="$(hostname -s 2>/dev/null || hostname || echo unknown-host)"
    payload="{\"event\":\"bluey_disk_guard\",\"state\":\"$(json_escape "$state")\",\"reasonCodes\":\"$(json_escape "$reason_codes")\",\"host\":\"$(json_escape "$host")\",\"usedPct\":$used_pct,\"freeKb\":$free_kb,\"inodeUsedPct\":$inode_used_pct,\"maxUsedPct\":$MAX_USED_PCT,\"minFreeGb\":$MIN_FREE_GB,\"backupStatus\":\"$backup_status\",\"backupAgeMinutes\":$backup_age_minutes,\"backupWriterActive\":$backup_writer_active,\"backupUnverifiedCount\":$backup_unverified_count,\"logArchiveStatus\":\"$log_archive_status\",\"logArchiveAgeMinutes\":$log_archive_age_minutes,\"observedAt\":\"$(date -u +%FT%TZ)\"}"
    escaped_webhook="$(printf '%s' "$ALERT_WEBHOOK_URL" | sed 's/\\/\\\\/g; s/"/\\"/g')"
    if curl --fail --silent --show-error --max-time 10 \
        -H 'Content-Type: application/json' --data "$payload" --config - \
        >/dev/null <<EOF
url = "$escaped_webhook"
EOF
    then
        last_alert_epoch="$now_epoch"
        return 0
    fi
    echo "warn: disk guard alert delivery failed" >&2
    return 1
}

write_status() {
    local state="$1" now_epoch="$2" fingerprint="$3" status_dir status_tmp
    status_dir="$(dirname "$STATUS_FILE")"
    if ! mkdir -p "$status_dir"; then
        return 1
    fi
    status_tmp="$(mktemp "${STATUS_FILE}.tmp.XXXXXX")" || return 1
    {
        printf 'schema=1\n'
        printf 'status=%s\n' "$state"
        printf 'fingerprint=%s\n' "$fingerprint"
        printf 'reason_codes=%s\n' "$reason_codes"
        printf 'observed_at=%s\n' "$(date -u +%FT%TZ)"
        printf 'observed_at_epoch=%s\n' "$now_epoch"
        printf 'last_alert_epoch=%s\n' "$last_alert_epoch"
        printf 'used_pct=%s\n' "$used_pct"
        printf 'free_kb=%s\n' "$free_kb"
        printf 'inode_used_pct=%s\n' "$inode_used_pct"
        printf 'max_used_pct=%s\n' "$MAX_USED_PCT"
        printf 'min_free_gb=%s\n' "$MIN_FREE_GB"
        printf 'warn_used_pct=%s\n' "$WARN_USED_PCT"
        printf 'warn_min_free_gb=%s\n' "$WARN_MIN_FREE_GB"
        printf 'backup_status=%s\n' "$backup_status"
        printf 'backup_health_required=%s\n' "$BACKUP_HEALTH_REQUIRED"
        printf 'backup_name=%s\n' "$backup_name"
        printf 'backup_age_minutes=%s\n' "$backup_age_minutes"
        printf 'backup_snapshot_bytes=%s\n' "$backup_snapshot_bytes"
        printf 'backup_checksum_status=%s\n' "$backup_checksum_status"
        printf 'backup_offsite_status=%s\n' "$backup_offsite_status"
        printf 'backup_writer_active=%s\n' "$backup_writer_active"
        printf 'backup_writer_age_minutes=%s\n' "$backup_writer_age_minutes"
        printf 'backup_run_status=%s\n' "$backup_run_status"
        printf 'backup_unverified_count=%s\n' "$backup_unverified_count"
        printf 'backup_hot_bytes=%s\n' "$backup_hot_bytes"
        printf 'backup_daily_status=%s\n' "$backup_daily_status"
        printf 'backup_daily_name=%s\n' "$backup_daily_name"
        printf 'backup_daily_age_minutes=%s\n' "$backup_daily_age_minutes"
        printf 'log_archive_status=%s\n' "$log_archive_status"
        printf 'log_archive_run_status=%s\n' "$log_archive_run_status"
        printf 'log_archive_name=%s\n' "$log_archive_name"
        printf 'log_archive_age_minutes=%s\n' "$log_archive_age_minutes"
        printf 'log_archive_writer_age_minutes=%s\n' "$log_archive_writer_age_minutes"
        printf 'backup_warn_age_minutes=%s\n' "$BACKUP_WARN_AGE_MINUTES"
        printf 'backup_hard_age_minutes=%s\n' "$BACKUP_HARD_AGE_MINUTES"
        printf 'backup_bytes=%s\n' "$(dir_bytes "$BACKUP_ROOT")"
        printf 'log_work_bytes=%s\n' "$(dir_bytes "$LOG_WORK_ROOT")"
        printf 'release_bytes=%s\n' "$(dir_bytes "$WEB_ROOT/releases")"
    } > "$status_tmp"
    chmod 0640 "$status_tmp"
    if ! mv "$status_tmp" "$STATUS_FILE"; then
        rm -f "$status_tmp"
        return 1
    fi
}

command -v flock >/dev/null 2>&1 || {
    echo "fail: flock is required for the disk guard" >&2
    exit 1
}
trusted_directory_chain "$(dirname "$STATUS_FILE")" || {
    echo "fail: disk guard status directory chain is not trusted" >&2
    exit 1
}
mkdir -p "$(dirname "$STATUS_FILE")"
chmod 0700 "$(dirname "$STATUS_FILE")"
trusted_directory_chain "$(dirname "$STATUS_FILE")" || {
    echo "fail: disk guard status directory chain is not trusted" >&2
    exit 1
}
if [ "$BACKUP_HEALTH_REQUIRED" = "1" ] && [ -d "$BACKUP_ROOT" ]; then
    trusted_directory_chain "$BACKUP_ROOT" || {
        echo "fail: backup directory chain is not trusted" >&2
        exit 1
    }
fi
if [ "$LOG_ARCHIVE_HEALTH_REQUIRED" = "1" ]; then
    trusted_directory_chain "$(dirname "$LOG_ARCHIVE_STATUS_FILE")" || {
        echo "fail: log archive status directory chain is not trusted" >&2
        exit 1
    }
fi
if [ "$PRUNE" = "1" ]; then
    trusted_directory_chain "$TMP_PRUNE_ROOT" || {
        echo "fail: disk guard temp directory chain is not trusted" >&2
        exit 1
    }
    mkdir -p "$TMP_PRUNE_ROOT"
    chmod 0700 "$TMP_PRUNE_ROOT"
    trusted_directory_chain "$TMP_PRUNE_ROOT" || {
        echo "fail: disk guard temp directory chain is not trusted" >&2
        exit 1
    }
fi
trusted_regular_file "$GUARD_LOCK_FILE" || {
    echo "fail: disk guard lock is not a trusted regular file" >&2
    exit 1
}
exec 6>>"$GUARD_LOCK_FILE"
if ! flock -n 6; then
    echo "fail: another disk guard process is active" >&2
    exit 1
fi
chmod 0600 "$GUARD_LOCK_FILE"
status_basename="$(basename "$STATUS_FILE")"
if find "$(dirname "$STATUS_FILE")" -maxdepth 1 -type l \
    -name "${status_basename}.tmp.*" -print -quit | grep -q .; then
    echo "fail: refusing symlinked disk guard status temp" >&2
    exit 1
fi
find "$(dirname "$STATUS_FILE")" -maxdepth 1 -type f \
    -name "${status_basename}.tmp.*" -print -delete 2>/dev/null || true

read_previous_status
capture_disk
report_disk "disk before prune"
report_components

if [ "$PRUNE" = "1" ]; then
    section "safe prune"
    find "$TMP_PRUNE_ROOT" -maxdepth 1 -type f -name 'bluey-*' -mtime +2 -print -delete \
        2>/dev/null || true
    if command -v journalctl >/dev/null 2>&1; then
        journalctl --vacuum-time="$JOURNAL_VACUUM_TIME" || true
    fi
    if [ -x "$LOG_ARCHIVE_SCRIPT" ]; then
        "$LOG_ARCHIVE_SCRIPT" --prune-only || true
    fi
    capture_disk
    report_disk "disk after prune"
    report_components
fi

# In prune mode only the post-prune snapshot is authoritative. This closes the
# old behavior where a recovered host still exited non-zero from stale values.
evaluate_disk
now_epoch="$(date -u +%s)"
inspect_backup_health "$now_epoch"
report_backup_health
inspect_log_archive_health "$now_epoch"
report_log_archive_health

section "journal"
journalctl --disk-usage 2>/dev/null || true

section "recent storage errors"
journalctl -u bluey-api -u bluey-jobs-api -u caddy --since '24 hours ago' --no-pager \
    2>/dev/null |
    grep -Ei 'database or disk is full|no space|AccessDenied|SignatureDoesNotMatch|backup failed|failed backup|backup upload|object delete|export failed|panic|log archive failed|archive failed|failed archive' |
    tail -80 || true

state=ok
if [ "$failures" -gt 0 ]; then
    state=fail
elif [ "$warnings" -gt 0 ]; then
    state=warn
fi
fingerprint="$state:$reason_codes"

should_alert=0
alert_state="$state"
if { [ "$state" = "fail" ] || [ "$state" = "warn" ]; } &&
    [ -n "$ALERT_WEBHOOK_URL" ]; then
    repeat_seconds=$((ALERT_REPEAT_MINUTES * 60))
    if [ "$previous_status" != "$state" ] || [ "$previous_fingerprint" != "$fingerprint" ] ||
        [ $((now_epoch - previous_last_alert_epoch)) -ge "$repeat_seconds" ]; then
        should_alert=1
    fi
elif [ "$state" = "ok" ] &&
    { [ "$previous_status" = "fail" ] || [ "$previous_status" = "warn" ]; } &&
    [ -n "$ALERT_WEBHOOK_URL" ]; then
    should_alert=1
    alert_state=recovered
fi

if [ "$should_alert" = "1" ]; then
    if ! send_alert "$alert_state" "$now_epoch" && [ "$REQUIRE_ALERT" = "1" ]; then
        record_failure alert_delivery_failed "required disk guard alert delivery failed"
        state=fail
        fingerprint="$state:$reason_codes"
    fi
fi

if ! write_status "$state" "$now_epoch" "$fingerprint"; then
    echo "warn: failed to write durable disk guard status: $STATUS_FILE" >&2
    if [ "$REQUIRE_STATUS" = "1" ]; then
        failures=$((failures + 1))
        state=fail
    fi
fi

if [ "$failures" -gt 0 ]; then
    exit 1
fi

if [ "$warnings" -gt 0 ]; then
    echo "warn: disk guard early-warning threshold reached"
else
    echo "ok: disk guard passed"
fi
