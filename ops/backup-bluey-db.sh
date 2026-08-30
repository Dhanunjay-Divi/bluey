#!/usr/bin/env bash
# Bluey API database backup script.
#
# The backup is staged on the backup filesystem, checksummed, and finalized by
# rename while a host-wide flock is held. Local count retention is bounded and
# capacity pruning may remove only snapshots with exact offsite read-back
# evidence, while always preserving the configured hourly and daily minima.

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
    while [ "$current" != "/" ]; do
        [ -d "$current" ] && [ ! -L "$current" ] || return 1
        owner="$(bootstrap_stat -c%u -f%u "$current")" || return 1
        if [ "$EUID" = "0" ]; then
            [ "$owner" = "0" ] || return 1
        else
            { [ "$owner" = "0" ] || [ "$owner" = "$EUID" ]; } || return 1
        fi
        mode="$(bootstrap_stat -c%a -f%Lp "$current")" || return 1
        case "$mode" in ""|*[!0-9]*) return 1 ;; esac
        mode_value=$((8#$mode))
        if [ $((mode_value & 8#022)) -ne 0 ]; then
            [ "$owner" = "0" ] && [ $((mode_value & 8#1000)) -ne 0 ] || return 1
        fi
        current="$(dirname "$current")"
    done
}

bootstrap_identity() {
    if stat -c '%i' -- "$1" >/dev/null 2>&1; then
        stat -c '%i' -- "$1"
    else
        stat -f '%i' -- "$1"
    fi
}

load_env_file() {
    local env_file="$1" env_fd path_identity fd_identity
    if [ -e "$env_file" ] || [ -L "$env_file" ]; then
        [ -r "$env_file" ] && bootstrap_trusted_parent_chain "$env_file" &&
            bootstrap_trusted_file "$env_file" || {
            echo "backup failed: environment file is not trusted and write-protected" >&2
            exit 1
        }
        path_identity="$(bootstrap_identity "$env_file")" || exit 1
        exec 9<"$env_file"
        env_fd=9
        fd_identity="$(bootstrap_identity "/dev/fd/$env_fd")" || exit 1
        [ "$path_identity" = "$fd_identity" ] || {
            echo "backup failed: environment file changed while opening" >&2
            exit 1
        }
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

DB_PATH="${BLUEY_DB_PATH-/opt/bluey-api/bluey.db}"
BACKUP_DIR="${BLUEY_BACKUP_DIR-/var/backups/bluey-api}"
HOURLY_KEEP="${BLUEY_BACKUP_HOURLY_KEEP:-4}"
DAILY_KEEP="${BLUEY_BACKUP_DAILY_KEEP:-7}"
MIN_HOURLY_KEEP="${BLUEY_BACKUP_MIN_HOURLY_KEEP:-2}"
MIN_DAILY_KEEP="${BLUEY_BACKUP_MIN_DAILY_KEEP:-2}"
LOCAL_MAX_BYTES="${BLUEY_BACKUP_LOCAL_MAX_BYTES:-12884901888}"
MAX_SNAPSHOT_BYTES="${BLUEY_BACKUP_MAX_SNAPSHOT_BYTES:-2147483648}"
MIN_FREE_GB="${BLUEY_BACKUP_MIN_FREE_GB:-16}"
LOCK_WAIT_SECONDS="${BLUEY_BACKUP_LOCK_WAIT_SECONDS:-0}"
COMMAND_TIMEOUT_SECONDS="${BLUEY_BACKUP_COMMAND_TIMEOUT_SECONDS:-900}"
REMOTE_TIMEOUT_SECONDS="${BLUEY_BACKUP_REMOTE_TIMEOUT_SECONDS:-300}"
TIMEOUT_KILL_GRACE_SECONDS="${BLUEY_BACKUP_TIMEOUT_KILL_GRACE_SECONDS:-5}"
REQUIRE_OFFSITE="${BLUEY_BACKUP_REQUIRE_OFFSITE:-0}"
DB_BACKEND="$(printf '%s' "${BLUEY_SERVER_DB_BACKEND:-sqlite}" | tr '[:upper:]' '[:lower:]')"

# Examples:
#   OFFSITE_DESTINATION=s3://my-bucket/bluey-api-backups/
#   OFFSITE_DESTINATION=user@backuphost:/srv/backups/bluey-api/
OFFSITE_DESTINATION="${OFFSITE_DESTINATION:-}"
BLUEY_BACKUP_S3_ENDPOINT_URL="${BLUEY_BACKUP_S3_ENDPOINT_URL:-}"
BACKUP_AWS_ACCESS_KEY_ID="${AWS_ACCESS_KEY_ID:-}"
BACKUP_AWS_SECRET_ACCESS_KEY="${AWS_SECRET_ACCESS_KEY:-}"
BACKUP_AWS_SESSION_TOKEN="${AWS_SESSION_TOKEN:-}"
BACKUP_AWS_REGION="${AWS_DEFAULT_REGION:-${AWS_REGION:-auto}}"
while IFS= read -r inherited_name; do
    case "$inherited_name" in
        *KEY*|*TOKEN*|*SECRET*|*PASSWORD*|*DATABASE_URL*|*DSN*|*WEBHOOK*|*CREDENTIAL*|*COOKIE*|*AUTH*)
            export -n "$inherited_name" 2>/dev/null || true
            ;;
    esac
done < <(compgen -e)
unset inherited_name
BACKUP_STATUS_FILE="${BLUEY_BACKUP_STATUS_FILE-$BACKUP_DIR/.backup.status}"
REQUIRE_STATUS="${BLUEY_BACKUP_REQUIRE_STATUS:-1}"
REQUIRE_TRUSTED_PATHS="${BLUEY_BACKUP_REQUIRE_TRUSTED_PATHS:-1}"

MODE="${1:-backup}"
case "$MODE" in
    backup|--backup) ;;
    --verify-existing) ;;
    --check-config) ;;
    *) echo "usage: $0 [--backup|--verify-existing|--check-config]" >&2; exit 2 ;;
esac

die() {
    echo "backup failed: $*" >&2
    exit 1
}

timeout_binary() {
    command -v timeout 2>/dev/null || command -v gtimeout 2>/dev/null || true
}

run_bounded() {
    local seconds="$1"; shift
    local timeout_bin
    timeout_bin="$(timeout_binary)"
    [ -n "$timeout_bin" ] || die "timeout utility is required for bounded operations"
    "$timeout_bin" --signal=TERM --kill-after="$TIMEOUT_KILL_GRACE_SECONDS" "$seconds" "$@"
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

is_uint() {
    case "$1" in
        ""|*[!0-9]*) return 1 ;;
        *) return 0 ;;
    esac
}

validate_uint() {
    local name="$1" value="$2" minimum="$3"
    is_uint "$value" || die "$name must be an unsigned integer"
    [ "$value" -ge "$minimum" ] || die "$name must be at least $minimum"
}

validate_bool() {
    local name="$1" value="$2"
    case "$value" in
        0|1) ;;
        *) die "$name must be 0 or 1" ;;
    esac
}

