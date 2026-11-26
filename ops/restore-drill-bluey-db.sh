#!/usr/bin/env bash
# Bluey restore drill script.
#
# Restores the newest local backup into an explicitly named disposable target
# and prints a small verification summary. PostgreSQL drills prove the live
# production and target database identities before any destructive restore.

set -Eeuo pipefail
set +x
umask 077

ENV_DIR="${BLUEY_ENV_DIR:-/etc/bluey-api}"
BLUEY_ENV_FILE="${BLUEY_ENV_FILE:-$ENV_DIR/bluey-api.env}"
BLUEY_STORAGE_ENV_FILE="${BLUEY_STORAGE_ENV_FILE:-$ENV_DIR/bluey-storage.env}"
BLUEY_POSTGRES_ENV_FILE="${BLUEY_POSTGRES_ENV_FILE:-$ENV_DIR/bluey-postgres.env}"

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

for env_file in "$BLUEY_ENV_FILE" "$BLUEY_STORAGE_ENV_FILE" \
    "$BLUEY_POSTGRES_ENV_FILE"; do
    if [ -f "$env_file" ]; then
        bootstrap_trusted_file "$env_file" || {
            echo "restore drill failed: environment file is not trusted and write-protected" >&2
            exit 1
        }
        # shellcheck disable=SC1090
        . "$env_file"
        set +x
    fi
done
unset -f bootstrap_stat bootstrap_trusted_file

BACKUP_DIR="${BLUEY_BACKUP_DIR:-/var/backups/bluey-api}"
BACKUP_FILE="${BLUEY_RESTORE_DRILL_BACKUP_FILE:-}"
DB_BACKEND="$(printf '%s' "${BLUEY_SERVER_DB_BACKEND:-sqlite}" | \
    tr '[:upper:]' '[:lower:]')"
BACKUP_EXTENSION=""
BACKEND_AUTHORITY=""
PRODUCTION_DATABASE_URL="${BLUEY_DATABASE_URL:-}"
TARGET_DATABASE_URL="${BLUEY_RESTORE_DRILL_DATABASE_URL:-}"
TARGET_DATABASE_NAME="${BLUEY_RESTORE_DRILL_DATABASE_NAME:-}"
DRILL_CONFIRMATION="${BLUEY_RESTORE_DRILL_CONFIRMATION:-}"
ALLOW_SAME_CLUSTER="${BLUEY_RESTORE_DRILL_ALLOW_SAME_CLUSTER:-0}"
SAME_CLUSTER_AUDIT_REF="${BLUEY_RESTORE_DRILL_SAME_CLUSTER_AUDIT_REF:-}"
DISPOSABLE_SENTINEL_TOKEN="${BLUEY_RESTORE_DRILL_SENTINEL_TOKEN:-}"
TARGET_EXPIRES_AT_EPOCH="${BLUEY_RESTORE_DRILL_TARGET_EXPIRES_AT_EPOCH:-}"
TEARDOWN_MODE="${BLUEY_RESTORE_DRILL_TEARDOWN_MODE:-}"
CONNECT_TIMEOUT_SECONDS="${BLUEY_RESTORE_DRILL_CONNECT_TIMEOUT_SECONDS:-10}"
RESTORE_TIMEOUT_SECONDS="${BLUEY_RESTORE_DRILL_RESTORE_TIMEOUT_SECONDS:-900}"
COMMAND_TIMEOUT_SECONDS="${BLUEY_RESTORE_DRILL_COMMAND_TIMEOUT_SECONDS:-60}"
LEASE_ACQUIRE_TIMEOUT_SECONDS="${BLUEY_RESTORE_DRILL_LEASE_ACQUIRE_TIMEOUT_SECONDS:-10}"
COPY_TIMEOUT_SECONDS="${BLUEY_RESTORE_DRILL_COPY_TIMEOUT_SECONDS:-300}"
MAX_BACKUP_BYTES="${BLUEY_RESTORE_DRILL_MAX_BACKUP_BYTES:-${BLUEY_BACKUP_MAX_SNAPSHOT_BYTES:-2147483648}}"
MIN_FREE_GB="${BLUEY_RESTORE_DRILL_MIN_FREE_GB:-${BLUEY_BACKUP_MIN_FREE_GB:-16}}"
REQUIRE_TRUSTED_PATHS="${BLUEY_RESTORE_DRILL_REQUIRE_TRUSTED_PATHS:-1}"
PRODUCTION_CLUSTER_SENTINEL="${BLUEY_RESTORE_DRILL_PRODUCTION_CLUSTER_SENTINEL:-}"
TARGET_CLUSTER_SENTINEL="${BLUEY_RESTORE_DRILL_TARGET_CLUSTER_SENTINEL:-}"
DRILL_AUTHORITY="${BLUEY_RESTORE_DRILL_AUTHORITY:-}"
DEADMAN_PROVIDER_IDENTITY="${BLUEY_RESTORE_DRILL_DEADMAN_PROVIDER_IDENTITY:-${BLUEY_RESTORE_DRILL_DEADMAN_AUTHORITY:-}}"
DEADMAN_MARKER_FILE="${BLUEY_RESTORE_DRILL_DEADMAN_MARKER_FILE:-}"
POSTGRES_CA_CERT_PATH="${BLUEY_POSTGRES_CA_CERT_PATH:-}"

# Keep connection strings out of the inherited environment. Each libpq client
# below selects a protected temporary service entry, never a credential-bearing
# connection string in argv.
unset BLUEY_DATABASE_URL BLUEY_RESTORE_DRILL_DATABASE_URL
export -n PRODUCTION_DATABASE_URL TARGET_DATABASE_URL 2>/dev/null || true

# Environment files may contain explicit `export` statements. Child processes
# receive only the small connection/runtime allowlist supplied at each call.
while IFS= read -r exported_name; do
    case "$exported_name" in
        PATH|HOME|TMPDIR|LANG|LC_ALL|LC_CTYPE|TZ) ;;
        *) export -n "$exported_name" 2>/dev/null || true ;;
    esac
done < <(compgen -e)

CATALOG_FILE=""
LIBPQ_SERVICE_FILE=""
LIBPQ_PASSFILE=""
RESTORE_ERROR_FILE=""
LIBPQ_ERROR_FILE=""
IDENTITY_FAILURE_FILE=""
CONTAINMENT_RUNNER_FILE=""
STABLE_WORK_DIR=""
STABLE_BACKUP_FILE=""
BACKUP_CHECKSUM=""
RESTORE_STARTED=0
TEARDOWN_COMPLETE=0
TARGET_IDENTITY_AUTHORITY=""
TARGET_ADMIN_SERVER_AUTHORITY=""
EXPECTED_SENTINEL=""
LAST_IDENTITY_FAILURE="unknown"
TARGET_LOCK_DIR=""
TARGET_LOCK_LEASE_FILE=""
TARGET_LOCK_HELD=0
TARGET_PROVIDER_LEASE_PID=""
TARGET_PROVIDER_LEASE_KEY=""
TARGET_PROVIDER_HOLDER_KEY=""
TARGET_PROVIDER_LEASE_HELD=0
STAT_STYLE=""

fail() {
    echo "restore drill failed: $*" >&2
    exit 1
}

cleanup() {
    local original_status=$? path teardown_failed=0 lock_key lock_value lock_owner=""
    trap - EXIT HUP INT TERM
    if [ "$RESTORE_STARTED" = "1" ] && [ "$TEARDOWN_COMPLETE" = "0" ]; then
        if teardown_target_database; then
            TEARDOWN_COMPLETE=1
        else
            teardown_failed=1
            echo "restore drill failed: disposable target teardown requires immediate operator action" >&2
        fi
    fi
    if [ -n "$TARGET_PROVIDER_LEASE_PID" ]; then
        kill -TERM "$TARGET_PROVIDER_LEASE_PID" >/dev/null 2>&1 || true
        wait "$TARGET_PROVIDER_LEASE_PID" 2>/dev/null || true
        TARGET_PROVIDER_LEASE_PID=""
        TARGET_PROVIDER_LEASE_HELD=0
    fi
    if [ "$TARGET_LOCK_HELD" = "1" ] && [ -n "$TARGET_LOCK_LEASE_FILE" ] &&
        [ -f "$TARGET_LOCK_LEASE_FILE" ]; then
        while IFS='=' read -r lock_key lock_value; do
            [ "$lock_key" = "token" ] && lock_owner="$lock_value"
        done < "$TARGET_LOCK_LEASE_FILE"
        if [ "$lock_owner" = "$DISPOSABLE_SENTINEL_TOKEN" ]; then
            rm -f -- "$TARGET_LOCK_LEASE_FILE" >/dev/null 2>&1 || true
            rmdir -- "$TARGET_LOCK_DIR" >/dev/null 2>&1 || true
        fi
    elif [ "$TARGET_LOCK_HELD" = "1" ] && [ -n "$TARGET_LOCK_DIR" ]; then
        rmdir -- "$TARGET_LOCK_DIR" >/dev/null 2>&1 || true
    fi
    for path in "$CATALOG_FILE" "$LIBPQ_SERVICE_FILE" "$LIBPQ_PASSFILE" \
        "$RESTORE_ERROR_FILE" "$LIBPQ_ERROR_FILE" "$IDENTITY_FAILURE_FILE" \
        "$CONTAINMENT_RUNNER_FILE"; do
        if [ -n "$path" ]; then
            rm -f -- "$path" >/dev/null 2>&1 || true
        fi
    done
    if [ -n "$STABLE_WORK_DIR" ]; then
        rm -f -- "$STABLE_BACKUP_FILE" >/dev/null 2>&1 || true
        rmdir -- "$STABLE_WORK_DIR" >/dev/null 2>&1 || true
    fi
    if [ "$original_status" = "0" ] && [ "$teardown_failed" = "1" ]; then
        original_status=1
    fi
    exit "$original_status"
}

