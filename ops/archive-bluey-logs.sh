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
    local path="$1" current owner mode mode_value
    current="$(cd -P -- "$(dirname "$path")" 2>/dev/null && pwd -P)" || return 1
    while :; do
        [ -d "$current" ] && [ ! -L "$current" ] || return 1
        owner="$(bootstrap_stat -c%u -f%u "$current")" || return 1
        { [ "$owner" = 0 ] || [ "$owner" = "$EUID" ]; } || return 1
        mode="$(bootstrap_stat -c%a -f%Lp "$current")" || return 1
        case "$mode" in ''|*[!0-9]*) return 1 ;; esac
        mode_value=$((8#$mode))
        if [ $((mode_value & 8#022)) -ne 0 ] &&
            ! { [ "$owner" = 0 ] && [ $((mode_value & 8#1000)) -ne 0 ]; }; then return 1; fi
        [ "$current" = / ] && break
        current="$(dirname "$current")"
    done
}

bootstrap_identity() { bootstrap_stat -c%i -f%i "$1"; }

load_env_file() {
    local env_file="$1" path_identity fd_identity env_fd
    if [ -e "$env_file" ] || [ -L "$env_file" ]; then
        [ -r "$env_file" ] && bootstrap_trusted_parent_chain "$env_file" && bootstrap_trusted_file "$env_file" || {
            echo "log archive failed: environment file is not trusted and write-protected" >&2
            exit 1
        }
        path_identity="$(bootstrap_identity "$env_file")" || exit 1
        exec 9<"$env_file"
        env_fd=9
        fd_identity="$(bootstrap_identity "/dev/fd/$env_fd")" || exit 1
        [ "$path_identity" = "$fd_identity" ] || exit 1
        # shellcheck disable=SC1090
        . "/dev/fd/$env_fd"
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
unset -f bootstrap_stat bootstrap_trusted_file bootstrap_trusted_parent_chain bootstrap_identity

ARCHIVE_ROOT="${BLUEY_LOG_ARCHIVE_LOCAL_DIR-/var/backups/bluey-api/logs}"
WORK_ROOT="${BLUEY_LOG_ARCHIVE_WORK_DIR-/var/lib/bluey-ops/log-archive}"
OPS_LOG_ROOT="${BLUEY_OPS_LOG_DIR-/var/log/bluey-ops}"
while [ "$WORK_ROOT" != "/" ] && [ "${WORK_ROOT%/}" != "$WORK_ROOT" ]; do
    WORK_ROOT="${WORK_ROOT%/}"
done
while [ "$ARCHIVE_ROOT" != "/" ] && [ "${ARCHIVE_ROOT%/}" != "$ARCHIVE_ROOT" ]; do
    ARCHIVE_ROOT="${ARCHIVE_ROOT%/}"
done
LOG_DIRS="${BLUEY_LOG_DIRS-/var/log/bluey-api /opt/bluey-api/logs $OPS_LOG_ROOT}"
SERVICES="${BLUEY_LOG_ARCHIVE_SERVICES:-bluey-api caddy}"
SINCE="${BLUEY_LOG_ARCHIVE_SINCE:-24 hours ago}"
ARCHIVE_LOCAL_RETENTION_DAYS="${BLUEY_LOG_ARCHIVE_LOCAL_RETENTION_DAYS:-7}"
LOG_ROOT_MAX_BYTES="${BLUEY_LOG_ROOT_MAX_BYTES:-2147483648}"
LOG_DIR_MAX_BYTES="${BLUEY_LOG_DIR_MAX_BYTES:-536870912}"
ARCHIVE_MAX_FILES="${BLUEY_LOG_ARCHIVE_MAX_FILES:-160}"
BUNDLE_MAX_BYTES="${BLUEY_LOG_ARCHIVE_BUNDLE_MAX_BYTES:-134217728}"
MIN_FREE_GB="${BLUEY_LOG_ARCHIVE_MIN_FREE_GB:-16}"
WORK_RETENTION_MINUTES="${BLUEY_LOG_WORK_RETENTION_MINUTES:-1440}"
WORK_MAX_DIRS="${BLUEY_LOG_WORK_MAX_DIRS:-4}"
WORK_ROOT_MAX_BYTES="${BLUEY_LOG_WORK_ROOT_MAX_BYTES:-268435456}"
LOCK_WAIT_SECONDS="${BLUEY_LOG_ARCHIVE_LOCK_WAIT_SECONDS:-0}"
REQUIRE_OFFHOST="${BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST:-0}"
REQUIRE_PROVIDER_RETENTION="${BLUEY_LOG_ARCHIVE_REQUIRE_PROVIDER_RETENTION:-0}"
REQUIRE_TRUSTED_PATHS="${BLUEY_LOG_ARCHIVE_REQUIRE_TRUSTED_PATHS:-1}"
STATUS_ROOT="${BLUEY_OPS_STATE_ROOT-/var/lib/bluey-ops}"
STATUS_FILE="${BLUEY_LOG_ARCHIVE_STATUS_FILE-$STATUS_ROOT/log-archive.status}"
REQUIRE_STATUS="${BLUEY_LOG_ARCHIVE_REQUIRE_STATUS:-1}"
STORAGE_PREFIX="${BLUEY_LOG_STORAGE_PREFIX:-prod}"
DIAGNOSTIC_RETENTION_DAYS="${BLUEY_UPLOAD_LOG_RETENTION_DAYS:-${BLUEY_LOG_RETENTION_DAYS:-180}}"
DB_CONNECT_TIMEOUT_SECONDS="${BLUEY_LOG_ARCHIVE_DB_CONNECT_TIMEOUT_SECONDS:-5}"
DB_STATEMENT_TIMEOUT_MS="${BLUEY_LOG_ARCHIVE_DB_STATEMENT_TIMEOUT_MS:-10000}"
DB_WALL_TIMEOUT_SECONDS="${BLUEY_LOG_ARCHIVE_DB_WALL_TIMEOUT_SECONDS:-20}"
COMMAND_TIMEOUT_SECONDS="${BLUEY_LOG_ARCHIVE_COMMAND_TIMEOUT_SECONDS:-300}"
REMOTE_TIMEOUT_SECONDS="${BLUEY_LOG_ARCHIVE_REMOTE_TIMEOUT_SECONDS:-300}"
TIMEOUT_KILL_GRACE_SECONDS="${BLUEY_LOG_ARCHIVE_TIMEOUT_KILL_GRACE_SECONDS:-5}"
UPLOADED_STORAGE=""
UPLOADED_OBJECT_KEY=""
UPLOADED_LOCAL_PATH=""
PRUNE_ONLY=0
VALIDATE_ONLY=0
CURRENT_BUNDLE=""
CURRENT_ARCHIVE_TMP=""
ARCHIVE_RUN_ACTIVE=0
RUN_STARTED_EPOCH=0
RUN_ARCHIVE_NAME=none
OPS_LOG_R2_ACCESS_KEY_ID="${BLUEY_OPS_LOG_R2_ACCESS_KEY_ID:-}"
OPS_LOG_R2_SECRET_ACCESS_KEY="${BLUEY_OPS_LOG_R2_SECRET_ACCESS_KEY:-}"
OPS_LOG_R2_SESSION_TOKEN="${BLUEY_OPS_LOG_R2_SESSION_TOKEN:-}"
OPS_LOG_R2_REGION="${BLUEY_OPS_LOG_R2_REGION:-auto}"
while IFS= read -r inherited_name; do
    case "$inherited_name" in
        *KEY*|*TOKEN*|*SECRET*|*PASSWORD*|*DATABASE_URL*|*DSN*|*WEBHOOK*|*CREDENTIAL*|*COOKIE*|*AUTH*)
            export -n "$inherited_name" 2>/dev/null || true
            ;;
    esac
done < <(compgen -e)
unset inherited_name

case "${1:-}" in
    --prune-only)
        PRUNE_ONLY=1
        ;;
    --check-config)
        VALIDATE_ONLY=1
        ;;
    "" )
        ;;
    * )
        echo "usage: $0 [--prune-only|--check-config]" >&2
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

is_placeholder() {
    case "$1" in
        ""|*replace-with*|*.invalid*) return 0 ;;
        *) return 1 ;;
    esac
}

die() {
    echo "log archive failed: $*" >&2
    exit 1
}

validate_safe_directory() {
    local name="$1" path="$2" minimum_components="$3"
    local remainder component component_count=0 canonical
    [ -n "$path" ] || die "$name must not be empty"
    case "$path" in
        /*) ;;
        *) die "$name must be an absolute path" ;;
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
            .|..) die "$name contains an unsafe path segment" ;;
            *) component_count=$((component_count + 1)) ;;
        esac
    done
    [ "$component_count" -ge "$minimum_components" ] ||
        die "$name is an unsafe broad filesystem root"
    [ ! -L "$path" ] || die "$name must not be a symlink"
    if [ -d "$path" ]; then
        canonical="$(cd -P -- "$path" 2>/dev/null && pwd -P)" ||
            die "$name cannot be resolved"
        [ -n "${canonical//\//}" ] || die "$name resolves to the filesystem root"
    fi
}

file_owner_uid() {
    if stat -c%u "$1" >/dev/null 2>&1; then stat -c%u "$1"; else stat -f%u "$1"; fi
}

file_mode() {
    if stat -c%a "$1" >/dev/null 2>&1; then stat -c%a "$1"; else stat -f%Lp "$1"; fi
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

for setting in \
    "$ARCHIVE_LOCAL_RETENTION_DAYS" \
    "$LOG_ROOT_MAX_BYTES" \
    "$LOG_DIR_MAX_BYTES" \
    "$ARCHIVE_MAX_FILES" \
    "$BUNDLE_MAX_BYTES" \
    "$MIN_FREE_GB" \
    "$WORK_RETENTION_MINUTES" \
    "$WORK_MAX_DIRS" \
    "$WORK_ROOT_MAX_BYTES" \
    "$DB_CONNECT_TIMEOUT_SECONDS" \
    "$DB_STATEMENT_TIMEOUT_MS" \
    "$DB_WALL_TIMEOUT_SECONDS" \
    "$COMMAND_TIMEOUT_SECONDS" \
    "$REMOTE_TIMEOUT_SECONDS"; do
    is_uint "$setting" || die "retention and capacity settings must be unsigned integers"
done
[ "$TIMEOUT_KILL_GRACE_SECONDS" -gt 0 ] 2>/dev/null || die "archive timeout kill grace must be positive"
[ "$COMMAND_TIMEOUT_SECONDS" -gt 0 ] && [ "$REMOTE_TIMEOUT_SECONDS" -gt 0 ] ||
    die "archive command timeouts must be positive"

timeout_binary() { command -v timeout 2>/dev/null || command -v gtimeout 2>/dev/null || true; }
run_bounded() {
    local seconds="$1"; shift
    local timeout_bin="$(timeout_binary)"
    [ -n "$timeout_bin" ] || die "timeout utility is required for bounded operations"
    "$timeout_bin" --signal=TERM --kill-after="$TIMEOUT_KILL_GRACE_SECONDS" "$seconds" "$@"
}
[ "$BUNDLE_MAX_BYTES" -gt 0 ] || die "BLUEY_LOG_ARCHIVE_BUNDLE_MAX_BYTES must be positive"
[ "$LOG_DIR_MAX_BYTES" -gt 0 ] || die "BLUEY_LOG_DIR_MAX_BYTES must be positive"
is_uint "$LOCK_WAIT_SECONDS" || die "BLUEY_LOG_ARCHIVE_LOCK_WAIT_SECONDS must be unsigned"
case "$REQUIRE_OFFHOST" in
    0|1) ;;
    *) die "BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST must be 0 or 1" ;;
esac
case "$REQUIRE_PROVIDER_RETENTION" in 0|1) ;; *) die "BLUEY_LOG_ARCHIVE_REQUIRE_PROVIDER_RETENTION must be 0 or 1" ;; esac
case "$REQUIRE_TRUSTED_PATHS" in
    0|1) ;;
    *) die "BLUEY_LOG_ARCHIVE_REQUIRE_TRUSTED_PATHS must be 0 or 1" ;;
esac
case "$REQUIRE_STATUS" in
    0|1) ;;
    *) die "BLUEY_LOG_ARCHIVE_REQUIRE_STATUS must be 0 or 1" ;;
esac
validate_safe_directory BLUEY_LOG_ARCHIVE_LOCAL_DIR "$ARCHIVE_ROOT" 2
validate_safe_directory BLUEY_LOG_ARCHIVE_WORK_DIR "$WORK_ROOT" 2
validate_safe_directory BLUEY_OPS_STATE_ROOT "$STATUS_ROOT" 2
validate_safe_directory BLUEY_LOG_ARCHIVE_STATUS_ROOT "$(dirname "$STATUS_FILE")" 2
validate_safe_directory BLUEY_LOG_ARCHIVE_STATUS_FILE "$STATUS_FILE" 3
case "$STATUS_FILE" in
    "$STATUS_ROOT"/*) ;;
    *) die "BLUEY_LOG_ARCHIVE_STATUS_FILE must be inside BLUEY_OPS_STATE_ROOT" ;;
esac
[ "$(dirname "$STATUS_FILE")" = "$STATUS_ROOT" ] ||
    die "BLUEY_LOG_ARCHIVE_STATUS_FILE must be directly inside BLUEY_OPS_STATE_ROOT"
for log_dir in $LOG_DIRS; do
    validate_safe_directory BLUEY_LOG_DIRS "$log_dir" 3
done
configured_log_destination="${BLUEY_OPS_LOG_ARCHIVE_DESTINATION:-${BLUEY_LOG_ARCHIVE_DESTINATION:-}}"
if [ -z "$configured_log_destination" ] && [ -n "${BLUEY_OPS_LOG_R2_BUCKET:-}" ]; then
    configured_log_destination="s3://${BLUEY_OPS_LOG_R2_BUCKET}"
fi
if [ "$REQUIRE_OFFHOST" = "1" ] && [ -z "$configured_log_destination" ]; then
    die "BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST=1 requires an offhost destination"
fi
case "$configured_log_destination" in
    s3://*)
        is_placeholder "$OPS_LOG_R2_ACCESS_KEY_ID" &&
            die "dedicated BLUEY_OPS_LOG_R2 access key is missing or a placeholder"
        is_placeholder "$OPS_LOG_R2_SECRET_ACCESS_KEY" &&
            die "dedicated BLUEY_OPS_LOG_R2 secret is missing or a placeholder"
        ;;
esac
if [ "$VALIDATE_ONLY" = "1" ]; then
    echo "Bluey log archive configuration is safe."
    exit 0
fi
command -v flock >/dev/null 2>&1 || die "flock is required"
for trusted_root in "$WORK_ROOT" "$ARCHIVE_ROOT" "$STATUS_ROOT"; do
    trusted_directory_chain "$trusted_root" ||
        die "archive directory chain is not trusted and write-protected: $trusted_root"
done
if [ -L "$ARCHIVE_ROOT/.staging" ] ||
    { [ -e "$ARCHIVE_ROOT/.staging" ] && [ ! -d "$ARCHIVE_ROOT/.staging" ]; }; then
    die "archive staging root must be a directory and not a symlink"
fi
if [ -d "$ARCHIVE_ROOT/.staging" ]; then
    trusted_directory_chain "$ARCHIVE_ROOT/.staging" ||
        die "archive staging directory chain is not trusted and write-protected"
fi
mkdir -p "$WORK_ROOT" "$ARCHIVE_ROOT" "$STATUS_ROOT"
mkdir -p "$ARCHIVE_ROOT/.staging"
chmod 0700 "$WORK_ROOT"
chmod 0700 "$ARCHIVE_ROOT"
chmod 0700 "$STATUS_ROOT"
for trusted_root in "$WORK_ROOT" "$ARCHIVE_ROOT" "$STATUS_ROOT"; do
    trusted_directory_chain "$trusted_root" ||
        die "archive directory chain is not trusted and write-protected: $trusted_root"
done
trusted_directory_chain "$ARCHIVE_ROOT/.staging" ||
    die "archive staging directory chain is not trusted and write-protected"
if [ -L "$WORK_ROOT/.archive.lock" ] ||
    { [ -e "$WORK_ROOT/.archive.lock" ] && [ ! -f "$WORK_ROOT/.archive.lock" ]; }; then
    die "archive lock must be a regular non-symlink file"
fi
if [ -e "$WORK_ROOT/.archive.lock" ]; then
    lock_uid="$(file_owner_uid "$WORK_ROOT/.archive.lock")"
    lock_mode="$(file_mode "$WORK_ROOT/.archive.lock")"
    [ "$lock_uid" = "$(id -u)" ] || die "archive lock owner is untrusted"
    lock_mode_value=$((8#$lock_mode))
    [ $((lock_mode_value & 8#022)) -eq 0 ] || die "archive lock is group/world writable"
fi
exec 8>>"$WORK_ROOT/.archive.lock"
if [ "$LOCK_WAIT_SECONDS" -eq 0 ]; then
    flock -n 8 || die "another log archive process is active"
else
    flock -w "$LOCK_WAIT_SECONDS" 8 || die "timed out waiting for the log archive lock"
fi
touch "$WORK_ROOT/.archive.lock"
chmod 0600 "$WORK_ROOT/.archive.lock"

if [ -L "$STATUS_FILE" ] || { [ -e "$STATUS_FILE" ] && [ ! -f "$STATUS_FILE" ]; }; then
    die "log archive status must be a regular non-symlink file"
fi
if [ -e "$STATUS_FILE" ] && [ "$REQUIRE_TRUSTED_PATHS" = "1" ]; then
    status_uid="$(file_owner_uid "$STATUS_FILE")"
    status_mode="$(file_mode "$STATUS_FILE")"
    [ "$status_uid" = "$(id -u)" ] || die "log archive status owner is untrusted"
    status_mode_value=$((8#$status_mode))
    [ $((status_mode_value & 8#022)) -eq 0 ] ||
        die "log archive status is group/world writable"
fi
status_basename="$(basename "$STATUS_FILE")"
if find "$STATUS_ROOT" -maxdepth 1 -type l -name "${status_basename}.tmp.*" \
    -print -quit | grep -q .; then
    die "refusing symlinked log archive status temp"
fi
find "$STATUS_ROOT" -maxdepth 1 -type f -name "${status_basename}.tmp.*" \
    -print -delete 2>/dev/null || true

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

file_size_bytes() {
    if stat -c%s "$1" >/dev/null 2>&1; then
        stat -c%s "$1"
    else
        stat -f%z "$1"
    fi
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

sha256_stream() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum | awk '{print $1}'
    else
        shasum -a 256 | awk '{print $1}'
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

list_work_dirs_oldest() {
    if supports_find_printf "$WORK_ROOT"; then
        find "$WORK_ROOT" -mindepth 1 -maxdepth 1 -type d -name 'bluey-logs-*' \
            -printf '%T@ %p\n' 2>/dev/null | sort -n | cut -d' ' -f2-
    else
        find "$WORK_ROOT" -mindepth 1 -maxdepth 1 -type d -name 'bluey-logs-*' \
            -exec stat -f '%m %N' {} \; 2>/dev/null | sort -n | cut -d' ' -f2-
    fi
}

remove_work_dir() {
    local path="$1" force_current="${2:-0}"
    case "$path" in
        "$WORK_ROOT"/bluey-logs-*)
            if [ "$path" = "$CURRENT_BUNDLE" ] && [ "$force_current" != "1" ]; then
                return 0
            fi
            [ -d "$path" ] && [ ! -L "$path" ] && rm -rf -- "$path"
            ;;
        *)
            echo "warn: refused unexpected log archive work path: $path" >&2
            ;;
    esac
}

write_archive_status() {
    local state="$1" exit_code="$2" now_epoch last_success status_tmp
    now_epoch="$(date -u +%s)"
    last_success=0
    if [ -r "$STATUS_FILE" ]; then
        last_success="$(awk -F= '$1 == "last_success_epoch" {print $2; exit}' \
            "$STATUS_FILE")"
        is_uint "$last_success" || last_success=0
    fi
    if [ "$state" = "ok" ]; then
        last_success="$now_epoch"
    fi
    status_tmp="$(mktemp "${STATUS_FILE}.tmp.XXXXXX")" || return 1
    {
        printf 'schema=1\n'
        printf 'status=%s\n' "$state"
        printf 'started_at_epoch=%s\n' "$RUN_STARTED_EPOCH"
        printf 'updated_at_epoch=%s\n' "$now_epoch"
        printf 'last_success_epoch=%s\n' "$last_success"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'archive=%s\n' "$RUN_ARCHIVE_NAME"
    } > "$status_tmp" || return 1
    chmod 0600 "$status_tmp" || return 1
    mv "$status_tmp" "$STATUS_FILE"
}

cleanup_current_bundle() {
    local status=$?
    trap - EXIT
    set +e
    if [ -n "$CURRENT_BUNDLE" ]; then
        remove_work_dir "$CURRENT_BUNDLE" 1
    fi
    if [ -n "$CURRENT_ARCHIVE_TMP" ]; then
        case "$CURRENT_ARCHIVE_TMP" in
            "$ARCHIVE_ROOT/.staging"/bluey-logs-*.tar.gz.tmp.*)
                rm -f -- "$CURRENT_ARCHIVE_TMP" "${CURRENT_ARCHIVE_TMP}.sha256"
                ;;
        esac
    fi
    if [ "$ARCHIVE_RUN_ACTIVE" = "1" ]; then
        if [ "$status" -eq 0 ]; then
            write_archive_status ok 0 || [ "$REQUIRE_STATUS" = "0" ] || status=1
        else
            write_archive_status fail "$status" || true
        fi
    fi
    exit "$status"
}
trap cleanup_current_bundle EXIT

prune_stale_work() {
    mkdir -p "$WORK_ROOT"
    chmod 0700 "$WORK_ROOT"

    local path count total oldest
    while IFS= read -r path; do
        [ -n "$path" ] || continue
        remove_work_dir "$path"
    done < <(
        find "$WORK_ROOT" -mindepth 1 -maxdepth 1 -type d -name 'bluey-logs-*' \
            -mmin +"$WORK_RETENTION_MINUTES" -print 2>/dev/null
    )

    while :; do
        count="$(find "$WORK_ROOT" -mindepth 1 -maxdepth 1 -type d \
            -name 'bluey-logs-*' 2>/dev/null | wc -l | tr -d ' ')"
        total="$(dir_bytes "$WORK_ROOT")"
        if [ "$count" -le "$WORK_MAX_DIRS" ] && [ "$total" -le "$WORK_ROOT_MAX_BYTES" ]; then
            break
        fi
        oldest="$(list_work_dirs_oldest | head -1)"
        if [ -z "$oldest" ] || [ "$oldest" = "$CURRENT_BUNDLE" ]; then
            echo "warn: log work root remains above cap; no stale work directory is removable" >&2
            break
        fi
        remove_work_dir "$oldest"
    done
}

prune_archive_staging() {
    mkdir -p "$ARCHIVE_ROOT/.staging"
    chmod 0700 "$ARCHIVE_ROOT/.staging"
    # The host-wide archive lock proves every pre-existing exact temp prefix is
    # orphaned. Current-run paths do not exist until after this sweep.
    find "$ARCHIVE_ROOT/.staging" -mindepth 1 -maxdepth 1 -type f \
        \( -name 'bluey-logs-*.tar.gz.tmp.*' -o \
           -name 'bluey-logs-*.tar.gz.tmp.*.sha256' -o \
           -name 'bluey-logs-*.tar.gz.offsite-verified.tmp.*' \) -print -delete \
        2>/dev/null || true
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
    local archive
    while IFS= read -r archive; do
        [ -n "$archive" ] || continue
        remove_archive_pair "$archive"
    done < <(
        find "$ARCHIVE_ROOT" -maxdepth 1 -type f -name 'bluey-logs-*.tar.gz' \
            -mtime +"$ARCHIVE_LOCAL_RETENTION_DAYS" -print 2>/dev/null
    )

    if is_uint "$LOG_ROOT_MAX_BYTES"; then
        local total
        total="$(dir_bytes "$ARCHIVE_ROOT")"
        while [ "$total" -gt "$LOG_ROOT_MAX_BYTES" ]; do
            local oldest
            oldest="$(list_files_oldest "$ARCHIVE_ROOT" -name 'bluey-logs-*.tar.gz' | head -1)"
            if [ -z "$oldest" ]; then
                break
            fi
            remove_archive_pair "$oldest"
            total="$(dir_bytes "$ARCHIVE_ROOT")"
        done
    fi
}

destination_base() {
    local configured_destination="${BLUEY_OPS_LOG_ARCHIVE_DESTINATION:-${BLUEY_LOG_ARCHIVE_DESTINATION:-}}"
    if [ -n "$configured_destination" ]; then
        printf '%s' "${configured_destination%/}"
        return
    fi
    local bucket="${BLUEY_OPS_LOG_R2_BUCKET:-}"
    if [ -n "$bucket" ]; then
        printf 's3://%s/%s/logs/api' "$bucket" "${STORAGE_PREFIX#/}"
        return
    fi
}

aws_endpoint() {
    local endpoint="${BLUEY_OPS_LOG_R2_ENDPOINT_URL:-}"
    if [ -n "$endpoint" ]; then
        printf '%s' "$endpoint"
    fi
}

archive_aws() {
    local endpoint
    endpoint="$(aws_endpoint)"
    (
        export AWS_ACCESS_KEY_ID="$OPS_LOG_R2_ACCESS_KEY_ID"
        export AWS_SECRET_ACCESS_KEY="$OPS_LOG_R2_SECRET_ACCESS_KEY"
        export AWS_DEFAULT_REGION="$OPS_LOG_R2_REGION"
        [ -z "$OPS_LOG_R2_SESSION_TOKEN" ] || export AWS_SESSION_TOKEN="$OPS_LOG_R2_SESSION_TOKEN"
        while IFS= read -r child_name; do
            case "$child_name" in
                PATH|HOME|TMPDIR|LANG|LC_*|SSL_CERT_*|AWS_ACCESS_KEY_ID|AWS_SECRET_ACCESS_KEY|AWS_SESSION_TOKEN|AWS_DEFAULT_REGION|MOCK_*) ;;
                *) export -n "$child_name" 2>/dev/null || true ;;
            esac
        done < <(compgen -e)
        unset child_name
        if [ -n "$endpoint" ]; then
            run_bounded "$REMOTE_TIMEOUT_SECONDS" aws --endpoint-url "$endpoint" "$@"
        else
            run_bounded "$REMOTE_TIMEOUT_SECONDS" aws "$@"
        fi
    )
}

validate_local_archive() {
    local archive="$1" expected_sha actual_sha
    [ -f "$archive" ] && [ -f "${archive}.sha256" ] ||
        die "incomplete local archive pair: $archive"
    run_bounded "$COMMAND_TIMEOUT_SECONDS" tar -tzf "$archive" >/dev/null || die "archive structure check failed: $archive"
    expected_sha="$(awk 'NR == 1 {print $1}' "${archive}.sha256")"
    printf '%s\n' "$expected_sha" | grep -Eq '^[0-9a-f]{64}$' ||
        die "malformed archive checksum: $archive"
    actual_sha="$(sha256_file "$archive")"
    [ "$actual_sha" = "$expected_sha" ] || die "archive checksum mismatch: $archive"
}

write_archive_marker() {
    local archive="$1" remote="$2" marker_tmp bytes sha
    bytes="$(file_size_bytes "$archive")"
    sha="$(awk 'NR == 1 {print $1}' "${archive}.sha256")"
    marker_tmp="$(mktemp "$ARCHIVE_ROOT/.staging/$(basename "$archive").offsite-verified.tmp.XXXXXX")"
    {
        printf 'schema=1\n'
        printf 'bytes=%s\n' "$bytes"
        printf 'sha256=%s\n' "$sha"
        printf 'destination=%s\n' "$remote"
        printf 'verified_at=%s\n' "$(date -u +%FT%TZ)"
    } > "$marker_tmp"
    chmod 0600 "$marker_tmp"
    mv "$marker_tmp" "${archive}.offsite-verified"
}

archive_marker_matches() {
    local archive="$1" marker="${1}.offsite-verified" destination
    local bytes sha marker_schema marker_bytes marker_sha marker_destination
    [ -f "$archive" ] && [ -f "${archive}.sha256" ] && [ -f "$marker" ] || return 1
    destination="$(destination_base)"
    [ -n "$destination" ] || return 1
    bytes="$(file_size_bytes "$archive")"
    sha="$(awk 'NR == 1 {print $1}' "${archive}.sha256")"
    printf '%s\n' "$sha" | grep -Eq '^[0-9a-f]{64}$' || return 1
    marker_schema="$(awk -F= '$1 == "schema" {print $2; exit}' "$marker")"
    marker_bytes="$(awk -F= '$1 == "bytes" {print $2; exit}' "$marker")"
    marker_sha="$(awk -F= '$1 == "sha256" {print $2; exit}' "$marker")"
    marker_destination="$(awk -F= '$1 == "destination" {print $2; exit}' "$marker")"
    case "$marker_destination" in
        "${destination%/}/"*"$(basename "$archive")") ;;
        *) return 1 ;;
    esac
    [ "$marker_schema" = "1" ] && [ "$marker_bytes" = "$bytes" ] &&
        [ "$marker_sha" = "$sha" ]
}

verify_archive_remote() {
    local archive="$1" remote="$2" expected_bytes expected_sha
    local path bucket key remote_bytes remote_sha remote_sidecar_sha
    expected_bytes="$(file_size_bytes "$archive")"
    expected_sha="$(sha256_file "$archive")"
    case "$remote" in
        s3://*)
            path="${remote#s3://}"
            bucket="${path%%/*}"
            key="${path#*/}"
            [ -n "$bucket" ] && [ "$key" != "$path" ] ||
                die "invalid offhost archive destination $remote"
            remote_bytes="$(archive_aws s3api head-object --bucket "$bucket" \
                --key "$key" --query ContentLength --output text)"
            [ "$remote_bytes" = "$expected_bytes" ] ||
                die "offhost archive size mismatch: $remote"
            remote_sidecar_sha="$(archive_aws s3 cp "${remote}.sha256" - --quiet |
                awk 'NR == 1 {print $1}')"
            [ "$remote_sidecar_sha" = "$expected_sha" ] ||
                die "offhost archive sidecar mismatch: $remote"
            remote_sha="$(archive_aws s3 cp "$remote" - --quiet | sha256_stream)"
            [ "$remote_sha" = "$expected_sha" ] ||
                die "offhost archive full read-back mismatch: $remote"
            if [ "$REQUIRE_PROVIDER_RETENTION" = 1 ]; then
                local retention_mode retention_until
                retention_mode="$(archive_aws s3api head-object --bucket "$bucket" --key "$key" --query ObjectLockMode --output text)"
                retention_until="$(archive_aws s3api head-object --bucket "$bucket" --key "$key" --query ObjectLockRetainUntilDate --output text)"
                case "$retention_mode" in GOVERNANCE|COMPLIANCE) ;; *) die "provider retention mode is missing for $remote" ;; esac
                [ -n "$retention_until" ] && [ "$retention_until" != None ] ||
                    die "provider retention deadline is missing for $remote"
            fi
            ;;
        *)
            if [ -n "$(run_bounded "$REMOTE_TIMEOUT_SECONDS" rsync -acn --itemize-changes "$archive" "${archive}.sha256" \
                "$(destination_base)/")" ]; then
                die "offhost archive rsync verification failed: $remote"
            fi
            ;;
    esac
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
            echo "BLUEY_OPS_LOG_ARCHIVE_DESTINATION or BLUEY_OPS_LOG_R2_BUCKET is required" >&2
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
            [ -n "$OPS_LOG_R2_ACCESS_KEY_ID" ] &&
                [ -n "$OPS_LOG_R2_SECRET_ACCESS_KEY" ] ||
                die "dedicated BLUEY_OPS_LOG_R2 credentials are required"
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
            local remote
            remote="$destination/$date_prefix/$(basename "$archive")"
            local payload_exists=0 sidecar_exists=0 remote_path remote_bucket remote_key
            local remote_sha remote_sidecar_sha
            remote_path="${remote#s3://}"; remote_bucket="${remote_path%%/*}"; remote_key="${remote_path#*/}"
            archive_aws s3api head-object --bucket "$remote_bucket" --key "$remote_key" >/dev/null 2>&1 && payload_exists=1
            archive_aws s3api head-object --bucket "$remote_bucket" --key "${remote_key}.sha256" >/dev/null 2>&1 && sidecar_exists=1
            # Never overwrite an immutable component. A surviving component
            # must first match the local authority; only its absent peer may be
            # uploaded, followed by a complete pair read-back.
            if [ "$payload_exists" = 1 ]; then
                remote_sha="$(archive_aws s3 cp "$remote" - --quiet | sha256_stream)"
                [ "$remote_sha" = "$(sha256_file "$archive")" ] || die "existing immutable archive differs: $remote"
            else
                archive_aws s3 cp "$archive" "$remote" --quiet
            fi
            if [ "$sidecar_exists" = 1 ]; then
                remote_sidecar_sha="$(archive_aws s3 cp "${remote}.sha256" - --quiet | awk 'NR == 1 {print $1}')"
                [ "$remote_sidecar_sha" = "$(sha256_file "$archive")" ] || die "existing immutable sidecar differs: $remote"
            else
                archive_aws s3 cp "${archive}.sha256" "${remote}.sha256" --quiet
            fi
            verify_archive_remote "$archive" "$remote"
            write_archive_marker "$archive" "$remote"
            UPLOADED_STORAGE="r2"
            UPLOADED_OBJECT_KEY="$object_key"
            UPLOADED_LOCAL_PATH=""
            ;;
        *)
            if command -v rsync >/dev/null 2>&1; then
                run_bounded "$REMOTE_TIMEOUT_SECONDS" rsync -a --quiet "$archive" "${archive}.sha256" "$destination/"
                local remote
                remote="$destination/$(basename "$archive")"
                verify_archive_remote "$archive" "$remote"
                write_archive_marker "$archive" "$remote"
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