is_placeholder() {
    case "$1" in
        ""|*replace-with*|*.invalid*) return 0 ;;
        *) return 1 ;;
    esac
}

validate_uint BLUEY_BACKUP_HOURLY_KEEP "$HOURLY_KEEP" 1
validate_uint BLUEY_BACKUP_DAILY_KEEP "$DAILY_KEEP" 1
validate_uint BLUEY_BACKUP_MIN_HOURLY_KEEP "$MIN_HOURLY_KEEP" 1
validate_uint BLUEY_BACKUP_MIN_DAILY_KEEP "$MIN_DAILY_KEEP" 1
validate_uint BLUEY_BACKUP_LOCAL_MAX_BYTES "$LOCAL_MAX_BYTES" 0
validate_uint BLUEY_BACKUP_MAX_SNAPSHOT_BYTES "$MAX_SNAPSHOT_BYTES" 1
validate_uint BLUEY_BACKUP_COMMAND_TIMEOUT_SECONDS "$COMMAND_TIMEOUT_SECONDS" 1
validate_uint BLUEY_BACKUP_REMOTE_TIMEOUT_SECONDS "$REMOTE_TIMEOUT_SECONDS" 1
validate_uint BLUEY_BACKUP_TIMEOUT_KILL_GRACE_SECONDS "$TIMEOUT_KILL_GRACE_SECONDS" 1
validate_uint BLUEY_BACKUP_MIN_FREE_GB "$MIN_FREE_GB" 0
validate_uint BLUEY_BACKUP_LOCK_WAIT_SECONDS "$LOCK_WAIT_SECONDS" 0
validate_bool BLUEY_BACKUP_REQUIRE_OFFSITE "$REQUIRE_OFFSITE"
validate_bool BLUEY_BACKUP_REQUIRE_STATUS "$REQUIRE_STATUS"
validate_bool BLUEY_BACKUP_REQUIRE_TRUSTED_PATHS "$REQUIRE_TRUSTED_PATHS"
validate_safe_directory BLUEY_BACKUP_DIR "$BACKUP_DIR" 2
validate_safe_directory BLUEY_BACKUP_STATUS_ROOT "$(dirname "$BACKUP_STATUS_FILE")" 2
validate_safe_directory BLUEY_BACKUP_STATUS_FILE "$BACKUP_STATUS_FILE" 3
case "$BACKUP_STATUS_FILE" in
    "$BACKUP_DIR"/*) ;;
    *) die "BLUEY_BACKUP_STATUS_FILE must be inside BLUEY_BACKUP_DIR" ;;
esac
[ "$(dirname "$BACKUP_STATUS_FILE")" = "$BACKUP_DIR" ] ||
    die "BLUEY_BACKUP_STATUS_FILE must be directly inside BLUEY_BACKUP_DIR"
[ "$MIN_HOURLY_KEEP" -le "$HOURLY_KEEP" ] ||
    die "BLUEY_BACKUP_MIN_HOURLY_KEEP exceeds BLUEY_BACKUP_HOURLY_KEEP"
[ "$MIN_DAILY_KEEP" -le "$DAILY_KEEP" ] ||
    die "BLUEY_BACKUP_MIN_DAILY_KEEP exceeds BLUEY_BACKUP_DAILY_KEEP"
if [ "$LOCAL_MAX_BYTES" -gt 0 ] && [ "$MAX_SNAPSHOT_BYTES" -ge "$LOCAL_MAX_BYTES" ]; then
    die "BLUEY_BACKUP_MAX_SNAPSHOT_BYTES must be below BLUEY_BACKUP_LOCAL_MAX_BYTES"
fi
if [ "$REQUIRE_OFFSITE" = "1" ] && [ -z "$OFFSITE_DESTINATION" ]; then
    die "BLUEY_BACKUP_REQUIRE_OFFSITE=1 requires OFFSITE_DESTINATION"
fi
case "$OFFSITE_DESTINATION" in
    s3://*)
        is_placeholder "$BACKUP_AWS_ACCESS_KEY_ID" &&
            die "S3 backup access key is missing or a placeholder"
        is_placeholder "$BACKUP_AWS_SECRET_ACCESS_KEY" &&
            die "S3 backup secret is missing or a placeholder"
        ;;
esac

file_size_bytes() {
    if stat -c%s "$1" >/dev/null 2>&1; then
        stat -c%s "$1"
    else
        stat -f%z "$1"
    fi
}

file_owner_uid() {
    if stat -c%u "$1" >/dev/null 2>&1; then stat -c%u "$1"; else stat -f%u "$1"; fi
}

file_mode() {
    if stat -c%a "$1" >/dev/null 2>&1; then stat -c%a "$1"; else stat -f%Lp "$1"; fi
}

trusted_lock_path() {
    local path="$1" mode mode_value
    if [ -L "$path" ] || { [ -e "$path" ] && [ ! -f "$path" ]; }; then
        return 1
    fi
    [ -e "$path" ] || return 0
    [ "$(file_owner_uid "$path")" = "$(id -u)" ] || return 1
    mode="$(file_mode "$path")"
    is_uint "$mode" || return 1
    mode_value=$((8#$mode))
    [ $((mode_value & 8#022)) -eq 0 ]
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

dir_size_bytes() {
    if [ ! -e "$1" ]; then
        echo 0
    elif du -sb "$1" >/dev/null 2>&1; then
        du -sb "$1" | awk '{print $1}'
    else
        du -sk "$1" | awk '{print $1 * 1024}'
    fi
}

backup_hot_bytes() {
    local hourly_bytes daily_bytes staging_bytes
    hourly_bytes="$(dir_size_bytes "$BACKUP_DIR/hourly")"
    daily_bytes="$(dir_size_bytes "$BACKUP_DIR/daily")"
    staging_bytes="$(dir_size_bytes "$BACKUP_DIR/.staging")"
    echo $((hourly_bytes + daily_bytes + staging_bytes))
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

list_backups() {
    local order="$1"
    if supports_find_printf "$BACKUP_DIR"; then
        find "$BACKUP_DIR/hourly" "$BACKUP_DIR/daily" -maxdepth 1 -type f \
            \( -name '*.db' -o -name '*.pgdump' \) -printf '%T@ %p\n' 2>/dev/null |
            if [ "$order" = "oldest" ]; then sort -n; else sort -rn; fi |
            cut -d' ' -f2-
    else
        find "$BACKUP_DIR/hourly" "$BACKUP_DIR/daily" -maxdepth 1 -type f \
            \( -name '*.db' -o -name '*.pgdump' \) \
            -exec stat -f '%m %N' {} \; 2>/dev/null |
            if [ "$order" = "oldest" ]; then sort -n; else sort -rn; fi |
            cut -d' ' -f2-
    fi
}

list_backend_backups_newest() {
    local dir="$1" extension="$2"
    if supports_find_printf "$dir"; then
        find "$dir" -maxdepth 1 -type f -name "*.${extension}" \
            -printf '%T@ %p\n' 2>/dev/null | sort -rn | cut -d' ' -f2-
    else
        find "$dir" -maxdepth 1 -type f -name "*.${extension}" \
            -exec stat -f '%m %N' {} \; 2>/dev/null | sort -rn | cut -d' ' -f2-
    fi
}

write_checksum() {
    local source="$1" destination="$2" sha
    sha="$(sha256_file "$source")"
    printf '%s  %s\n' "$sha" "$(basename "$destination")" > "${source}.sha256"
}

finalize_pair() {
    local staged="$1" target="$2"
    [ ! -e "$target" ] || die "refusing to replace existing backup $target"
    validate_snapshot "$staged"
    write_checksum "$staged" "$target"
    mv "${staged}.sha256" "${target}.sha256"
    if ! mv "$staged" "$target"; then
        rm -f "${target}.sha256"
        die "could not finalize $target"
    fi
    chmod 600 "$target" "${target}.sha256"
}

remove_backup_pair() {
    local backup="$1"
    rm -f -- "$backup" "${backup}.sha256" "${backup}.offsite-verified"
}

verified_marker_matches() {
    local backup="$1" marker="${1}.offsite-verified" bytes sidecar_sha
    local marker_schema marker_bytes marker_sha marker_destination marker_layout expected_destination
    [ -f "$backup" ] && [ -f "${backup}.sha256" ] && [ -f "$marker" ] || return 1
    [ -n "$OFFSITE_DESTINATION" ] || return 1
    bytes="$(file_size_bytes "$backup")"
    sidecar_sha="$(awk 'NR == 1 {print $1}' "${backup}.sha256")"
    printf '%s\n' "$sidecar_sha" | grep -Eq '^[0-9a-f]{64}$' || return 1
    marker_layout="$(awk -F= '$1 == "layout" {print $2; exit}' "$marker")"
    [ -n "$marker_layout" ] || marker_layout=class-prefix
    expected_destination="$(expected_remote_for_backup "$backup" "$marker_layout")" || return 1
    marker_schema="$(awk -F= '$1 == "schema" {print $2; exit}' "$marker")"
    marker_bytes="$(awk -F= '$1 == "bytes" {print $2; exit}' "$marker")"
    marker_sha="$(awk -F= '$1 == "sha256" {print $2; exit}' "$marker")"
    marker_destination="$(awk -F= '$1 == "destination" {print $2; exit}' "$marker")"
    [ "$marker_schema" = "1" ] && [ "$marker_bytes" = "$bytes" ] &&
        [ "$marker_sha" = "$sidecar_sha" ] &&
        [ "$marker_destination" = "$expected_destination" ]
}

expected_remote_for_backup() {
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
        "$BACKUP_DIR/hourly"/*) class=hourly ;;
        "$BACKUP_DIR/daily"/*) class=daily ;;
        *) return 1 ;;
    esac
    printf '%s/%s/%s\n' "${OFFSITE_DESTINATION%/}" "$class" "$source_name"
}

sidecar_source_name() {
    local backup="$1" source_path source_name
    [ -f "${backup}.sha256" ] || return 1
    source_path="$(awk 'NR == 1 {print $NF}' "${backup}.sha256")"
    source_name="$(basename "${source_path#\*}")"
    case "$source_name" in
        bluey-*.db|bluey-postgres-*.pgdump) printf '%s\n' "$source_name" ;;
        *) return 1 ;;
    esac
}

validate_snapshot() {
    local snapshot="$1" check_output
    case "$snapshot" in
        *.db)
            command -v sqlite3 >/dev/null 2>&1 ||
                die "sqlite3 is required to validate SQLite backups"
            check_output="$(run_bounded "$COMMAND_TIMEOUT_SECONDS" sqlite3 "$snapshot" 'PRAGMA quick_check;')" ||
                die "SQLite structural validation failed for $snapshot"
            [ "$check_output" = "ok" ] ||
                die "SQLite quick_check rejected $snapshot"
            ;;
        *.pgdump)
            command -v pg_restore >/dev/null 2>&1 ||
                die "pg_restore is required to validate PostgreSQL backups"
            run_bounded "$COMMAND_TIMEOUT_SECONDS" pg_restore --list "$snapshot" >/dev/null ||
                die "PostgreSQL archive validation failed for $snapshot"
            ;;
        *) die "unsupported backup snapshot type: $snapshot" ;;
    esac
}

run_with_snapshot_limit() {
    local limit_blocks
    limit_blocks=$(((MAX_SNAPSHOT_BYTES + 1023) / 1024))
    (
        ulimit -f "$limit_blocks" || exit 70
        "$@"
    )
}

enforce_snapshot_size() {
    local snapshot="$1" bytes
    bytes="$(file_size_bytes "$snapshot")"
    [ "$bytes" -le "$MAX_SNAPSHOT_BYTES" ] ||
        die "snapshot exceeds BLUEY_BACKUP_MAX_SNAPSHOT_BYTES"
}

write_verified_marker() {
    local backup="$1" destination="$2" layout="${3:-class-prefix}"
    local bytes="${4:-}" sha="${5:-}" marker_tmp
    [ -n "$bytes" ] || bytes="$(file_size_bytes "$backup")"
    [ -n "$sha" ] || sha="$(sha256_file "$backup")"
    marker_tmp="$(mktemp "$BACKUP_DIR/.staging/$(basename "$backup").offsite-verified.tmp.XXXXXX")"
    {
        printf 'schema=1\n'
        printf 'bytes=%s\n' "$bytes"
        printf 'sha256=%s\n' "$sha"
        printf 'destination=%s\n' "$destination"
        printf 'layout=%s\n' "$layout"
        printf 'verified_at=%s\n' "$(date -u +%FT%TZ)"
    } > "$marker_tmp"
    chmod 600 "$marker_tmp"
    mv "$marker_tmp" "${backup}.offsite-verified"
}

aws_cli() {
    (
        export AWS_ACCESS_KEY_ID="$BACKUP_AWS_ACCESS_KEY_ID"
        export AWS_SECRET_ACCESS_KEY="$BACKUP_AWS_SECRET_ACCESS_KEY"
        export AWS_DEFAULT_REGION="$BACKUP_AWS_REGION"
        [ -z "$BACKUP_AWS_SESSION_TOKEN" ] || export AWS_SESSION_TOKEN="$BACKUP_AWS_SESSION_TOKEN"
        while IFS= read -r child_name; do
            case "$child_name" in
                PATH|HOME|TMPDIR|LANG|LC_*|SSL_CERT_*|AWS_ACCESS_KEY_ID|AWS_SECRET_ACCESS_KEY|AWS_SESSION_TOKEN|AWS_DEFAULT_REGION|MOCK_*) ;;
                *) export -n "$child_name" 2>/dev/null || true ;;
            esac
        done < <(compgen -e)
        unset child_name
        if [ -n "$BLUEY_BACKUP_S3_ENDPOINT_URL" ]; then
            run_bounded "$REMOTE_TIMEOUT_SECONDS" aws --endpoint-url "$BLUEY_BACKUP_S3_ENDPOINT_URL" "$@"
        else
            run_bounded "$REMOTE_TIMEOUT_SECONDS" aws "$@"
        fi
    )
}

verify_s3_pair() {
    local backup="$1" remote="$2" expected_bytes="${3:-}" expected_sha="${4:-}"
    local path bucket key remote_bytes remote_sha remote_sidecar_sha
    path="${remote#s3://}"
    bucket="${path%%/*}"
    key="${path#*/}"
    [ -n "$bucket" ] && [ "$key" != "$path" ] || die "invalid S3 backup destination $remote"
    [ -n "$expected_bytes" ] || expected_bytes="$(file_size_bytes "$backup")"
    [ -n "$expected_sha" ] || expected_sha="$(sha256_file "$backup")"
    remote_bytes="$(aws_cli s3api head-object \
        --bucket "$bucket" --key "$key" --query ContentLength --output text)"
    [ "$remote_bytes" = "$expected_bytes" ] ||
        die "offsite size mismatch for $remote"
    remote_sidecar_sha="$(aws_cli s3 cp "${remote}.sha256" - --quiet |
        awk 'NR == 1 {print $1}')"
    [ "$remote_sidecar_sha" = "$expected_sha" ] ||
        die "offsite checksum sidecar mismatch for $remote"
    remote_sha="$(aws_cli s3 cp "$remote" - --quiet | sha256_stream)"
    [ "$remote_sha" = "$expected_sha" ] ||
        die "offsite full read-back mismatch for $remote"
}