trap cleanup EXIT
trap 'exit 130' HUP INT TERM

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || fail "$1 is required"
}

is_uint() {
    [[ "$1" =~ ^[0-9]+$ ]]
}

file_size_bytes() {
    case "$STAT_STYLE" in
        gnu) timeout "${COMMAND_TIMEOUT_SECONDS}s" stat -c%s -- "$1" ;;
        bsd) timeout "${COMMAND_TIMEOUT_SECONDS}s" stat -f%z -- "$1" ;;
        *) return 1 ;;
    esac
}

file_owner_uid() {
    case "$STAT_STYLE" in
        gnu) timeout "${COMMAND_TIMEOUT_SECONDS}s" stat -c%u -- "$1" ;;
        bsd) timeout "${COMMAND_TIMEOUT_SECONDS}s" stat -f%u -- "$1" ;;
        *) return 1 ;;
    esac
}

file_mode() {
    case "$STAT_STYLE" in
        gnu) timeout "${COMMAND_TIMEOUT_SECONDS}s" stat -c%a -- "$1" ;;
        bsd) timeout "${COMMAND_TIMEOUT_SECONDS}s" stat -f%Lp -- "$1" ;;
        *) return 1 ;;
    esac
}

detect_stat_style() {
    if timeout "${COMMAND_TIMEOUT_SECONDS}s" stat -c%s -- "$0" >/dev/null 2>&1; then
        STAT_STYLE=gnu
    elif timeout "${COMMAND_TIMEOUT_SECONDS}s" stat -f%z -- "$0" >/dev/null 2>&1; then
        STAT_STYLE=bsd
    else
        fail "supported stat implementation is required"
    fi
}