remove_archive_pair() {
    local archive="$1" destination marker_destination
    destination="$(destination_base)"
    if [ -n "$destination" ] || [ "$REQUIRE_OFFHOST" = "1" ]; then
        archive_marker_matches "$archive" ||
            die "refusing to prune archive without current offhost proof: $archive"
        validate_local_archive "$archive"
        marker_destination="$(awk -F= '$1 == "destination" {print $2; exit}' \
            "${archive}.offsite-verified")"
        verify_archive_remote "$archive" "$marker_destination"
        write_archive_marker "$archive" "$marker_destination"
    fi
    rm -f -- "$archive" "${archive}.sha256" "${archive}.offsite-verified"
}

reconcile_unverified_archives() {
    local destination archive
    destination="$(destination_base)"
    [ -n "$destination" ] || return 0
    while IFS= read -r archive; do
        [ -n "$archive" ] || continue
        archive_marker_matches "$archive" && continue
        echo "reconciling unverified local log archive: $archive" >&2
        validate_local_archive "$archive"
        upload_archive "$archive"
    done < <(list_files_oldest "$ARCHIVE_ROOT" -maxdepth 1 -name 'bluey-logs-*.tar.gz')
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
    if ! command -v timeout >/dev/null 2>&1; then
        echo "warn: timeout not installed; skipped noncritical diagnostic index insert" >&2
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

    if ! PGDATABASE="$BLUEY_DATABASE_URL" \
        PGSSLROOTCERT="${PGSSLROOTCERT:-${BLUEY_POSTGRES_CA_CERT_PATH:-}}" \
        PGCONNECT_TIMEOUT="$DB_CONNECT_TIMEOUT_SECONDS" \
        PGOPTIONS="-c statement_timeout=${DB_STATEMENT_TIMEOUT_MS}" \
        timeout --signal=TERM --kill-after="$TIMEOUT_KILL_GRACE_SECONDS" "$DB_WALL_TIMEOUT_SECONDS" \
        psql -X \
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
     'redaction_best_effort', true,
     'may_contain_user_content', true,
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
    mkdir -p "$ARCHIVE_ROOT/.staging" "$WORK_ROOT"
    chmod 0700 "$ARCHIVE_ROOT/.staging"
    local available_kb required_kb log_dir log_dir_bytes=0
    # This is an operational hard bound, not merely a documented target. Stop
    # before making another work copy when the configured hot-log roots have
    # already exceeded their aggregate authority; preserve every source byte
    # for explicit recovery/pruning rather than deleting under pressure.
    for log_dir in $LOG_DIRS; do
        [ -d "$log_dir" ] || continue
        log_dir_bytes=$((log_dir_bytes + $(dir_bytes "$log_dir")))
    done
    [ "$log_dir_bytes" -le "$LOG_DIR_MAX_BYTES" ] ||
        die "configured hot log roots exceed BLUEY_LOG_DIR_MAX_BYTES"
    available_kb="$(df -Pk "$ARCHIVE_ROOT" | awk 'NR == 2 {print $4}')"
    is_uint "$available_kb" || die "could not measure archive filesystem reserve"
    required_kb=$((MIN_FREE_GB * 1024 * 1024 + (BUNDLE_MAX_BYTES * 2 + 1023) / 1024))
    [ "$available_kb" -ge "$required_kb" ] ||
        die "archive reserve is below ${MIN_FREE_GB}G plus two bounded work copies"
    local ts host bundle bundle_name archive archive_tmp archive_sha
    ts="$(date -u +%Y%m%dT%H%M%SZ)"
    host="$(hostname -s 2>/dev/null || hostname || echo unknown-host)"
    bundle="$(mktemp -d "$WORK_ROOT/bluey-logs-${host}-${ts}.XXXXXX")"
    bundle_name="$(basename "$bundle")"
    CURRENT_BUNDLE="$bundle"
    archive="$ARCHIVE_ROOT/${bundle_name}.tar.gz"
    archive_tmp="$(mktemp "$ARCHIVE_ROOT/.staging/${bundle_name}.tar.gz.tmp.XXXXXX")"
    CURRENT_ARCHIVE_TMP="$archive_tmp"
    RUN_ARCHIVE_NAME="$(basename "$archive")"
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
            local remaining
            remaining=$((BUNDLE_MAX_BYTES - $(dir_bytes "$bundle")))
            [ "$remaining" -gt 0 ] || break
            run_bounded "$COMMAND_TIMEOUT_SECONDS" journalctl -u "$service" --since "$SINCE" --no-pager -o short-iso 2>/dev/null \
                | redact_stream | tail -c "$remaining" \
                > "$bundle/journal/${service}.log" || true
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
            local remaining
            remaining=$((BUNDLE_MAX_BYTES - $(dir_bytes "$bundle")))
            [ "$remaining" -gt 0 ] || break
            case "$path" in
                *.gz)
                    if command -v gzip >/dev/null 2>&1; then
                        gzip -cd "$path" 2>/dev/null | redact_stream | tail -c "$remaining" \
                            > "${dest%.gz}.log" || true
                    fi
                    ;;
                *)
                    redact_stream < "$path" | tail -c "$remaining" > "$dest" || true
                    ;;
            esac
        done < <(list_files_newest "$log_dir" \
            \( -name '*.log' -o -name '*.log.*' -o -name '*.log-*' -o -name '*.jsonl' -o -name '*.gz' \))
    done

    [ "$(dir_bytes "$bundle")" -le "$BUNDLE_MAX_BYTES" ] ||
        die "bounded archive bundle exceeded BLUEY_LOG_ARCHIVE_BUNDLE_MAX_BYTES"
    tar -C "$WORK_ROOT" -czf "$archive_tmp" "$bundle_name"
    [ "$(file_size_bytes "$archive_tmp")" -le "$BUNDLE_MAX_BYTES" ] ||
        die "compressed archive exceeded BLUEY_LOG_ARCHIVE_BUNDLE_MAX_BYTES"
    tar -tzf "$archive_tmp" >/dev/null || die "new log archive failed structural validation"
    archive_sha="$(sha256_file "$archive_tmp")"
    printf '%s  %s\n' "$archive_sha" "$(basename "$archive")" > "${archive_tmp}.sha256"
    mv "${archive_tmp}.sha256" "${archive}.sha256"
    if ! mv "$archive_tmp" "$archive"; then
        rm -f "${archive}.sha256"
        return 1
    fi
    CURRENT_ARCHIVE_TMP=""
    upload_archive "$archive"
    index_archive_metadata "$archive"
    remove_work_dir "$bundle" 1
    CURRENT_BUNDLE=""
    echo "$(date -u +%FT%TZ) log archive ok: $archive"
}

prune_stale_work
prune_archive_staging
if [ "$REQUIRE_OFFHOST" = "1" ] && [ -z "$(destination_base)" ]; then
    die "BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST=1 requires an offhost destination"
fi
if [ "$PRUNE_ONLY" != "1" ]; then
    RUN_STARTED_EPOCH="$(date -u +%s)"
    ARCHIVE_RUN_ACTIVE=1
    write_archive_status running 0 || die "could not persist log archive running status"
fi
reconcile_unverified_archives

if [ "$PRUNE_ONLY" = "1" ]; then
    prune_archives
    echo "$(date -u +%FT%TZ) log prune ok"
    exit 0
fi

archive_logs
prune_archives