s3_object_exists() {
    local remote="$1" path bucket key
    path="${remote#s3://}"
    bucket="${path%%/*}"
    key="${path#*/}"
    [ -n "$bucket" ] && [ "$key" != "$path" ] || return 1
    aws_cli s3api head-object --bucket "$bucket" --key "$key" \
        --query ContentLength --output text >/dev/null 2>&1
}

verify_existing_s3_payload_component() {
    local remote="$1" expected_bytes="$2" expected_sha="$3" path bucket key remote_bytes remote_sha
    path="${remote#s3://}"
    bucket="${path%%/*}"
    key="${path#*/}"
    remote_bytes="$(aws_cli s3api head-object --bucket "$bucket" --key "$key" \
        --query ContentLength --output text)"
    [ "$remote_bytes" = "$expected_bytes" ] ||
        die "existing offsite payload size mismatch for $remote"
    remote_sha="$(aws_cli s3 cp "$remote" - --quiet | sha256_stream)"
    [ "$remote_sha" = "$expected_sha" ] ||
        die "existing offsite payload hash mismatch for $remote"
}

verify_existing_s3_sidecar_component() {
    local remote="$1" expected_sha="$2" remote_sidecar_sha
    remote_sidecar_sha="$(aws_cli s3 cp "${remote}.sha256" - --quiet |
        awk 'NR == 1 {print $1}')"
    [ "$remote_sidecar_sha" = "$expected_sha" ] ||
        die "existing offsite sidecar mismatch for $remote"
}