trusted_directory_chain() {
    local path="$1" remainder component current="" owner mode mode_value effective_uid
    [ "$REQUIRE_TRUSTED_PATHS" = "1" ] || return 0
    effective_uid="$(id -u)"
    remainder="${path#/}"
    while [ -n "$remainder" ]; do
        component="${remainder%%/*}"
        if [ "$remainder" = "$component" ]; then
            remainder=""
        else
            remainder="${remainder#*/}"
        fi
        [ -n "$component" ] || continue
        current="$current/$component"
        [ -d "$current" ] && [ ! -L "$current" ] || return 1
        owner="$(file_owner_uid "$current")" || return 1
        if [ "$effective_uid" = "0" ]; then
            [ "$owner" = "0" ] || return 1
        else
            { [ "$owner" = "0" ] || [ "$owner" = "$effective_uid" ]; } || return 1
        fi
        mode="$(file_mode "$current")" || return 1
        is_uint "$mode" || return 1
        mode_value=$((8#$mode))
        [ $((mode_value & 8#022)) -eq 0 ] || return 1
    done
}

trusted_regular_file() {
    local path="$1" effective_uid owner mode mode_value
    [ -f "$path" ] && [ ! -L "$path" ] || return 1
    [ "$REQUIRE_TRUSTED_PATHS" = "1" ] || return 0
    effective_uid="$(id -u)"
    owner="$(file_owner_uid "$path")" || return 1
    if [ "$effective_uid" = "0" ]; then
        [ "$owner" = "0" ] || return 1
    else
        { [ "$owner" = "0" ] || [ "$owner" = "$effective_uid" ]; } || return 1
    fi
    mode="$(file_mode "$path")" || return 1
    is_uint "$mode" || return 1
    mode_value=$((8#$mode))
    [ $((mode_value & 8#022)) -eq 0 ]
}

available_bytes() {
    local path="$1" df_output line available_kib="" ignored
    df_output="$(timeout "${COMMAND_TIMEOUT_SECONDS}s" df -Pk -- "$path")" || return 1
    while IFS= read -r line; do
        if [ -n "$line" ]; then
            read -r ignored ignored ignored available_kib ignored <<< "$line"
        fi
    done <<< "$df_output"
    is_uint "$available_kib" || return 1
    printf '%s\n' $((available_kib * 1024))
}

latest_backup() {
    ls -1t "$BACKUP_DIR/hourly"/*."$BACKUP_EXTENSION" 2>/dev/null | head -n 1 || true
}

resolve_backend_extension() {
    case "$DB_BACKEND" in
        sqlite|"") BACKUP_EXTENSION="db"; BACKEND_AUTHORITY="sqlite" ;;
        postgres|postgresql) BACKUP_EXTENSION="pgdump"; BACKEND_AUTHORITY="postgres" ;;
        *) fail "unsupported BLUEY_SERVER_DB_BACKEND" ;;
    esac
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        timeout "${COMMAND_TIMEOUT_SECONDS}s" sha256sum -- "$1" 2>/dev/null | awk '{print $1}'
    elif command -v shasum >/dev/null 2>&1; then
        timeout "${COMMAND_TIMEOUT_SECONDS}s" shasum -a 256 -- "$1" 2>/dev/null | awk '{print $1}'
    else
        return 1
    fi
}

is_safe_backup_name() {
    local backup_name="$1"
    case "$backup_name" in
        bluey-postgres-*.pgdump)
            [[ "$backup_name" =~ ^bluey-postgres-[A-Za-z0-9][A-Za-z0-9._-]*\.pgdump$ ]]
            ;;
        bluey-*.db)
            [[ "$backup_name" =~ ^bluey-[A-Za-z0-9][A-Za-z0-9._-]*\.db$ ]]
            ;;
        *) return 1 ;;
    esac
}

validate_backup_path() {
    local backup_name backup_parent backup_size
    case "$BACKUP_FILE" in
        /*) ;;
        *) fail "BLUEY_RESTORE_DRILL_BACKUP_FILE must be an absolute path" ;;
    esac
    case "$BACKUP_FILE/" in
        *$'\n'*|*$'\r'*|*//*|*/./*|*/../*)
            fail "BLUEY_RESTORE_DRILL_BACKUP_FILE contains an unsafe path segment"
            ;;
    esac
    [ -f "$BACKUP_FILE" ] || fail "backup file does not exist: $BACKUP_FILE"
    [ ! -L "$BACKUP_FILE" ] || fail "backup file must not be a symbolic link"
    backup_parent="${BACKUP_FILE%/*}"
    case "$backup_parent" in
        "$BACKUP_DIR/hourly"|"$BACKUP_DIR/daily") ;;
        *) fail "backup file must be directly inside the configured hourly or daily directory" ;;
    esac
    trusted_directory_chain "$backup_parent" ||
        fail "backup directory chain is not trusted and write-protected"
    trusted_regular_file "$BACKUP_FILE" ||
        fail "backup file is not trusted and write-protected"
    backup_name="${BACKUP_FILE##*/}"
    is_safe_backup_name "$backup_name" || fail "backup file name is outside the Bluey policy"
    [ "${backup_name##*.}" = "$BACKUP_EXTENSION" ] ||
        fail "backup extension does not match BLUEY_SERVER_DB_BACKEND"
    backup_size="$(file_size_bytes "$BACKUP_FILE")" || fail "backup size could not be read"
    is_uint "$backup_size" && [ "$backup_size" -gt 0 ] || fail "backup file is empty"
    [ "$backup_size" -le "$MAX_BACKUP_BYTES" ] ||
        fail "backup exceeds BLUEY_RESTORE_DRILL_MAX_BACKUP_BYTES"
}

sidecar_source_matches_backup() {
    local backup_name="$1" source_name="$2" backup_date
    [ "$source_name" = "$backup_name" ] && return 0

    # Before daily checksums were rewritten at finalization, a daily snapshot
    # retained the exact same-day hourly source name in its checksum sidecar.
    if [[ "$backup_name" =~ ^bluey-postgres-([0-9]{8})\.pgdump$ ]]; then
        backup_date="${BASH_REMATCH[1]}"
        [[ "$source_name" =~ ^bluey-postgres-${backup_date}T[0-9]{6}Z\.pgdump$ ]]
        return
    fi
    if [[ "$backup_name" =~ ^bluey-([0-9]{8})\.db$ ]]; then
        backup_date="${BASH_REMATCH[1]}"
        [[ "$source_name" =~ ^bluey-${backup_date}T[0-9]{6}Z\.db$ ]]
        return
    fi
    return 1
}

verify_backup_checksum() {
    local sidecar="${BACKUP_FILE}.sha256"
    local actual_sha backup_name expected_sha extra line nonempty_lines=0
    local source_name source_path

    [ -f "$sidecar" ] || fail "backup checksum sidecar is missing"
    [ -r "$sidecar" ] || fail "backup checksum sidecar is not readable"
    [ ! -L "$sidecar" ] || fail "backup checksum sidecar must not be a symbolic link"
    trusted_regular_file "$sidecar" ||
        fail "backup checksum sidecar is not trusted and write-protected"
    expected_sha=""
    source_path=""
    while IFS= read -r line || [ -n "$line" ]; do
        [ -n "${line//[[:space:]]/}" ] || continue
        nonempty_lines=$((nonempty_lines + 1))
        [ "$nonempty_lines" -eq 1 ] ||
            fail "backup checksum sidecar has an invalid format"
        read -r expected_sha source_path extra <<< "$line"
        [ -n "$expected_sha" ] && [ -n "$source_path" ] && [ -z "${extra:-}" ] ||
            fail "backup checksum sidecar has an invalid format"
    done < "$sidecar"
    [ "$nonempty_lines" -eq 1 ] || fail "backup checksum sidecar has an invalid format"
    [[ "$expected_sha" =~ ^[0-9a-f]{64}$ ]] ||
        fail "backup checksum sidecar has an invalid SHA-256"
    source_path="${source_path#\*}"
    source_name="${source_path##*/}"
    backup_name="${BACKUP_FILE##*/}"
    is_safe_backup_name "$source_name" ||
        fail "backup checksum sidecar has an unsafe source name"
    [ "${source_name##*.}" = "${backup_name##*.}" ] ||
        fail "backup checksum sidecar source extension does not match"
    sidecar_source_matches_backup "$backup_name" "$source_name" ||
        fail "backup checksum sidecar names an unrelated backup"

    actual_sha="$(sha256_file "$BACKUP_FILE")" || fail "SHA-256 tooling failed"
    [ "$actual_sha" = "$expected_sha" ] || fail "backup checksum does not match"
    BACKUP_CHECKSUM="$actual_sha"
}

create_stable_backup_copy() {
    local available_after available_before backup_size min_free_bytes stable_sha source_sha
    backup_size="$(file_size_bytes "$BACKUP_FILE")" || fail "backup size could not be read"
    min_free_bytes=$((MIN_FREE_GB * 1024 * 1024 * 1024))
    STABLE_WORK_DIR="$(mktemp -d "${TMPDIR:-/tmp}/bluey-restore-drill-copy.XXXXXX")" ||
        fail "could not allocate protected restore staging"
    chmod 700 "$STABLE_WORK_DIR" || fail "could not protect restore staging"
    available_before="$(available_bytes "$STABLE_WORK_DIR")" ||
        fail "restore staging capacity could not be measured"
    [ "$available_before" -ge $((backup_size + min_free_bytes)) ] ||
        fail "restore staging lacks the configured post-copy free-space reserve"
    STABLE_BACKUP_FILE="$STABLE_WORK_DIR/backup.$BACKUP_EXTENSION"
    timeout --signal=TERM --kill-after=10s "${COPY_TIMEOUT_SECONDS}s" \
        cp -- "$BACKUP_FILE" "$STABLE_BACKUP_FILE" || fail "protected backup copy failed"
    chmod 600 "$STABLE_BACKUP_FILE" || fail "could not protect the stable backup copy"
    [ ! -L "$STABLE_BACKUP_FILE" ] && [ -f "$STABLE_BACKUP_FILE" ] ||
        fail "stable backup copy is not a regular file"
    stable_sha="$(sha256_file "$STABLE_BACKUP_FILE")" ||
        fail "stable backup copy checksum failed"
    source_sha="$(sha256_file "$BACKUP_FILE")" || fail "source backup recheck failed"
    [ "$stable_sha" = "$BACKUP_CHECKSUM" ] && [ "$source_sha" = "$BACKUP_CHECKSUM" ] ||
        fail "backup changed while creating its protected copy"
    available_after="$(available_bytes "$STABLE_WORK_DIR")" ||
        fail "post-copy restore capacity could not be measured"
    [ "$available_after" -ge "$min_free_bytes" ] ||
        fail "protected backup copy consumed the configured free-space reserve"
}

validate_global_guard() {
    case "$BACKUP_DIR" in
        /*) ;;
        *) fail "BLUEY_BACKUP_DIR must be an absolute path" ;;
    esac
    case "$BACKUP_DIR/" in
        *$'\n'*|*$'\r'*|*//*|*/./*|*/../*)
            fail "BLUEY_BACKUP_DIR contains an unsafe path segment"
            ;;
    esac
    is_uint "$MAX_BACKUP_BYTES" && [ "$MAX_BACKUP_BYTES" -ge 1 ] ||
        fail "BLUEY_RESTORE_DRILL_MAX_BACKUP_BYTES must be a positive integer"
    is_uint "$MIN_FREE_GB" ||
        fail "BLUEY_RESTORE_DRILL_MIN_FREE_GB must be an unsigned integer"
    is_uint "$COMMAND_TIMEOUT_SECONDS" && [ "$COMMAND_TIMEOUT_SECONDS" -ge 1 ] &&
        [ "$COMMAND_TIMEOUT_SECONDS" -le 600 ] ||
        fail "BLUEY_RESTORE_DRILL_COMMAND_TIMEOUT_SECONDS must be between 1 and 600"
    is_uint "$COPY_TIMEOUT_SECONDS" && [ "$COPY_TIMEOUT_SECONDS" -ge 1 ] &&
        [ "$COPY_TIMEOUT_SECONDS" -le 3600 ] ||
        fail "BLUEY_RESTORE_DRILL_COPY_TIMEOUT_SECONDS must be between 1 and 3600"
    is_uint "$LEASE_ACQUIRE_TIMEOUT_SECONDS" &&
        [ "$LEASE_ACQUIRE_TIMEOUT_SECONDS" -ge 1 ] &&
        [ "$LEASE_ACQUIRE_TIMEOUT_SECONDS" -le 60 ] ||
        fail "BLUEY_RESTORE_DRILL_LEASE_ACQUIRE_TIMEOUT_SECONDS must be between 1 and 60"
    case "$REQUIRE_TRUSTED_PATHS" in
        0|1) ;;
        *) fail "BLUEY_RESTORE_DRILL_REQUIRE_TRUSTED_PATHS must be 0 or 1" ;;
    esac
    if [ "$REQUIRE_TRUSTED_PATHS" != "1" ]; then
        fail "production restore drills require trusted backup paths"
    fi
    trusted_directory_chain "$BACKUP_DIR" ||
        fail "BLUEY_BACKUP_DIR is not trusted and write-protected"
}

create_libpq_service_file() {
    LIBPQ_SERVICE_FILE="$(mktemp \
        "${TMPDIR:-/tmp}/bluey-restore-drill-libpq.XXXXXX")" ||
        fail "could not allocate a temporary libpq service file"
    chmod 600 "$LIBPQ_SERVICE_FILE" || fail "could not protect the libpq service file"
    LIBPQ_PASSFILE="$(mktemp \
        "${TMPDIR:-/tmp}/bluey-restore-drill-pgpass.XXXXXX")" ||
        fail "could not allocate a temporary libpq password file"
    chmod 600 "$LIBPQ_PASSFILE" || fail "could not protect the libpq password file"
    IDENTITY_FAILURE_FILE="$(mktemp \
        "${TMPDIR:-/tmp}/bluey-restore-drill-identity-failure.XXXXXX")" ||
        fail "could not allocate a temporary identity status file"
    chmod 600 "$IDENTITY_FAILURE_FILE" || fail "could not protect identity status"

    if ! env -i PATH="$PATH" HOME="${HOME:-/}" TMPDIR="${TMPDIR:-/tmp}" \
        BLUEY_LIBPQ_CONNECT_TIMEOUT="$CONNECT_TIMEOUT_SECONDS" \
        BLUEY_LIBPQ_SERVICE_FILE="$LIBPQ_SERVICE_FILE" \
        BLUEY_LIBPQ_PASSFILE="$LIBPQ_PASSFILE" \
        timeout --signal=TERM --kill-after=5s "${COMMAND_TIMEOUT_SECONDS}s" \
        python3 3<<<"$PRODUCTION_DATABASE_URL" 4<<<"$TARGET_DATABASE_URL" \
        <<'PY' 2>/dev/null
import os
import re
from urllib.parse import parse_qsl, unquote_to_bytes, urlsplit


def decode(raw: str, field: str) -> str:
    if re.search(r"%(?![0-9A-Fa-f]{2})", raw):
        raise ValueError(f"malformed percent escape in {field}")
    value = unquote_to_bytes(raw).decode("utf-8", "strict")
    if not value or any(character in value for character in "\x00\r\n"):
        raise ValueError(f"unsafe {field}")
    return value


def require_service_atom(value: str, field: str, pattern: str) -> str:
    if not re.fullmatch(pattern, value):
        raise ValueError(f"unsafe {field}")
    return value


def pgpass_escape(value: str) -> str:
    return value.replace("\\", "\\\\").replace(":", "\\:")


def parse_uri(uri: str) -> dict[str, str]:
    parsed = urlsplit(uri)
    if parsed.scheme not in {"postgres", "postgresql"} or parsed.fragment:
        raise ValueError("unsupported PostgreSQL URI")
    if not parsed.hostname or "," in parsed.hostname:
        raise ValueError("a single network host is required")
    host = parsed.hostname
    if not re.fullmatch(r"[A-Za-z0-9._:-]+", host):
        raise ValueError("unsafe PostgreSQL host")
    try:
        port = parsed.port
    except ValueError as error:
        raise ValueError("invalid PostgreSQL port") from error
    username = require_service_atom(
        decode(parsed.username or "", "username"), "username", r"[A-Za-z0-9_.@+-]{1,128}"
    )
    password = decode(parsed.password or "", "password")
    if not parsed.path.startswith("/") or parsed.path == "/":
        raise ValueError("database name is required")
    database = require_service_atom(
        decode(parsed.path[1:], "database name"),
        "database name",
        r"[A-Za-z0-9_][A-Za-z0-9_.-]{0,127}",
    )

    options: dict[str, str] = {}
    for key, value in parse_qsl(parsed.query, keep_blank_values=True, strict_parsing=True):
        if key in options or key not in {"sslmode", "channel_binding", "target_session_attrs"}:
            raise ValueError("unsupported or repeated PostgreSQL URI option")
        options[key] = value
    if "sslmode" in options and options["sslmode"] not in {
        "disable", "allow", "prefer", "require", "verify-ca", "verify-full"
    }:
        raise ValueError("invalid sslmode")
    if "channel_binding" in options and options["channel_binding"] not in {
        "disable", "prefer", "require"
    }:
        raise ValueError("invalid channel_binding")
    if "target_session_attrs" in options and options["target_session_attrs"] not in {
        "any", "read-write", "read-only", "primary", "standby", "prefer-standby"
    }:
        raise ValueError("invalid target_session_attrs")

    result = {
        "host": host,
        "user": username,
        "dbname": database,
        "connect_timeout": os.environ["BLUEY_LIBPQ_CONNECT_TIMEOUT"],
    }
    result["port"] = str(port if port is not None else 5432)
    result.update(options)
    return result, password


production_uri = os.fdopen(3, "r", encoding="utf-8").read()
target_uri = os.fdopen(4, "r", encoding="utf-8").read()
if not production_uri.endswith("\n") or not target_uri.endswith("\n"):
    raise ValueError("connection URI input was truncated")
production_service, production_password = parse_uri(production_uri[:-1])
target_service, target_password = parse_uri(target_uri[:-1])
target_admin_service = dict(target_service)
target_admin_service["dbname"] = "postgres"
services = {
    "bluey_restore_production": production_service,
    "bluey_restore_target": target_service,
    "bluey_restore_target_admin": target_admin_service,
}

lines: list[str] = []
for service_name, values in services.items():
    lines.append(f"[{service_name}]")
    for key, value in values.items():
        lines.append(f"{key}={value}")
    lines.append("")

service_path = os.environ["BLUEY_LIBPQ_SERVICE_FILE"]
with open(service_path, "w", encoding="utf-8", newline="\n") as service_file:
    service_file.write("\n".join(lines))
os.chmod(service_path, 0o600)

password_lines = []
for values, password in (
    (production_service, production_password),
    (target_service, target_password),
    (target_admin_service, target_password),
):
    password_lines.append(":".join(pgpass_escape(values[key]) for key in (
        "host", "port", "dbname", "user"
    )) + ":" + pgpass_escape(password))
pass_path = os.environ["BLUEY_LIBPQ_PASSFILE"]
with open(pass_path, "w", encoding="utf-8", newline="\n") as password_file:
    password_file.write("\n".join(password_lines) + "\n")
os.chmod(pass_path, 0o600)
PY
    then
        fail "PostgreSQL connection URLs could not be converted to a safe libpq service"
    fi

    # Only the protected service file is needed after parsing. Keep both raw
    # connection strings out of all subsequent child environments and argv.
    PRODUCTION_DATABASE_URL=""
    TARGET_DATABASE_URL=""
}

create_containment_runner() {
    CONTAINMENT_RUNNER_FILE="$(mktemp \
        "${TMPDIR:-/tmp}/bluey-restore-drill-contained.XXXXXX")" ||
        fail "could not allocate the child-containment runner"
    chmod 600 "$CONTAINMENT_RUNNER_FILE" || fail "could not protect child containment"
    cat >"$CONTAINMENT_RUNNER_FILE" <<'PY'
import ctypes
import os
import signal
import subprocess
import sys
import threading
import time

parent = os.getppid()
limit = int(sys.argv[1])
command = sys.argv[2:]

def parent_death_signal():
    if sys.platform.startswith("linux"):
        libc = ctypes.CDLL(None, use_errno=True)
        if libc.prctl(1, signal.SIGKILL, 0, 0, 0) != 0:
            raise OSError(ctypes.get_errno(), "prctl(PR_SET_PDEATHSIG) failed")

parent_death_signal()
if os.getppid() != parent:
    os.kill(os.getpid(), signal.SIGKILL)

child_env = dict(os.environ)
child_env["BLUEY_RESTORE_CONTAINED"] = "1"
child = subprocess.Popen(command, start_new_session=True, env=child_env,
                         preexec_fn=parent_death_signal if sys.platform.startswith("linux") else None)

def terminate(_signum=None, _frame=None):
    try:
        os.killpg(child.pid, signal.SIGTERM)
    except ProcessLookupError:
        return

for caught in (signal.SIGHUP, signal.SIGINT, signal.SIGTERM):
    signal.signal(caught, terminate)

def watch_parent():
    while child.poll() is None:
        if os.getppid() != parent:
            terminate()
            time.sleep(1)
            try:
                os.killpg(child.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            return
        time.sleep(0.2)

threading.Thread(target=watch_parent, daemon=True).start()
try:
    status = child.wait(timeout=limit)
except subprocess.TimeoutExpired:
    terminate()
    try:
        status = child.wait(timeout=10)
    except subprocess.TimeoutExpired:
        try:
            os.killpg(child.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        child.wait()
        status = 124
sys.exit(status if status >= 0 else 128 - status)
PY
}

validate_target_guard() {
    local expected_confirmation minimum_horizon now_epoch

    [ -n "$PRODUCTION_DATABASE_URL" ] ||
        fail "BLUEY_DATABASE_URL is required to prove the production database identity"
    [ -n "$TARGET_DATABASE_URL" ] ||
        fail "BLUEY_RESTORE_DRILL_DATABASE_URL is required for PostgreSQL drills"
    [ -n "$TARGET_DATABASE_NAME" ] ||
        fail "BLUEY_RESTORE_DRILL_DATABASE_NAME is required for PostgreSQL drills"
    [ "${#TARGET_DATABASE_NAME}" -le 63 ] ||
        fail "BLUEY_RESTORE_DRILL_DATABASE_NAME exceeds PostgreSQL's identifier limit"
    [[ "$TARGET_DATABASE_NAME" =~ ^bluey_restore_drill(_[a-z0-9]+)*$ ]] ||
        fail "BLUEY_RESTORE_DRILL_DATABASE_NAME must start with bluey_restore_drill"
    case "$ALLOW_SAME_CLUSTER" in
        0|1) ;;
        *) fail "BLUEY_RESTORE_DRILL_ALLOW_SAME_CLUSTER must be 0 or 1" ;;
    esac
    is_uint "$CONNECT_TIMEOUT_SECONDS" && [ "$CONNECT_TIMEOUT_SECONDS" -ge 1 ] &&
        [ "$CONNECT_TIMEOUT_SECONDS" -le 60 ] ||
        fail "BLUEY_RESTORE_DRILL_CONNECT_TIMEOUT_SECONDS must be between 1 and 60"
    is_uint "$RESTORE_TIMEOUT_SECONDS" && [ "$RESTORE_TIMEOUT_SECONDS" -ge 1 ] &&
        [ "$RESTORE_TIMEOUT_SECONDS" -le 7200 ] ||
        fail "BLUEY_RESTORE_DRILL_RESTORE_TIMEOUT_SECONDS must be between 1 and 7200"
    # Reserve every bounded PostgreSQL proof (including failure teardown) plus
    # five minutes of scheduling margin; the restore itself has its own bound.
    minimum_horizon=$((RESTORE_TIMEOUT_SECONDS + 32 * COMMAND_TIMEOUT_SECONDS + 300))
    [[ "$DISPOSABLE_SENTINEL_TOKEN" =~ ^[0-9A-Fa-f]{32,128}$ ]] ||
        fail "BLUEY_RESTORE_DRILL_SENTINEL_TOKEN must be 32 to 128 hexadecimal characters"
    is_uint "$TARGET_EXPIRES_AT_EPOCH" ||
        fail "BLUEY_RESTORE_DRILL_TARGET_EXPIRES_AT_EPOCH must be an epoch timestamp"
    now_epoch="$(date -u +%s)"
    [ "$TARGET_EXPIRES_AT_EPOCH" -ge $((now_epoch + minimum_horizon)) ] &&
        [ "$TARGET_EXPIRES_AT_EPOCH" -le $((now_epoch + 86400)) ] ||
        fail "restore-target expiry must cover restore, verification, and teardown bounds"
    [ "$TEARDOWN_MODE" = "drop" ] ||
        fail "BLUEY_RESTORE_DRILL_TEARDOWN_MODE must equal drop"
    [[ "$PRODUCTION_CLUSTER_SENTINEL" =~ ^[A-Za-z0-9][A-Za-z0-9._:-]{15,127}$ ]] ||
        fail "BLUEY_RESTORE_DRILL_PRODUCTION_CLUSTER_SENTINEL is invalid"
    [[ "$TARGET_CLUSTER_SENTINEL" =~ ^[A-Za-z0-9][A-Za-z0-9._:-]{15,127}$ ]] ||
        fail "BLUEY_RESTORE_DRILL_TARGET_CLUSTER_SENTINEL is invalid"
    [ "${#DRILL_AUTHORITY}" -le 128 ] &&
        [[ "$DRILL_AUTHORITY" =~ ^[A-Za-z0-9][A-Za-z0-9._:/-]{2,127}$ ]] ||
        fail "BLUEY_RESTORE_DRILL_AUTHORITY requires a safe drill authority"
    [ "${#DEADMAN_PROVIDER_IDENTITY}" -le 128 ] &&
        [[ "$DEADMAN_PROVIDER_IDENTITY" =~ ^[A-Za-z0-9][A-Za-z0-9._:/-]{2,127}$ ]] ||
        fail "BLUEY_RESTORE_DRILL_DEADMAN_PROVIDER_IDENTITY requires a safe provider identity"
    case "$DEADMAN_MARKER_FILE" in
        /*) ;;
        *) fail "BLUEY_RESTORE_DRILL_DEADMAN_MARKER_FILE must be an absolute path" ;;
    esac

    expected_confirmation="restore:$TARGET_DATABASE_NAME"
    [ "$DRILL_CONFIRMATION" = "$expected_confirmation" ] ||
        fail "BLUEY_RESTORE_DRILL_CONFIRMATION must equal $expected_confirmation"
}

validate_deadman_marker() {
    local expected marker_line="" marker_parent line line_count=0
    marker_parent="${DEADMAN_MARKER_FILE%/*}"
    [ "$marker_parent" = "$BACKUP_DIR/deadman" ] ||
        fail "dead-man marker must be directly inside BLUEY_BACKUP_DIR/deadman"
    trusted_directory_chain "$marker_parent" ||
        fail "dead-man marker directory is not trusted and write-protected"
    trusted_regular_file "$DEADMAN_MARKER_FILE" ||
        fail "dead-man marker is not trusted and write-protected"
    while IFS= read -r line || [ -n "$line" ]; do
        line_count=$((line_count + 1))
        [ "$line_count" -eq 1 ] || fail "dead-man marker has unexpected extra content"
        marker_line="$line"
    done < "$DEADMAN_MARKER_FILE"
    [ "$line_count" -eq 1 ] || fail "dead-man marker could not be read"
    expected="bluey-restore-drill-deadman:v1:$TARGET_CLUSTER_SENTINEL:$TARGET_DATABASE_NAME:$TARGET_EXPIRES_AT_EPOCH:$DISPOSABLE_SENTINEL_TOKEN:$DRILL_AUTHORITY:$DEADMAN_PROVIDER_IDENTITY"
    [ "$marker_line" = "$expected" ] ||
        fail "external dead-man cleanup attestation does not match the drill target"
}

consume_deadman_lease_marker() {
    local expected line="" line_count=0 value
    trusted_regular_file "$DEADMAN_MARKER_FILE" || return 1
    while IFS= read -r value || [ -n "$value" ]; do
        line_count=$((line_count + 1))
        [ "$line_count" -eq 1 ] || return 1
        line="$value"
    done < "$DEADMAN_MARKER_FILE"
    expected="bluey-restore-drill-deadman:v1:$TARGET_CLUSTER_SENTINEL:$TARGET_DATABASE_NAME:$TARGET_EXPIRES_AT_EPOCH:$DISPOSABLE_SENTINEL_TOKEN:$DRILL_AUTHORITY:$DEADMAN_PROVIDER_IDENTITY"
    [ "$line_count" -eq 1 ] && [ "$line" = "$expected" ] || return 1
    rm -f -- "$DEADMAN_MARKER_FILE" || return 1
    [ ! -e "$DEADMAN_MARKER_FILE" ]
}

acquire_target_lease() {
    local lock_root="$BACKUP_DIR/.restore-drill-locks"
    [ ! -L "$lock_root" ] || fail "restore-drill lock root must not be a symlink"
    if [ ! -d "$lock_root" ]; then
        mkdir -- "$lock_root" || fail "could not create restore-drill lock root"
        chmod 700 "$lock_root" || fail "could not protect restore-drill lock root"
    fi
    trusted_directory_chain "$lock_root" ||
        fail "restore-drill lock root is not trusted and write-protected"
    TARGET_LOCK_DIR="$lock_root/$TARGET_DATABASE_NAME.lock"
    mkdir -- "$TARGET_LOCK_DIR" 2>/dev/null ||
        fail "another restore drill already holds the target lease"
    TARGET_LOCK_HELD=1
    chmod 700 "$TARGET_LOCK_DIR" || fail "could not protect the target lease"
    TARGET_LOCK_LEASE_FILE="$TARGET_LOCK_DIR/lease"
    {
        printf 'schema=1\n'
        printf 'database=%s\n' "$TARGET_DATABASE_NAME"
        printf 'expires_at_epoch=%s\n' "$TARGET_EXPIRES_AT_EPOCH"
        printf 'token=%s\n' "$DISPOSABLE_SENTINEL_TOKEN"
        printf 'drill_authority=%s\n' "$DRILL_AUTHORITY"
        printf 'deadman_provider_identity=%s\n' "$DEADMAN_PROVIDER_IDENTITY"
    } > "$TARGET_LOCK_LEASE_FILE"
    chmod 600 "$TARGET_LOCK_LEASE_FILE" || fail "could not protect the target lease"
}

run_libpq_command() {
    local service_name="$1" command_limit="$2"
    shift 2
    if [ -n "$POSTGRES_CA_CERT_PATH" ]; then
        env -i PATH="$PATH" HOME="${HOME:-/}" TMPDIR="${TMPDIR:-/tmp}" \
            PGCONNECT_TIMEOUT="$CONNECT_TIMEOUT_SECONDS" \
            PGSERVICE="$service_name" PGSERVICEFILE="$LIBPQ_SERVICE_FILE" \
            PGPASSFILE="$LIBPQ_PASSFILE" PGSSLROOTCERT="$POSTGRES_CA_CERT_PATH" \
            python3 "$CONTAINMENT_RUNNER_FILE" "$command_limit" "$@"
    else
        env -i PATH="$PATH" HOME="${HOME:-/}" TMPDIR="${TMPDIR:-/tmp}" \
            PGCONNECT_TIMEOUT="$CONNECT_TIMEOUT_SECONDS" \
            PGSERVICE="$service_name" PGSERVICEFILE="$LIBPQ_SERVICE_FILE" \
            PGPASSFILE="$LIBPQ_PASSFILE" \
            python3 "$CONTAINMENT_RUNNER_FILE" "$command_limit" "$@"
    fi
}

provider_target_lease_probe() {
    local output probe_sql
    probe_sql="SELECT
CASE WHEN pg_catalog.pg_try_advisory_lock(pg_catalog.hashtextextended('$TARGET_PROVIDER_LEASE_KEY', 0)) THEN 1 ELSE 0 END,
CASE WHEN pg_catalog.pg_try_advisory_lock(pg_catalog.hashtextextended('$TARGET_PROVIDER_HOLDER_KEY', 0)) THEN 1 ELSE 0 END;"
    output="$(run_libpq_command bluey_restore_target_admin "$COMMAND_TIMEOUT_SECONDS" \
        psql -X -v ON_ERROR_STOP=1 -Atq -F '|' -c "$probe_sql" 2>/dev/null)" || return 1
    [ "$output" = "0|0" ]
}

acquire_provider_target_lease() {
    local deadline holder_sql now_epoch remaining_seconds
    TARGET_PROVIDER_LEASE_KEY="bluey-restore-target:v1:$TARGET_CLUSTER_SENTINEL:$TARGET_DATABASE_NAME"
    TARGET_PROVIDER_HOLDER_KEY="bluey-restore-holder:v1:$TARGET_CLUSTER_SENTINEL:$TARGET_DATABASE_NAME:$DEADMAN_PROVIDER_IDENTITY:$DISPOSABLE_SENTINEL_TOKEN"
    now_epoch="$(date -u +%s)"
    remaining_seconds=$((TARGET_EXPIRES_AT_EPOCH - now_epoch))
    holder_sql="SELECT pg_catalog.pg_advisory_lock(pg_catalog.hashtextextended('$TARGET_PROVIDER_LEASE_KEY', 0));
SELECT pg_catalog.pg_advisory_lock(pg_catalog.hashtextextended('$TARGET_PROVIDER_HOLDER_KEY', 0));
SELECT pg_catalog.pg_sleep($remaining_seconds);"
    if [ -n "$POSTGRES_CA_CERT_PATH" ]; then
        env -i PATH="$PATH" HOME="${HOME:-/}" TMPDIR="${TMPDIR:-/tmp}" \
            PGCONNECT_TIMEOUT="$CONNECT_TIMEOUT_SECONDS" PGSERVICE=bluey_restore_target_admin \
            PGSERVICEFILE="$LIBPQ_SERVICE_FILE" PGPASSFILE="$LIBPQ_PASSFILE" \
            PGSSLROOTCERT="$POSTGRES_CA_CERT_PATH" \
            python3 "$CONTAINMENT_RUNNER_FILE" $((remaining_seconds + 30)) \
            psql -X -v ON_ERROR_STOP=1 -Atq -c "$holder_sql" >/dev/null 2>&1 &
    else
        env -i PATH="$PATH" HOME="${HOME:-/}" TMPDIR="${TMPDIR:-/tmp}" \
            PGCONNECT_TIMEOUT="$CONNECT_TIMEOUT_SECONDS" PGSERVICE=bluey_restore_target_admin \
            PGSERVICEFILE="$LIBPQ_SERVICE_FILE" PGPASSFILE="$LIBPQ_PASSFILE" \
            python3 "$CONTAINMENT_RUNNER_FILE" $((remaining_seconds + 30)) \
            psql -X -v ON_ERROR_STOP=1 -Atq -c "$holder_sql" >/dev/null 2>&1 &
    fi
    TARGET_PROVIDER_LEASE_PID=$!
    deadline=$(( $(date -u +%s) + LEASE_ACQUIRE_TIMEOUT_SECONDS ))
    while kill -0 "$TARGET_PROVIDER_LEASE_PID" 2>/dev/null; do
        if provider_target_lease_probe; then
            TARGET_PROVIDER_LEASE_HELD=1
            return 0
        fi
        [ "$(date -u +%s)" -lt "$deadline" ] || break
        sleep 1
    done
    return 1
}

prove_provider_target_lease() {
    [ "$TARGET_PROVIDER_LEASE_HELD" = "1" ] &&
        [ -n "$TARGET_PROVIDER_LEASE_PID" ] &&
        kill -0 "$TARGET_PROVIDER_LEASE_PID" 2>/dev/null &&
        provider_target_lease_probe
}

postgres_identity() {
    local service_name="$1"
    local cluster_sentinel database_name database_oid extra libpq_error output postmaster_start
    local server_address server_port
    local identity_sql
    LAST_IDENTITY_FAILURE="query-or-output"
    printf 'query-or-output\n' > "$IDENTITY_FAILURE_FILE"

    identity_sql="SELECT pg_catalog.host(pg_catalog.inet_server_addr()),
       pg_catalog.inet_server_port()::text,
       EXTRACT(EPOCH FROM pg_catalog.pg_postmaster_start_time())::text,
       pg_catalog.current_setting('bluey.logical_cluster_id', true),
       d.oid::text,
       d.datname
FROM pg_catalog.pg_database AS d
WHERE d.datname = pg_catalog.current_database();"

    LIBPQ_ERROR_FILE="$(mktemp "${TMPDIR:-/tmp}/bluey-restore-drill-libpq-error.XXXXXX")" ||
        return 1
    if ! output="$(run_libpq_command "$service_name" "$COMMAND_TIMEOUT_SECONDS" \
        psql -X -v ON_ERROR_STOP=1 -Atq -c "$identity_sql" 2>"$LIBPQ_ERROR_FILE")"; then
        libpq_error="$(<"$LIBPQ_ERROR_FILE")"
        case "$libpq_error" in
            *"service file"*|*"service definition"*) LAST_IDENTITY_FAILURE="service-config" ;;
            *"password authentication failed"*|*"no password supplied"*)
                LAST_IDENTITY_FAILURE="authentication" ;;
            *"timeout"*) LAST_IDENTITY_FAILURE="timeout" ;;
            *"could not connect"*|*"Connection refused"*|*"could not translate host"*)
                LAST_IDENTITY_FAILURE="network" ;;
            *) LAST_IDENTITY_FAILURE="query-or-output" ;;
        esac
        printf '%s\n' "$LAST_IDENTITY_FAILURE" > "$IDENTITY_FAILURE_FILE"
        rm -f -- "$LIBPQ_ERROR_FILE" >/dev/null 2>&1 || true
        LIBPQ_ERROR_FILE=""
        return 1
    fi
    rm -f -- "$LIBPQ_ERROR_FILE" >/dev/null 2>&1 || true
    LIBPQ_ERROR_FILE=""
    case "$output" in
        *$'\n'*|*$'\r'*) return 1 ;;
    esac
    IFS='|' read -r server_address server_port postmaster_start cluster_sentinel \
        database_oid database_name extra <<< "$output"
    [[ "$server_address" =~ ^[0-9A-Fa-f:.]+$ ]] || return 1
    [[ "$server_port" =~ ^[0-9]+$ ]] || return 1
    [[ "$postmaster_start" =~ ^[0-9]+([.][0-9]+)?$ ]] || return 1
    [[ "$cluster_sentinel" =~ ^[A-Za-z0-9][A-Za-z0-9._:-]{15,127}$ ]] || return 1
    [[ "$database_oid" =~ ^[0-9]+$ ]] || return 1
    [ -n "$database_name" ] && [ -z "$extra" ] || return 1
    printf '%s|%s|%s|%s|%s|%s\n' "$server_address" "$server_port" \
        "$postmaster_start" "$cluster_sentinel" "$database_oid" "$database_name"
}

postgres_disposable_sentinel() {
    local service_name="$1" output sentinel_sql
    sentinel_sql="SELECT COALESCE(pg_catalog.shobj_description(d.oid, 'pg_database'), '')
FROM pg_catalog.pg_database AS d
WHERE d.datname = pg_catalog.current_database();"
    if ! output="$(run_libpq_command "$service_name" "$COMMAND_TIMEOUT_SECONDS" \
        psql -X -v ON_ERROR_STOP=1 -Atq -c "$sentinel_sql" 2>/dev/null)"; then
        return 1
    fi
    case "$output" in
        *$'\n'*|*$'\r'*) return 1 ;;
    esac
    printf '%s\n' "$output"
}

validate_postgres_catalog() {
    CATALOG_FILE="$(mktemp "${TMPDIR:-/tmp}/bluey-restore-drill-catalog.XXXXXX")" ||
        fail "could not allocate a temporary catalog file"
    if ! timeout --signal=TERM --kill-after=10s "${COMMAND_TIMEOUT_SECONDS}s" \
        pg_restore --list -- "$STABLE_BACKUP_FILE" >"$CATALOG_FILE" 2>/dev/null; then
        fail "pg_restore rejected the backup catalog"
    fi
    awk 'NF && substr($0, 1, 1) != ";" { found = 1 } END { exit(found ? 0 : 1) }' \
        "$CATALOG_FILE" || fail "PostgreSQL backup catalog contains no restorable entries"
}

postgres_count() {
    local service_name="$1" query="$2" output
    if ! output="$(run_libpq_command "$service_name" "$COMMAND_TIMEOUT_SECONDS" \
        psql -X -v ON_ERROR_STOP=1 -Atq -c "$query" 2>/dev/null)"; then
        return 1
    fi
    [[ "$output" =~ ^[0-9]+$ ]] || return 1
    printf '%s\n' "$output"
}

postgres_user_object_count() {
    local output user_object_sql
    user_object_sql="WITH user_namespaces AS (
    SELECT oid, nspname
    FROM pg_catalog.pg_namespace
    WHERE nspname NOT IN ('pg_catalog', 'information_schema')
      AND nspname NOT LIKE 'pg_toast%'
      AND nspname NOT LIKE 'pg_temp_%'
), bluey_restore_drill_user_objects AS (
    SELECT c.oid::text AS object_id FROM pg_catalog.pg_class AS c
      JOIN user_namespaces AS n ON n.oid = c.relnamespace
    UNION ALL SELECT p.oid::text FROM pg_catalog.pg_proc AS p
      JOIN user_namespaces AS n ON n.oid = p.pronamespace
    UNION ALL SELECT t.oid::text FROM pg_catalog.pg_type AS t
      JOIN user_namespaces AS n ON n.oid = t.typnamespace
    UNION ALL SELECT o.oid::text FROM pg_catalog.pg_operator AS o
      JOIN user_namespaces AS n ON n.oid = o.oprnamespace
    UNION ALL SELECT c.oid::text FROM pg_catalog.pg_collation AS c
      JOIN user_namespaces AS n ON n.oid = c.collnamespace
    UNION ALL SELECT c.oid::text FROM pg_catalog.pg_conversion AS c
      JOIN user_namespaces AS n ON n.oid = c.connamespace
    UNION ALL SELECT c.oid::text FROM pg_catalog.pg_ts_config AS c
      JOIN user_namespaces AS n ON n.oid = c.cfgnamespace
    UNION ALL SELECT d.oid::text FROM pg_catalog.pg_ts_dict AS d
      JOIN user_namespaces AS n ON n.oid = d.dictnamespace
    UNION ALL SELECT p.oid::text FROM pg_catalog.pg_ts_parser AS p
      JOIN user_namespaces AS n ON n.oid = p.prsnamespace
    UNION ALL SELECT t.oid::text FROM pg_catalog.pg_ts_template AS t
      JOIN user_namespaces AS n ON n.oid = t.tmplnamespace
    UNION ALL SELECT e.oid::text FROM pg_catalog.pg_extension AS e
      WHERE e.extname <> 'plpgsql'
    UNION ALL SELECT l.oid::text FROM pg_catalog.pg_largeobject_metadata AS l
    UNION ALL SELECT n.oid::text FROM user_namespaces AS n
      WHERE n.nspname <> 'public'
)
SELECT count(*) FROM bluey_restore_drill_user_objects;"
    if ! output="$(run_libpq_command bluey_restore_target "$COMMAND_TIMEOUT_SECONDS" \
        psql -X -v ON_ERROR_STOP=1 -Atq -c "$user_object_sql" 2>/dev/null)"; then
        return 1
    fi
    [[ "$output" =~ ^[0-9]+$ ]] || return 1
    printf '%s\n' "$output"
}

postgres_target_is_owned() {
    local output owner_sql
    owner_sql="SELECT CASE WHEN d.datdba = r.oid THEN 1 ELSE 0 END
FROM pg_catalog.pg_database AS d
CROSS JOIN pg_catalog.pg_roles AS r
WHERE d.datname = pg_catalog.current_database()
  AND r.rolname = CURRENT_USER;"
    if ! output="$(run_libpq_command bluey_restore_target "$COMMAND_TIMEOUT_SECONDS" \
        psql -X -v ON_ERROR_STOP=1 -Atq -c "$owner_sql" 2>/dev/null)"; then
        return 1
    fi
    [ "$output" = "1" ]
}

teardown_target_database() {
    local admin_address admin_cluster admin_database_name admin_database_oid admin_identity admin_port
    local admin_start current_identity current_sentinel drop_sql exists_sql remaining

    prove_provider_target_lease || return 1
    [ -n "$TARGET_IDENTITY_AUTHORITY" ] && [ -n "$TARGET_ADMIN_SERVER_AUTHORITY" ] &&
        [ -n "$EXPECTED_SENTINEL" ] || return 1
    current_identity="$(postgres_identity bluey_restore_target)" || return 1
    [ "$current_identity" = "$TARGET_IDENTITY_AUTHORITY" ] || return 1
    current_sentinel="$(postgres_disposable_sentinel bluey_restore_target)" || return 1
    [ "$current_sentinel" = "$EXPECTED_SENTINEL" ] || return 1

    admin_identity="$(postgres_identity bluey_restore_target_admin)" || return 1
    IFS='|' read -r admin_address admin_port admin_start admin_cluster admin_database_oid \
        admin_database_name <<< "$admin_identity"
    [ "$admin_database_name" = "postgres" ] || return 1
    [ "$admin_address|$admin_port|$admin_start|$admin_cluster" = \
        "$TARGET_ADMIN_SERVER_AUTHORITY" ] ||
        return 1

    prove_provider_target_lease || return 1
    drop_sql="DROP DATABASE \"$TARGET_DATABASE_NAME\" WITH (FORCE);"
    if ! run_libpq_command bluey_restore_target_admin "$COMMAND_TIMEOUT_SECONDS" \
        psql -X -v ON_ERROR_STOP=1 -q -c "$drop_sql" >/dev/null 2>&1; then
        return 1
    fi
    exists_sql="SELECT count(*) FROM pg_catalog.pg_database WHERE datname = '$TARGET_DATABASE_NAME';"
    remaining="$(run_libpq_command bluey_restore_target_admin "$COMMAND_TIMEOUT_SECONDS" \
        psql -X -v ON_ERROR_STOP=1 -Atq -c "$exists_sql" 2>/dev/null)" || return 1
    [ "$remaining" = "0" ] || return 1
    consume_deadman_lease_marker || return 1
    TEARDOWN_COMPLETE=1
    return 0
}

for required_command in timeout stat df mktemp cp chmod id; do
    require_cmd "$required_command"
done
detect_stat_style
validate_global_guard
resolve_backend_extension
if [ -z "$BACKUP_FILE" ]; then
    BACKUP_FILE="$(latest_backup)"
fi

[ -n "$BACKUP_FILE" ] || fail "no backup file found in $BACKUP_DIR/hourly"
validate_backup_path
verify_backup_checksum
create_stable_backup_copy

case "$BACKUP_FILE" in
    *.pgdump)
        require_cmd pg_restore
        require_cmd psql
        require_cmd python3
        require_cmd timeout
        create_containment_runner
        validate_target_guard
        validate_deadman_marker
        acquire_target_lease
        create_libpq_service_file

        # Parsing the archive, stable live identity reads, target-only sentinel
        # checks, and a final checksum pass all precede destructive --clean.
        validate_postgres_catalog
        if ! production_identity="$(postgres_identity bluey_restore_production)"; then
            fail "could not prove the live production PostgreSQL identity ($( < "$IDENTITY_FAILURE_FILE"))"
        fi
        if ! target_identity="$(postgres_identity bluey_restore_target)"; then
            fail "could not prove the live restore-target PostgreSQL identity ($( < "$IDENTITY_FAILURE_FILE"))"
        fi
        if ! production_identity_after_target="$(postgres_identity \
            bluey_restore_production)"; then
            fail "could not re-prove the live production PostgreSQL identity"
        fi
        [ "$production_identity_after_target" = "$production_identity" ] ||
            fail "the live production PostgreSQL identity drifted during preflight"

        IFS='|' read -r production_server_address production_server_port \
            production_postmaster_start production_cluster_sentinel_actual \
            production_database_oid \
            production_database_name <<< "$production_identity"
        IFS='|' read -r target_server_address target_server_port target_postmaster_start \
            target_cluster_sentinel_actual target_database_oid \
            actual_target_database_name <<< "$target_identity"
        [ "$production_cluster_sentinel_actual" = "$PRODUCTION_CLUSTER_SENTINEL" ] ||
            fail "production logical-cluster attestation does not match authority"
        [ "$target_cluster_sentinel_actual" = "$TARGET_CLUSTER_SENTINEL" ] ||
            fail "target logical-cluster attestation does not match authority"
        [ "$production_database_name" != "$actual_target_database_name" ] ||
            fail "production and restore-target database names must differ"
        [ "$actual_target_database_name" = "$TARGET_DATABASE_NAME" ] ||
            fail "the live target database name does not match BLUEY_RESTORE_DRILL_DATABASE_NAME"
        if ! target_admin_identity="$(postgres_identity bluey_restore_target_admin)"; then
            fail "could not prove the restore-target maintenance connection ($( < "$IDENTITY_FAILURE_FILE"))"
        fi
        IFS='|' read -r target_admin_address target_admin_port target_admin_start \
            target_admin_cluster target_admin_database_oid target_admin_database_name \
            <<< "$target_admin_identity"
        [ "$target_admin_database_name" = "postgres" ] ||
            fail "restore-target maintenance connection did not reach postgres"
        [ "$target_admin_cluster" = "$target_cluster_sentinel_actual" ] ||
            fail "restore-target maintenance connection reached a different logical cluster"
        [ "$target_admin_address" = "$target_server_address" ] &&
            [ "$target_admin_port" = "$target_server_port" ] &&
            [ "$target_admin_start" = "$target_postmaster_start" ] ||
            fail "restore-target maintenance connection reached a different server"
        TARGET_ADMIN_SERVER_AUTHORITY="$target_admin_address|$target_admin_port|$target_admin_start|$target_admin_cluster"

        same_server=0
        if [ "$production_server_address" = "$target_server_address" ] &&
            [ "$production_server_port" = "$target_server_port" ] &&
            [ "$production_postmaster_start" = "$target_postmaster_start" ]; then
            same_server=1
        fi
        if [ "$same_server" = "1" ] &&
            [ "$production_database_oid" = "$target_database_oid" ]; then
            fail "the restore target is the live production database"
        fi

        expected_sentinel="bluey-restore-drill-disposable:v1:$TARGET_DATABASE_NAME:$TARGET_EXPIRES_AT_EPOCH:$DISPOSABLE_SENTINEL_TOKEN"
        if ! production_sentinel="$(postgres_disposable_sentinel \
            bluey_restore_production)"; then
            fail "could not prove production lacks the disposable-target sentinel"
        fi
        [ "$production_sentinel" != "$expected_sentinel" ] ||
            fail "production unexpectedly carries the disposable-target sentinel"
        if ! disposable_sentinel="$(postgres_disposable_sentinel \
            bluey_restore_target)"; then
            fail "could not prove the live disposable-target sentinel"
        fi
        [ "$disposable_sentinel" = "$expected_sentinel" ] ||
            fail "restore target lacks its exact disposable-target sentinel"
        if ! baseline_user_objects="$(postgres_user_object_count)"; then
            fail "could not prove an empty restore-target baseline"
        fi
        [ "$baseline_user_objects" = "0" ] ||
            fail "restore target contains stale user objects"
        postgres_target_is_owned ||
            fail "restore-target role must directly own the disposable database"

        same_logical_cluster=0
        [ "$production_cluster_sentinel_actual" != "$target_cluster_sentinel_actual" ] ||
            same_logical_cluster=1
        restore_authority="separate-logical-cluster"
        if [ "$same_server" = "1" ] || [ "$same_logical_cluster" = "1" ]; then
            [ "$ALLOW_SAME_CLUSTER" = "1" ] ||
                fail "same-cluster restore targets require an explicit audited override"
            [ "${#SAME_CLUSTER_AUDIT_REF}" -le 128 ] &&
                [[ "$SAME_CLUSTER_AUDIT_REF" =~ ^[A-Za-z0-9][A-Za-z0-9._:/-]{2,127}$ ]] ||
                fail "same-cluster override requires a safe audit reference"
            restore_authority="same-logical-cluster:$SAME_CLUSTER_AUDIT_REF"
        fi

        verify_backup_checksum
        TARGET_IDENTITY_AUTHORITY="$target_identity"
        EXPECTED_SENTINEL="$expected_sentinel"
        acquire_provider_target_lease ||
            fail "could not acquire the provider/target-scoped restore lease"
        prove_provider_target_lease ||
            fail "provider/target-scoped restore lease was not held before restore"
        RESTORE_ERROR_FILE="$(mktemp \
            "${TMPDIR:-/tmp}/bluey-restore-drill-pg-restore.XXXXXX")" ||
            fail "could not allocate a temporary pg_restore error file"
        RESTORE_STARTED=1
        if ! run_libpq_command bluey_restore_target "$RESTORE_TIMEOUT_SECONDS" \
            pg_restore \
            --dbname="$TARGET_DATABASE_NAME" --clean --if-exists \
            --exit-on-error --no-owner --no-acl -- "$STABLE_BACKUP_FILE" \
            >/dev/null 2>"$RESTORE_ERROR_FILE"; then
            fail "pg_restore failed for the verified drill target"
        fi

        if ! target_identity_after_restore="$(postgres_identity \
            bluey_restore_target)"; then
            fail "could not re-prove the restore-target PostgreSQL identity"
        fi
        [ "$target_identity_after_restore" = "$target_identity" ] ||
            fail "restore-target PostgreSQL identity drifted during restore"
        if ! disposable_sentinel_after="$(postgres_disposable_sentinel \
            bluey_restore_target)"; then
            fail "could not re-prove the disposable-target sentinel"
        fi
        [ "$disposable_sentinel_after" = "$expected_sentinel" ] ||
            fail "restore target lost its disposable-target sentinel"

        accounts="$(postgres_count bluey_restore_target \
            'select count(*) from accounts')" ||
            fail "could not verify the restored accounts table"
        usage_events="$(postgres_count bluey_restore_target \
            'select count(*) from usage_events')" ||
            fail "could not verify the restored usage_events table"
        if ! production_identity_final="$(postgres_identity \
            bluey_restore_production)"; then
            fail "could not prove final production PostgreSQL non-impact"
        fi
        [ "$production_identity_final" = "$production_identity" ] ||
            fail "production PostgreSQL identity changed during the restore drill"
        if ! production_sentinel_final="$(postgres_disposable_sentinel \
            bluey_restore_production)"; then
            fail "could not re-prove final production sentinel non-impact"
        fi
        [ "$production_sentinel_final" = "$production_sentinel" ] ||
            fail "production sentinel changed during the restore drill"
        teardown_target_database ||
            fail "verified restore completed but disposable target teardown failed"
        TEARDOWN_COMPLETE=1
        backend="postgres"
        ;;
    *.db)
        require_cmd sqlite3
        integrity="$(timeout --signal=TERM --kill-after=10s \
            "${COMMAND_TIMEOUT_SECONDS}s" sqlite3 "$STABLE_BACKUP_FILE" \
            'PRAGMA integrity_check;' 2>/dev/null || true)"
        [ "$integrity" = "ok" ] || fail "SQLite integrity_check failed"
        accounts="$(timeout --signal=TERM --kill-after=10s \
            "${COMMAND_TIMEOUT_SECONDS}s" sqlite3 "$STABLE_BACKUP_FILE" \
            'select count(*) from accounts;' 2>/dev/null || true)"
        usage_events="$(timeout --signal=TERM --kill-after=10s \
            "${COMMAND_TIMEOUT_SECONDS}s" sqlite3 "$STABLE_BACKUP_FILE" \
            'select count(*) from usage_events;' 2>/dev/null || true)"
        [[ "$accounts" =~ ^[0-9]+$ ]] || fail "could not verify the restored accounts table"
        [[ "$usage_events" =~ ^[0-9]+$ ]] ||
            fail "could not verify the restored usage_events table"
        backend="sqlite"
        restore_authority="local-scratch"
        ;;
    *)
        fail "unsupported backup extension: $BACKUP_FILE"
        ;;
esac

[ "$backend" = "$BACKEND_AUTHORITY" ] ||
    fail "restored backup backend does not match configured authority"

size_bytes="$(file_size_bytes "$BACKUP_FILE")" || fail "backup size could not be re-read"

cat <<EOF
$(date -u +%FT%TZ) restore drill ok
backend=$backend
backup_file=$BACKUP_FILE
size_bytes=$size_bytes
sha256=$BACKUP_CHECKSUM
restore_authority=$restore_authority
target_expires_at_epoch=${TARGET_EXPIRES_AT_EPOCH:-none}
teardown=$([ "$TEARDOWN_COMPLETE" = "1" ] && echo drop-complete || echo not-required)
accounts=$accounts
usage_events=$usage_events
EOF