upload_and_verify() {
    local backup="$1" remote expected_bytes expected_sha payload_exists=0 sidecar_exists=0
    [ -n "$OFFSITE_DESTINATION" ] || return 0
    remote="$(expected_remote_for_backup "$backup" class-prefix)" ||
        die "cannot derive class-prefixed offsite destination for $backup"
    expected_bytes="$(file_size_bytes "$backup")"
    expected_sha="$(awk 'NR == 1 {print $1}' "${backup}.sha256")"
    printf '%s\n' "$expected_sha" | grep -Eq '^[0-9a-f]{64}$' ||
        die "malformed checksum sidecar for $backup"
    case "$OFFSITE_DESTINATION" in
        s3://*)
            command -v aws >/dev/null 2>&1 || die "aws CLI is required for offsite backups"
            s3_object_exists "$remote" && payload_exists=1
            s3_object_exists "${remote}.sha256" && sidecar_exists=1
            # Immutable-prefix recovery must never overwrite a remote component
            # that survived a killed writer. Upload only missing pieces, then
            # prove the exact pair by HEAD, sidecar, and full read-back.
            if [ "$payload_exists" != "1" ] || [ "$sidecar_exists" != "1" ]; then
                if [ "$payload_exists" = "1" ]; then
                    verify_existing_s3_payload_component \
                        "$remote" "$expected_bytes" "$expected_sha"
                else
                    aws_cli s3 cp "$backup" "$remote" --quiet
                fi
                if [ "$sidecar_exists" = "1" ]; then
                    verify_existing_s3_sidecar_component "$remote" "$expected_sha"
                else
                    aws_cli s3 cp "${backup}.sha256" "${remote}.sha256" --quiet
                fi
            fi
            # A complete immutable pair needs one, not two, full payload reads.
            # Partial-pair recovery validates any surviving component before it
            # uploads the absent one, then repeats the whole-pair proof below.
            verify_s3_pair "$backup" "$remote" "$expected_bytes" "$expected_sha"
            write_verified_marker "$backup" "$remote" class-prefix \
                "$expected_bytes" "$expected_sha"
            ;;
        *)
            command -v rsync >/dev/null 2>&1 || die "rsync is required for offsite backups"
            run_bounded "$REMOTE_TIMEOUT_SECONDS" rsync -a --quiet "$backup" "$remote"
            run_bounded "$REMOTE_TIMEOUT_SECONDS" rsync -a --quiet "${backup}.sha256" "${remote}.sha256"
            if [ -n "$(run_bounded "$REMOTE_TIMEOUT_SECONDS" rsync -acn --itemize-changes "$backup" "$remote")" ] ||
                [ -n "$(run_bounded "$REMOTE_TIMEOUT_SECONDS" rsync -acn --itemize-changes "${backup}.sha256" "${remote}.sha256")" ]; then
                die "offsite rsync checksum verification failed for $remote"
            fi
            write_verified_marker "$backup" "$remote" class-prefix \
                "$expected_bytes" "$expected_sha"
            ;;
    esac
}

reverify_offsite_pair() {
    local backup="$1" remote layout expected_bytes="${2:-}" expected_sha="${3:-}"
    verified_marker_matches "$backup" || die "current offsite marker is required for deletion"
    remote="$(awk -F= '$1 == "destination" {print $2; exit}' "${backup}.offsite-verified")"
    layout="$(awk -F= '$1 == "layout" {print $2; exit}' "${backup}.offsite-verified")"
    [ -n "$layout" ] || layout=class-prefix
    case "$OFFSITE_DESTINATION" in
        s3://*)
            command -v aws >/dev/null 2>&1 || die "aws CLI is required for offsite backups"
            verify_s3_pair "$backup" "$remote" "$expected_bytes" "$expected_sha"
            ;;
        *)
            command -v rsync >/dev/null 2>&1 || die "rsync is required for offsite backups"
            [ "$layout" = "class-prefix" ] ||
                die "legacy remapped backups require S3 --verify-existing bootstrap"
            if [ -n "$(run_bounded "$REMOTE_TIMEOUT_SECONDS" rsync -acn --itemize-changes "$backup" "$remote")" ] ||
                [ -n "$(run_bounded "$REMOTE_TIMEOUT_SECONDS" rsync -acn --itemize-changes "${backup}.sha256" "${remote}.sha256")" ]; then
                die "offsite rsync checksum verification failed for $remote"
            fi
            ;;
    esac
    write_verified_marker "$backup" "$remote" "$layout" "$expected_bytes" "$expected_sha"
}

VALIDATED_LOCAL_BYTES=""
VALIDATED_LOCAL_SHA=""
validate_local_pair() {
    local backup="$1" expected_sha actual_sha
    [ -f "${backup}.sha256" ] || die "missing checksum sidecar for $backup"
    expected_sha="$(awk 'NR == 1 {print $1}' "${backup}.sha256")"
    printf '%s\n' "$expected_sha" | grep -Eq '^[0-9a-f]{64}$' ||
        die "malformed checksum sidecar for $backup"
    actual_sha="$(sha256_file "$backup")"
    [ "$actual_sha" = "$expected_sha" ] ||
        die "local checksum sidecar mismatch for $backup"
    VALIDATED_LOCAL_BYTES="$(file_size_bytes "$backup")"
    VALIDATED_LOCAL_SHA="$actual_sha"
}

reconcile_unverified_snapshots() {
    local extension dir backup source_name
    [ -n "$OFFSITE_DESTINATION" ] || return 0
    case "$DB_BACKEND" in
        sqlite|"") extension=db ;;
        postgres|postgresql) extension=pgdump ;;
        *) die "unsupported BLUEY_SERVER_DB_BACKEND=$DB_BACKEND" ;;
    esac
    for dir in "$BACKUP_DIR/hourly" "$BACKUP_DIR/daily"; do
        while IFS= read -r backup; do
            [ -n "$backup" ] || continue
            verified_marker_matches "$backup" && continue
            echo "reconciling unverified local snapshot: $backup" >&2
            source_name="$(sidecar_source_name "$backup")" ||
                die "unsafe source backup name in ${backup}.sha256"
            [ "$source_name" = "$(basename "$backup")" ] ||
                die "legacy remapped snapshot requires explicit --verify-existing: $backup"
            validate_snapshot "$backup"
            validate_local_pair "$backup"
            upload_and_verify "$backup"
        done < <(list_backend_backups_newest "$dir" "$extension")
    done
}

remove_proven_backup_pair() {
    local backup="$1"
    [ -n "$OFFSITE_DESTINATION" ] ||
        die "refusing capacity deletion without a configured offsite destination"
    # Re-read the local payload and the remote object immediately before the
    # destructive step. Cheap markers are selection hints, never deletion proof.
    validate_local_pair "$backup"
    reverify_offsite_pair "$backup" "$VALIDATED_LOCAL_BYTES" "$VALIDATED_LOCAL_SHA"
    verified_marker_matches "$backup" ||
        die "offsite proof refresh failed for $backup"
    remove_backup_pair "$backup"
}

verify_existing_snapshots() {
    [ -n "$OFFSITE_DESTINATION" ] ||
        die "--verify-existing requires OFFSITE_DESTINATION"
    case "$OFFSITE_DESTINATION" in
        s3://*) ;;
        *) die "--verify-existing currently requires an S3-compatible destination" ;;
    esac
    command -v aws >/dev/null 2>&1 || die "aws CLI is required for offsite verification"

    local backup expected_bytes expected_sha sidecar_sha source_path source_name remote verified_count=0
    while IFS= read -r backup; do
        [ -n "$backup" ] || continue
        validate_snapshot "$backup"
        if verified_marker_matches "$backup"; then
            continue
        fi
        [ -f "${backup}.sha256" ] || die "missing checksum sidecar for $backup"
        expected_sha="$(sha256_file "$backup")"
        expected_bytes="$(file_size_bytes "$backup")"
        sidecar_sha="$(awk 'NR == 1 {print $1}' "${backup}.sha256")"
        [ "$sidecar_sha" = "$expected_sha" ] ||
            die "local checksum sidecar mismatch for $backup"
        source_path="$(awk 'NR == 1 {print $NF}' "${backup}.sha256")"
        source_name="$(basename "${source_path#\*}")"
        case "$source_name" in
            bluey-*.db|bluey-postgres-*.pgdump) ;;
            *) die "unsafe source backup name in ${backup}.sha256" ;;
        esac
        [ "${source_name##*.}" = "${backup##*.}" ] ||
            die "source backup extension mismatch in ${backup}.sha256"
        remote="${OFFSITE_DESTINATION%/}/$source_name"
        verify_s3_pair "$backup" "$remote" "$expected_bytes" "$expected_sha"
        write_verified_marker "$backup" "$remote" legacy-flat \
            "$expected_bytes" "$expected_sha"
        verified_count=$((verified_count + 1))
        echo "verified existing local snapshot: $backup -> $remote"
    done < <(list_backups oldest)
    echo "$(date -u +%FT%TZ) existing backup verification ok: $verified_count marker(s) minted"
}

rotate_count() {
    local dir="$1" extension="$2" keep="$3" index=0 backup
    while IFS= read -r backup; do
        [ -n "$backup" ] || continue
        index=$((index + 1))
        if [ "$index" -gt "$keep" ]; then
            if { [ -n "$OFFSITE_DESTINATION" ] || [ "$REQUIRE_OFFSITE" = "1" ]; } &&
                ! verified_marker_matches "$backup"; then
                echo "count retention stopped: $backup lacks exact offsite verification" >&2
                return 1
            fi
            if [ -n "$OFFSITE_DESTINATION" ]; then
                remove_proven_backup_pair "$backup"
            else
                remove_backup_pair "$backup"
            fi
        fi
    done < <(list_backend_backups_newest "$dir" "$extension")
}

category_minimum() {
    case "$1" in
        "$BACKUP_DIR/hourly"/*) echo "$MIN_HOURLY_KEEP" ;;
        "$BACKUP_DIR/daily"/*) echo "$MIN_DAILY_KEEP" ;;
        *) return 1 ;;
    esac
}

category_count() {
    local backup="$1" dir extension
    dir="$(dirname "$backup")"
    extension="${backup##*.}"
    find "$dir" -maxdepth 1 -type f -name "*.${extension}" 2>/dev/null | wc -l | tr -d ' '
}

free_kb() {
    df -Pk "$BACKUP_DIR" | awk 'NR == 2 {print $4}'
}

capacity_exceeded() {
    local total available min_free_kb
    total="$(backup_hot_bytes)"
    available="$(free_kb)"
    min_free_kb=$((MIN_FREE_GB * 1024 * 1024))
    if [ "$LOCAL_MAX_BYTES" -gt 0 ] && [ "$total" -gt "$LOCAL_MAX_BYTES" ]; then
        return 0
    fi
    if [ "$MIN_FREE_GB" -gt 0 ] && [ "$available" -lt "$min_free_kb" ]; then
        return 0
    fi
    return 1
}

WRITER_ALLOCATION_COUNT=1

writer_capacity_exceeded() {
    local total available required_free_kb allocation_bytes
    total="$(backup_hot_bytes)"
    available="$(free_kb)"
    allocation_bytes=$((MAX_SNAPSHOT_BYTES * WRITER_ALLOCATION_COUNT))
    required_free_kb=$((MIN_FREE_GB * 1024 * 1024 + (allocation_bytes + 1023) / 1024))
    if [ "$LOCAL_MAX_BYTES" -gt 0 ] &&
        [ $((total + allocation_bytes)) -gt "$LOCAL_MAX_BYTES" ]; then
        return 0
    fi
    if [ "$available" -lt "$required_free_kb" ]; then
        return 0
    fi
    return 1
}

prune_for_capacity() {
    local predicate="${1:-capacity_exceeded}" candidate backup minimum count
    while "$predicate"; do
        candidate=""
        while IFS= read -r backup; do
            [ -n "$backup" ] || continue
            verified_marker_matches "$backup" || continue
            minimum="$(category_minimum "$backup")" || continue
            count="$(category_count "$backup")"
            if [ "$count" -gt "$minimum" ]; then
                candidate="$backup"
                break
            fi
        done < <(list_backups oldest)
        if [ -z "$candidate" ]; then
            echo "capacity prune stopped: no verified offsite snapshot can be removed without violating local minima" >&2
            return 1
        fi
        echo "capacity prune: removing verified local pair $candidate" >&2
        remove_proven_backup_pair "$candidate"
    done
}

remove_staging_run() {
    local path="$1"
    case "$path" in
        "$BACKUP_DIR/.staging"/run.*)
            [ -d "$path" ] && [ ! -L "$path" ] && rm -rf -- "$path"
            ;;
        *) die "refusing unexpected backup staging path $path" ;;
    esac
}

prune_stale_staging() {
    local path
    # Once the exclusive backup lock is held, every pre-existing exact run.*
    # directory is orphaned. Retaining one can consume the writer reserve and
    # force deletion of good restore points, so remove all of them before any
    # capacity decision.
    while IFS= read -r path; do
        [ -n "$path" ] || continue
        remove_staging_run "$path"
    done < <(
        find "$BACKUP_DIR/.staging" -mindepth 1 -maxdepth 1 -type d -name 'run.*' \
            -print 2>/dev/null
    )
    # With the exclusive host lock held, no marker temp belongs to a live run.
    find "$BACKUP_DIR/hourly" "$BACKUP_DIR/daily" -maxdepth 1 -type f \
        -name '*.offsite-verified.tmp.*' -print -delete 2>/dev/null || true
    find "$BACKUP_DIR/.staging" -mindepth 1 -maxdepth 1 -type f \
        -name '*.offsite-verified.tmp.*' -print -delete 2>/dev/null || true
}

BACKUP_RUN_ACTIVE=0
RUN_STARTED_EPOCH=0
RUN_SNAPSHOT_NAME=none
STAGING_RUN_DIR=""

write_backup_status() {
    local state="$1" exit_code="$2" now_epoch last_success status_tmp
    now_epoch="$(date -u +%s)"
    last_success=0
    if [ -r "$BACKUP_STATUS_FILE" ]; then
        last_success="$(awk -F= '$1 == "last_success_epoch" {print $2; exit}' \
            "$BACKUP_STATUS_FILE")"
        is_uint "$last_success" || last_success=0
    fi
    if [ "$state" = "ok" ]; then
        last_success="$now_epoch"
    fi
    status_tmp="$(mktemp "${BACKUP_STATUS_FILE}.tmp.XXXXXX")" || return 1
    {
        printf 'schema=1\n'
        printf 'status=%s\n' "$state"
        printf 'started_at_epoch=%s\n' "$RUN_STARTED_EPOCH"
        printf 'updated_at_epoch=%s\n' "$now_epoch"
        printf 'last_success_epoch=%s\n' "$last_success"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'backend=%s\n' "$DB_BACKEND"
        printf 'snapshot=%s\n' "$RUN_SNAPSHOT_NAME"
    } > "$status_tmp" || return 1
    chmod 600 "$status_tmp" || return 1
    mv "$status_tmp" "$BACKUP_STATUS_FILE"
}

backup_exit() {
    local status=$?
    trap - EXIT
    set +e
    if [ -n "$STAGING_RUN_DIR" ]; then
        case "$STAGING_RUN_DIR" in
            "$BACKUP_DIR/.staging"/run.*) rm -rf -- "$STAGING_RUN_DIR" ;;
        esac
    fi
    if [ "$BACKUP_RUN_ACTIVE" = "1" ]; then
        if [ "$status" -eq 0 ]; then
            write_backup_status ok 0 || [ "$REQUIRE_STATUS" = "0" ] || status=1
        else
            write_backup_status fail "$status" || true
        fi
    fi
    exit "$status"
}

if { [ "$DB_BACKEND" = "sqlite" ] || [ -z "$DB_BACKEND" ]; } &&
    [ "$MODE" != "--verify-existing" ]; then
    if [ "$MODE" = "--check-config" ]; then
        echo "Bluey database backup configuration is safe."
        exit 0
    fi
    [ -f "$DB_PATH" ] || die "no SQLite DB at $DB_PATH"
fi
if [ "$MODE" = "--check-config" ]; then
    echo "Bluey database backup configuration is safe."
    exit 0
fi

command -v flock >/dev/null 2>&1 || die "flock is required"
for backup_subdir in hourly daily .staging; do
    backup_subpath="$BACKUP_DIR/$backup_subdir"
    [ ! -L "$backup_subpath" ] || die "backup subdirectory must not be a symlink"
    [ ! -e "$backup_subpath" ] || [ -d "$backup_subpath" ] ||
        die "backup subdirectory path is not a directory"
done
trusted_directory_chain "$BACKUP_DIR/hourly" ||
    die "backup directory chain is not root-trusted and write-protected"
trusted_directory_chain "$BACKUP_DIR/daily" ||
    die "daily backup directory chain is not root-trusted and write-protected"
trusted_directory_chain "$BACKUP_DIR/.staging" ||
    die "backup staging directory chain is not root-trusted and write-protected"
mkdir -p "$BACKUP_DIR/hourly" "$BACKUP_DIR/daily" "$BACKUP_DIR/.staging"
chmod 0700 "$BACKUP_DIR" "$BACKUP_DIR/hourly" "$BACKUP_DIR/daily"
chmod 700 "$BACKUP_DIR/.staging"
trusted_directory_chain "$BACKUP_DIR/hourly" ||
    die "backup directory chain is not root-trusted and write-protected"
trusted_directory_chain "$BACKUP_DIR/daily" ||
    die "daily backup directory chain is not root-trusted and write-protected"
trusted_directory_chain "$BACKUP_DIR/.staging" ||
    die "backup staging directory chain is not root-trusted and write-protected"
trusted_lock_path "$BACKUP_DIR/.backup.lock" ||
    die "backup lock must be a trusted regular non-symlink file"
exec 9>>"$BACKUP_DIR/.backup.lock"
if [ "$LOCK_WAIT_SECONDS" -eq 0 ]; then
    flock -n 9 || die "another backup process holds $BACKUP_DIR/.backup.lock"
else
    flock -w "$LOCK_WAIT_SECONDS" 9 || die "timed out waiting for the backup lock"
fi
touch "$BACKUP_DIR/.backup.lock"
chmod 600 "$BACKUP_DIR/.backup.lock"
backup_status_basename="$(basename "$BACKUP_STATUS_FILE")"
if find "$BACKUP_DIR" -maxdepth 1 -type l -name "${backup_status_basename}.tmp.*" -print -quit |
    grep -q .; then
    die "refusing symlinked backup status temp"
fi
find "$BACKUP_DIR" -maxdepth 1 -type f -name "${backup_status_basename}.tmp.*" \
    -print -delete 2>/dev/null || true
prune_stale_staging

if [ "$MODE" = "--verify-existing" ]; then
    verify_existing_snapshots
    exit 0
fi

RUN_STARTED_EPOCH="$(date -u +%s)"
BACKUP_RUN_ACTIVE=1
trap backup_exit EXIT
write_backup_status running 0 || die "could not persist backup running status"

# A transient offsite outage must not create an ever-growing unverified
# backlog. Reconcile existing active-backend pairs before allocating a new dump.
reconcile_unverified_snapshots

# Capture one run timestamp before the capacity decision. At midnight the
# finalized hourly file and an in-progress daily copy coexist, so reserve two
# maximum-size snapshots unless today's daily already exists.
ts="$(date -u +%Y%m%dT%H%M%SZ)"
run_hour="${ts:9:2}"
run_day="${ts:0:8}"
daily_target=""
case "$DB_BACKEND" in
    sqlite|"")
        backup_ext=db
        hourly_target="$BACKUP_DIR/hourly/bluey-${ts}.db"
        potential_daily_target="$BACKUP_DIR/daily/bluey-${run_day}.db"
        ;;
    postgres|postgresql)
        backup_ext=pgdump
        hourly_target="$BACKUP_DIR/hourly/bluey-postgres-${ts}.pgdump"
        potential_daily_target="$BACKUP_DIR/daily/bluey-postgres-${run_day}.pgdump"
        ;;
    *) die "unsupported BLUEY_SERVER_DB_BACKEND=$DB_BACKEND" ;;
esac
if [ "$run_hour" = "00" ] && [ ! -e "$potential_daily_target" ]; then
    daily_target="$potential_daily_target"
    WRITER_ALLOCATION_COUNT=2
fi

# Reserve the configured post-write margin plus the maximum writer allocation.
# Only freshly re-proven offsite pairs may be removed to make that room.
prune_for_capacity writer_capacity_exceeded ||
    die "writer reserve is not recoverable by verified pruning"

STAGING_RUN_DIR="$(mktemp -d "$BACKUP_DIR/.staging/run.XXXXXX")"

case "$DB_BACKEND" in
    sqlite | "")
        staged_hourly="$STAGING_RUN_DIR/bluey-${ts}.db"
        run_with_snapshot_limit run_bounded "$COMMAND_TIMEOUT_SECONDS" sqlite3 "$DB_PATH" ".backup '$staged_hourly'" ||
            die "SQLite backup exceeded its writer limit or failed"
        ;;
    postgres | postgresql)
        : "${BLUEY_DATABASE_URL:?BLUEY_DATABASE_URL is required for Postgres backups}"
        command -v pg_dump >/dev/null 2>&1 ||
            die "BLUEY_SERVER_DB_BACKEND=postgres but pg_dump is not installed"
        if [ -n "${PGSSLROOTCERT:-}" ]; then
            export PGSSLROOTCERT
        elif [ -n "${BLUEY_POSTGRES_CA_CERT_PATH:-}" ]; then
            export PGSSLROOTCERT="$BLUEY_POSTGRES_CA_CERT_PATH"
        fi
        staged_hourly="$STAGING_RUN_DIR/bluey-postgres-${ts}.pgdump"
        (
            limit_blocks=$(((MAX_SNAPSHOT_BYTES + 1023) / 1024))
            ulimit -f "$limit_blocks" || exit 70
            export PGDATABASE="$BLUEY_DATABASE_URL"
            run_bounded "$COMMAND_TIMEOUT_SECONDS" pg_dump --format=custom --no-owner --no-acl --file "$staged_hourly"
        ) || die "PostgreSQL dump exceeded its writer limit or failed"
        ;;
    *) die "unsupported BLUEY_SERVER_DB_BACKEND=$DB_BACKEND" ;;
esac
[ -s "$staged_hourly" ] || die "database backup is empty"
enforce_snapshot_size "$staged_hourly"
finalize_pair "$staged_hourly" "$hourly_target"
RUN_SNAPSHOT_NAME="$(basename "$hourly_target")"

if [ -n "$daily_target" ]; then
    staged_daily="$STAGING_RUN_DIR/$(basename "$daily_target")"
    cp "$hourly_target" "$staged_daily"
    finalize_pair "$staged_daily" "$daily_target"
fi

# Retention never advances past a failed upload/read-back. The marker records
# the exact size/hash used by the capacity pruner; it contains no credential.
upload_and_verify "$hourly_target"
if [ -n "$daily_target" ]; then
    upload_and_verify "$daily_target"
fi

rotation_failed=0
rotate_count "$BACKUP_DIR/hourly" "$backup_ext" "$HOURLY_KEEP" || rotation_failed=1
rotate_count "$BACKUP_DIR/daily" "$backup_ext" "$DAILY_KEEP" || rotation_failed=1
[ "$rotation_failed" -eq 0 ] || die "count retention retained an unverified local snapshot"
prune_for_capacity || die "backup completed but the local capacity reserve remains unsafe"

echo "$(date -u +%FT%TZ) backup ok: $hourly_target ($(file_size_bytes "$hourly_target") bytes, backend=$DB_BACKEND)"
