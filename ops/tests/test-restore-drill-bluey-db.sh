#!/usr/bin/env bash

set -Eeuo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
SCRIPT="$ROOT/ops/restore-drill-bluey-db.sh"
TEST_TEMP_ROOT="$ROOT/.restore-drill-test-root"
mkdir -p "$TEST_TEMP_ROOT"
chmod 700 "$TEST_TEMP_ROOT"
TEST_ROOT="$(mktemp -d "$TEST_TEMP_ROOT/bluey-restore-drill-test.XXXXXX")"
MOCK_BIN="$TEST_ROOT/bin"
EMPTY_ENV="$TEST_ROOT/env"
STORAGE_ENV_DIR="$TEST_ROOT/storage-env"
SECRET_ENV_DIR="$TEST_ROOT/secret-env"
TMP_AREA="$TEST_ROOT/tmp"
MOCK_STATE_DIR="$TEST_ROOT/mock-state"
DEADMAN_DIR="$TEST_ROOT/deadman"
DEADMAN_MARKER="$DEADMAN_DIR/bluey_restore_drill_safe.lease"
ARGV_LOG="$TEST_ROOT/argv.log"
SERVICE_LOG="$TEST_ROOT/service.log"
RESTORE_LOG="$TEST_ROOT/restore.log"
TEARDOWN_LOG="$TEST_ROOT/teardown.log"
TARGET_DROPPED_FILE="$TEST_ROOT/target-dropped"
STDOUT_FILE="$TEST_ROOT/stdout"
STDERR_FILE="$TEST_ROOT/stderr"
PRODUCTION_IDENTITY_COUNT="$TEST_ROOT/production-identity-count"
TARGET_IDENTITY_COUNT="$TEST_ROOT/target-identity-count"
TARGET_SENTINEL_COUNT="$TEST_ROOT/target-sentinel-count"
PG_BACKUP="$TEST_ROOT/hourly/bluey-postgres-20260830T120000Z.pgdump"
PG_DAILY_BACKUP="$TEST_ROOT/daily/bluey-postgres-20260830.pgdump"
SQLITE_BACKUP="$TEST_ROOT/hourly/bluey-20260830T120000Z.db"
SQLITE_DAILY_BACKUP="$TEST_ROOT/daily/bluey-20260830.db"
UNSAFE_BACKUP="$TEST_ROOT/hourly/not-bluey.pgdump"
MIXED_BACKUP_DIR="$TEST_ROOT/mixed-backups"
MIXED_PG_BACKUP="$MIXED_BACKUP_DIR/hourly/bluey-postgres-20260830T120000Z.pgdump"
MIXED_SQLITE_BACKUP="$MIXED_BACKUP_DIR/hourly/bluey-20260830T130000Z.db"
ORIGINAL_PATH="$PATH"
SENTINEL_TOKEN='0123456789abcdef0123456789abcdef'
TEST_NOW_EPOCH="$(date -u +%s)"
DEFAULT_EXPIRES_AT_EPOCH=$((TEST_NOW_EPOCH + 3600))
REAL_PG_CTL="$(PATH="$ORIGINAL_PATH" command -v pg_ctl 2>/dev/null || true)"
REAL_PRODUCTION_DATA=""
REAL_TARGET_DATA=""

cleanup() {
    if [ -n "$REAL_PG_CTL" ]; then
        [ -z "$REAL_PRODUCTION_DATA" ] ||
            "$REAL_PG_CTL" -D "$REAL_PRODUCTION_DATA" -m immediate stop \
                >/dev/null 2>&1 || true
        [ -z "$REAL_TARGET_DATA" ] ||
            "$REAL_PG_CTL" -D "$REAL_TARGET_DATA" -m immediate stop \
                >/dev/null 2>&1 || true
    fi
    rm -rf -- "$TEST_ROOT"
    rmdir -- "$TEST_TEMP_ROOT" 2>/dev/null || true
}
trap cleanup EXIT

fail() {
    echo "test-restore-drill-bluey-db: $*" >&2
    exit 1
}

write_state() {
    printf '%s\n' "$2" > "$MOCK_STATE_DIR/$1"
    chmod 600 "$MOCK_STATE_DIR/$1"
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -- "$1" | awk '{print $1}'
    else
        shasum -a 256 -- "$1" | awk '{print $1}'
    fi
}

write_sidecar() {
    local backup="$1" source_name
    source_name="${2:-$(basename "$1")}" # Sidecar source field.
    printf '%s  %s\n' "$(sha256_file "$backup")" "$source_name" >"${backup}.sha256"
}

assert_no_temp_files() {
    if find "$TMP_AREA" -maxdepth 1 \
        \( -name 'bluey-restore-drill-catalog.*' \
        -o -name 'bluey-restore-drill-libpq.*' \
        -o -name 'bluey-restore-drill-pgpass.*' \
        -o -name 'bluey-restore-drill-identity-failure.*' \
        -o -name 'bluey-restore-drill-contained.*' \
        -o -name 'bluey-restore-drill-pg-restore.*' \
        -o -name 'bluey-restore-drill-copy.*' \) -print -quit | grep -q .; then
        fail "$1 retained a restore-drill temporary file"
    fi
}

assert_no_secret_output() {
    local label="$1"
    if grep -Fq '://' "$STDOUT_FILE" "$STDERR_FILE" "$ARGV_LOG" "$SERVICE_LOG" ||
        grep -Fq 'prod-secret' "$STDOUT_FILE" "$STDERR_FILE" "$ARGV_LOG" "$SERVICE_LOG" ||
        grep -Fq 'drill-secret' "$STDOUT_FILE" "$STDERR_FILE" "$ARGV_LOG" "$SERVICE_LOG" ||
        grep -Fq 'must-not-leak' "$STDOUT_FILE" "$STDERR_FILE" "$ARGV_LOG" "$SERVICE_LOG" ||
        grep -Fq 'env-file-must-not-leak' "$STDOUT_FILE" "$STDERR_FILE" "$ARGV_LOG" "$SERVICE_LOG" ||
        grep -Fq -- '-secret' "$STDOUT_FILE" "$STDERR_FILE" "$ARGV_LOG" "$SERVICE_LOG"; then
        fail "$label printed a connection URL or credential"
    fi
}

assert_failure_before_restore() {
    local label="$1" expected="$2"
    [ "$RUN_STATUS" -ne 0 ] || fail "$label unexpectedly succeeded"
    grep -Fq "$expected" "$STDERR_FILE" ||
        fail "$label did not report the expected failure"
    [ ! -s "$RESTORE_LOG" ] || fail "$label reached destructive pg_restore"
    assert_no_secret_output "$label"
    assert_no_temp_files "$label"
}

assert_postgres_success() {
    local label="$1"
    [ "$RUN_STATUS" = "0" ] || fail "$label failed"
    [ "$(wc -l <"$RESTORE_LOG" | tr -d ' ')" = "1" ] ||
        fail "$label did not restore exactly once"
    [ "$(wc -l <"$TEARDOWN_LOG" | tr -d ' ')" = "1" ] ||
        fail "$label did not tear down the disposable target exactly once"
    [ ! -e "$DEADMAN_MARKER" ] || fail "$label did not consume its local dead-man marker"
    grep -Fq 'backend=postgres' "$STDOUT_FILE" || fail "$label summary is missing"
    grep -Fq 'accounts=17' "$STDOUT_FILE" || fail "$label accounts count is missing"
    grep -Fq 'usage_events=29' "$STDOUT_FILE" ||
        fail "$label usage-events count is missing"
    assert_no_secret_output "$label"
    assert_no_temp_files "$label"
}

mkdir -p "$MOCK_BIN" "$EMPTY_ENV" "$TMP_AREA" "$MOCK_STATE_DIR" "$DEADMAN_DIR" \
    "$(dirname "$PG_BACKUP")" "$(dirname "$PG_DAILY_BACKUP")" \
    "$MIXED_BACKUP_DIR/hourly" "$STORAGE_ENV_DIR" "$SECRET_ENV_DIR"
chmod 700 "$EMPTY_ENV" "$TMP_AREA" "$MOCK_STATE_DIR" "$DEADMAN_DIR" \
    "$TEST_ROOT/hourly" "$TEST_ROOT/daily" "$MIXED_BACKUP_DIR" \
    "$MIXED_BACKUP_DIR/hourly" "$STORAGE_ENV_DIR"
printf 'export BLUEY_UNRELATED_SECRET=env-file-must-not-leak\n' \
    > "$SECRET_ENV_DIR/bluey-api.env"
chmod 600 "$SECRET_ENV_DIR/bluey-api.env"
printf 'deterministic PostgreSQL archive fixture\n' >"$PG_BACKUP"
cp "$PG_BACKUP" "$PG_DAILY_BACKUP"
printf 'deterministic SQLite archive fixture\n' >"$SQLITE_BACKUP"
cp "$SQLITE_BACKUP" "$SQLITE_DAILY_BACKUP"
printf 'unsafe archive fixture\n' >"$UNSAFE_BACKUP"
cp "$PG_BACKUP" "$MIXED_PG_BACKUP"
cp "$SQLITE_BACKUP" "$MIXED_SQLITE_BACKUP"
write_sidecar "$PG_BACKUP"
write_sidecar "$PG_DAILY_BACKUP"
write_sidecar "$SQLITE_BACKUP"
write_sidecar "$SQLITE_DAILY_BACKUP"
write_sidecar "$UNSAFE_BACKUP"
write_sidecar "$MIXED_PG_BACKUP"
write_sidecar "$MIXED_SQLITE_BACKUP"
chmod 600 "$PG_BACKUP" "$PG_DAILY_BACKUP" "$SQLITE_BACKUP" "$SQLITE_DAILY_BACKUP" \
    "$UNSAFE_BACKUP" "$MIXED_PG_BACKUP" "$MIXED_SQLITE_BACKUP" \
    "${PG_BACKUP}.sha256" "${PG_DAILY_BACKUP}.sha256" \
    "${SQLITE_BACKUP}.sha256" "${SQLITE_DAILY_BACKUP}.sha256" \
    "${UNSAFE_BACKUP}.sha256" "${MIXED_PG_BACKUP}.sha256" \
    "${MIXED_SQLITE_BACKUP}.sha256"
touch -t 202608301200.00 "$MIXED_PG_BACKUP" "${MIXED_PG_BACKUP}.sha256"
touch -t 202608301300.00 "$MIXED_SQLITE_BACKUP" "${MIXED_SQLITE_BACKUP}.sha256"
printf 'BLUEY_BACKUP_DIR=%q\nBLUEY_SERVER_DB_BACKEND=postgres\n' \
    "$MIXED_BACKUP_DIR" >"$STORAGE_ENV_DIR/bluey-storage.env"
chmod 600 "$STORAGE_ENV_DIR/bluey-storage.env"

cat >"$MOCK_BIN/psql" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail

mock_root="${TMPDIR%/tmp}"
state_dir="$mock_root/mock-state"
state_value() { cat "$state_dir/$1"; }
argv_log="$mock_root/argv.log"
service_log="$mock_root/service.log"
teardown_log="$mock_root/teardown.log"
target_dropped_file="$mock_root/target-dropped"
production_identity_count="$mock_root/production-identity-count"
target_identity_count="$mock_root/target-identity-count"
target_sentinel_count="$mock_root/target-sentinel-count"

[ "${PGCONNECT_TIMEOUT:-}" = "$(state_value expected_connect_timeout)" ] || exit 91
[ -f "${PGSERVICEFILE:-}" ] || exit 92
[ -f "${PGPASSFILE:-}" ] || exit 89
if stat -c '%a' "$PGSERVICEFILE" >/dev/null 2>&1; then
    service_mode="$(stat -c '%a' "$PGSERVICEFILE")"
else
    service_mode="$(stat -f '%Lp' "$PGSERVICEFILE")"
fi
[ "$service_mode" = "600" ] || exit 90
if stat -c '%a' "$PGPASSFILE" >/dev/null 2>&1; then
    pass_mode="$(stat -c '%a' "$PGPASSFILE")"
else
    pass_mode="$(stat -f '%Lp' "$PGPASSFILE")"
fi
[ "$pass_mode" = "600" ] || exit 88
case "${PGSERVICE:-}" in
    bluey_restore_production|bluey_restore_target|bluey_restore_target_admin) ;;
    *) exit 93 ;;
esac

service_value() {
    local key="$1"
    awk -v section="[$PGSERVICE]" -v key="$key" '
        $0 == section { active = 1; next }
        /^\[/ { active = 0 }
        active && index($0, key "=") == 1 {
            print substr($0, length(key) + 2)
            exit
        }
    ' "$PGSERVICEFILE"
}

host="$(service_value host)"
port="$(service_value port)"
user="$(service_value user)"
database="$(service_value dbname)"
sslmode="$(service_value sslmode)"
connect_timeout="$(service_value connect_timeout)"
[ -n "$host" ] && [ -n "$user" ] && [ -n "$database" ] || exit 94
case "$host$user$database$port" in *"'"*) exit 85 ;; esac
if grep -q '^password=' "$PGSERVICEFILE"; then exit 84; fi
if grep -Fq ":$user:" "$PGPASSFILE"; then :; else exit 87; fi
printf 'service=%s host=%s port=%s user=%s dbname=%s sslmode=%s connect_timeout=%s passfile=set\n' \
    "$PGSERVICE" "$host" "$port" "$user" "$database" "$sslmode" "$connect_timeout" \
    >>"$service_log"

{
    printf 'psql'
    for argument in "$@"; do
        case "$argument" in
            *://*|*prod-secret*|*drill-secret*) exit 95 ;;
        esac
        printf '\t%s' "$argument"
    done
    printf '\n'
} >>"$argv_log"

if env | grep -Ev '^BLUEY_RESTORE_CONTAINED=1$' | \
    grep -Eq '^(BLUEY_|AWS_|OFFSITE_|MOCK_|PRODUCTION_DATABASE_URL|TARGET_DATABASE_URL)='; then
    exit 86
fi
[ "${BLUEY_RESTORE_CONTAINED:-}" = "1" ] || exit 83

next_value() {
    local count_file="$1" first="$2" later="$3" final="${4:-}" count=0
    if [ -f "$count_file" ]; then
        count="$(cat "$count_file")"
    fi
    count=$((count + 1))
    printf '%s\n' "$count" >"$count_file"
    if [ "$count" -ge 3 ] && [ -n "$final" ]; then
        printf '%s\n' "$final"
    elif [ "$count" -ge 2 ] && [ -n "$later" ]; then
        printf '%s\n' "$later"
    else
        printf '%s\n' "$first"
    fi
}

arguments="$*"
case "$arguments" in
    *pg_advisory_lock*pg_sleep*)
        lease_marker="$mock_root/provider-target-lease-held"
        trap 'rm -f -- "$lease_marker"; exit 0' EXIT HUP INT TERM
        : >"$lease_marker"
        while :; do sleep 1; done
        ;;
    *pg_try_advisory_lock*)
        if [ "$(state_value provider_target_lease_conflict)" = "1" ]; then
            printf '0|1\n'
        elif [ -f "$mock_root/provider-target-lease-held" ]; then
            printf '0|0\n'
        else
            printf '1|1\n'
        fi
        ;;
    *inet_server_addr*)
        if [ "${PGSERVICE:-}" = "$(state_value identity_unavailable_service)" ]; then
            exit 1
        fi
        if [ "$PGSERVICE" = "bluey_restore_production" ]; then
            next_value "$production_identity_count" "$(state_value production_identity)" \
                "$(state_value production_identity_after)" \
                "$(state_value production_identity_final)"
        elif [ "$PGSERVICE" = "bluey_restore_target" ]; then
            next_value "$target_identity_count" "$(state_value target_identity)" \
                "$(state_value target_identity_after)"
        else
            state_value target_admin_identity
        fi
        ;;
    *shobj_description*)
        if [ "$PGSERVICE" = "bluey_restore_production" ]; then
            state_value production_sentinel
        else
            next_value "$target_sentinel_count" "$(state_value target_sentinel)" \
                "$(state_value target_sentinel_after)"
        fi
        ;;
    *bluey_restore_drill_user_objects*) state_value target_object_count ;;
    *'d.datdba = r.oid'*) state_value target_owner ;;
    *'DROP DATABASE'*)
        printf 'teardown\n' >>"$teardown_log"
        [ "$(state_value teardown_fail)" = "0" ] || exit 1
        : >"$target_dropped_file"
        ;;
    *'SELECT count(*) FROM pg_catalog.pg_database WHERE datname ='*)
        if [ -f "$target_dropped_file" ]; then printf '0\n'; else printf '1\n'; fi
        ;;
    *'select count(*) from accounts'*) printf '17\n' ;;
    *'select count(*) from usage_events'*) printf '29\n' ;;
    *) exit 1 ;;
esac
MOCK

cat >"$MOCK_BIN/pg_restore" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail

mock_root="${TMPDIR%/tmp}"
state_dir="$mock_root/mock-state"
state_value() { cat "$state_dir/$1"; }
argv_log="$mock_root/argv.log"
restore_log="$mock_root/restore.log"

list=0
destructive=0
terminator=0
database_name=''
{
    printf 'pg_restore'
    for argument in "$@"; do
        case "$argument" in
            *://*|*prod-secret*|*drill-secret*) exit 95 ;;
            --list) list=1 ;;
            --clean) destructive=1 ;;
            --dbname=*) database_name="${argument#--dbname=}" ;;
            --) terminator=1 ;;
        esac
        printf '\t%s' "$argument"
    done
    printf '\n'
} >>"$argv_log"
[ "$terminator" = "1" ] || exit 96

if [ "$list" = "1" ]; then
    [ "$(state_value catalog_fail)" = "0" ] || exit 1
    printf '%s\n' '; PostgreSQL database dump'
    printf '%s\n' '1; 2615 2200 SCHEMA - public bluey'
    exit 0
fi
if [ "$destructive" = "1" ]; then
    [ "${BLUEY_RESTORE_CONTAINED:-}" = "1" ] || exit 101
    [ "${PGSERVICE:-}" = "bluey_restore_target" ] || exit 97
    [ -f "${PGSERVICEFILE:-}" ] || exit 98
    [ "${PGCONNECT_TIMEOUT:-}" = "$(state_value expected_connect_timeout)" ] || exit 99
    [ "$database_name" = "$(state_value expected_target_name)" ] || exit 100
    printf 'restore\n' >>"$restore_log"
    if [ "$(state_value restore_block)" = "1" ]; then
        printf '%s\n' "$$" >"$mock_root/blocked-restore-pid"
        trap 'exit 0' HUP INT TERM
        while :; do sleep 1; done
    fi
    [ "$(state_value restore_fail)" = "0" ] || exit 1
    exit 0
fi
exit 1
MOCK

cat >"$MOCK_BIN/timeout" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail

mock_root="${TMPDIR%/tmp}"
argv_log="$mock_root/argv.log"

{
    printf 'timeout'
    for argument in "$@"; do
        case "$argument" in
            *://*|*prod-secret*|*drill-secret*) exit 95 ;;
        esac
        printf '\t%s' "$argument"
    done
    printf '\n'
} >>"$argv_log"
while [[ "${1:-}" == --* ]]; do shift; done
[[ "${1:-}" =~ ^[0-9]+s$ ]] || exit 98
shift
exec "$@"
MOCK

cat >"$MOCK_BIN/sqlite3" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail

mock_root="${TMPDIR%/tmp}"
query="${2:-}"
case "$query" in
    'PRAGMA integrity_check;') cat "$mock_root/mock-state/sqlite_integrity" ;;
    'select count(*) from accounts;') printf '5\n' ;;
    'select count(*) from usage_events;') printf '8\n' ;;
    *) exit 1 ;;
esac
MOCK

cat >"$MOCK_BIN/df" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
mock_root="${TMPDIR%/tmp}"
available_kib="$(cat "$mock_root/mock-state/available_kib")"
printf 'Filesystem 1024-blocks Used Available Capacity Mounted on\n'
printf '/dev/mock 999999 1 %s 1%% /mock\n' "$available_kib"
MOCK

cat >"$MOCK_BIN/cp" <<'MOCK'
#!/usr/bin/env bash
set -euo pipefail
mock_root="${TMPDIR%/tmp}"
/bin/cp "$@"
if [ "$(cat "$mock_root/mock-state/mutate_source_after_copy")" = "1" ]; then
    arguments=("$@")
    source_index=$((${#arguments[@]} - 2))
    printf 'mutated-after-copy\n' >> "${arguments[$source_index]}"
fi
MOCK

chmod 755 "$MOCK_BIN/psql" "$MOCK_BIN/pg_restore" "$MOCK_BIN/timeout" \
    "$MOCK_BIN/sqlite3" "$MOCK_BIN/df" "$MOCK_BIN/cp"

reset_postgres_case() {
    CASE_BACKUP_FILE="$PG_BACKUP"
    CASE_BACKUP_DIR="$TEST_ROOT"
    CASE_EFFECTIVE_BACKUP_DIR=''
    CASE_ENV_DIR="$EMPTY_ENV"
    CASE_PRODUCTION_URL='postgresql://prod-user:prod%2Dsecret@prod.invalid:5433/bluey_prod?sslmode=require'
    CASE_TARGET_URL='postgresql://drill-user:drill%2Dsecret@drill.invalid:6543/bluey_restore_drill_safe?sslmode=require'
    CASE_PRODUCTION_CLUSTER='production-cluster-0001'
    CASE_TARGET_CLUSTER='target-cluster-0000001'
    CASE_PRODUCTION_IDENTITY="10.1.0.1|5433|1725000000.100|$CASE_PRODUCTION_CLUSTER|16384|bluey_prod"
    CASE_PRODUCTION_IDENTITY_AFTER="$CASE_PRODUCTION_IDENTITY"
    CASE_PRODUCTION_IDENTITY_FINAL="$CASE_PRODUCTION_IDENTITY"
    CASE_TARGET_IDENTITY="10.2.0.1|6543|1725000100.200|$CASE_TARGET_CLUSTER|16385|bluey_restore_drill_safe"
    CASE_TARGET_IDENTITY_AFTER="$CASE_TARGET_IDENTITY"
    CASE_TARGET_ADMIN_IDENTITY="10.2.0.1|6543|1725000100.200|$CASE_TARGET_CLUSTER|5|postgres"
    CASE_TARGET_NAME='bluey_restore_drill_safe'
    CASE_CONFIRMATION='restore:bluey_restore_drill_safe'
    CASE_SENTINEL_TOKEN="$SENTINEL_TOKEN"
    CASE_EXPIRES_AT_EPOCH="$DEFAULT_EXPIRES_AT_EPOCH"
    CASE_PRODUCTION_SENTINEL=''
    CASE_TARGET_SENTINEL="bluey-restore-drill-disposable:v1:$CASE_TARGET_NAME:$CASE_EXPIRES_AT_EPOCH:$CASE_SENTINEL_TOKEN"
    CASE_TARGET_SENTINEL_AFTER="$CASE_TARGET_SENTINEL"
    CASE_TARGET_OBJECT_COUNT=0
    CASE_TARGET_OWNER=1
    CASE_ALLOW_SAME_CLUSTER=0
    CASE_AUDIT_REF=''
    CASE_CATALOG_FAIL=0
    CASE_RESTORE_FAIL=0
    CASE_RESTORE_BLOCK=0
    CASE_TEARDOWN_FAIL=0
    CASE_IDENTITY_UNAVAILABLE_SERVICE=''
    CASE_DRILL_AUTHORITY='audit:restore-drill-test'
    CASE_DEADMAN_PROVIDER_IDENTITY='provider:test-restore-monitor'
    CASE_DEADMAN_MODE='valid'
    CASE_PRECREATE_TARGET_LOCK=0
    CASE_PROVIDER_TARGET_LEASE_CONFLICT=0
    CASE_MAX_BACKUP_BYTES=1048576
    CASE_MIN_FREE_GB=0
    CASE_AVAILABLE_KIB=99999999
    CASE_MUTATE_SOURCE_AFTER_COPY=0
    write_sidecar "$PG_BACKUP"
}

run_postgres() {
    : >"$ARGV_LOG"
    : >"$SERVICE_LOG"
    : >"$RESTORE_LOG"
    : >"$TEARDOWN_LOG"
    : >"$STDOUT_FILE"
    : >"$STDERR_FILE"
    rm -f -- "$PRODUCTION_IDENTITY_COUNT" "$TARGET_IDENTITY_COUNT" \
        "$TARGET_SENTINEL_COUNT" "$TARGET_DROPPED_FILE"
    effective_backup_dir="${CASE_EFFECTIVE_BACKUP_DIR:-$CASE_BACKUP_DIR}"
    rm -rf -- "$effective_backup_dir/.restore-drill-locks"
    mkdir -p "$effective_backup_dir/deadman"
    chmod 700 "$effective_backup_dir/deadman"
    DEADMAN_MARKER="$effective_backup_dir/deadman/$CASE_TARGET_NAME.lease"
    printf 'bluey-restore-drill-deadman:v1:%s:%s:%s:%s:%s:%s\n' \
        "$CASE_TARGET_CLUSTER" "$CASE_TARGET_NAME" "$CASE_EXPIRES_AT_EPOCH" \
        "$CASE_SENTINEL_TOKEN" "$CASE_DRILL_AUTHORITY" \
        "$CASE_DEADMAN_PROVIDER_IDENTITY" > "$DEADMAN_MARKER"
    chmod 600 "$DEADMAN_MARKER"
    case "$CASE_DEADMAN_MODE" in
        valid) ;;
        missing) rm -f -- "$DEADMAN_MARKER" ;;
        mismatch) printf 'wrong-deadman-attestation\n' > "$DEADMAN_MARKER" ;;
        *) fail "unknown dead-man fixture mode" ;;
    esac
    if [ "$CASE_PRECREATE_TARGET_LOCK" = "1" ]; then
        mkdir -p "$effective_backup_dir/.restore-drill-locks/$CASE_TARGET_NAME.lock"
        chmod 700 "$effective_backup_dir/.restore-drill-locks" \
            "$effective_backup_dir/.restore-drill-locks/$CASE_TARGET_NAME.lock"
    fi
    write_state production_identity "$CASE_PRODUCTION_IDENTITY"
    write_state production_identity_after "$CASE_PRODUCTION_IDENTITY_AFTER"
    write_state production_identity_final "$CASE_PRODUCTION_IDENTITY_FINAL"
    write_state target_identity "$CASE_TARGET_IDENTITY"
    write_state target_identity_after "$CASE_TARGET_IDENTITY_AFTER"
    write_state target_admin_identity "$CASE_TARGET_ADMIN_IDENTITY"
    write_state production_sentinel "$CASE_PRODUCTION_SENTINEL"
    write_state target_sentinel "$CASE_TARGET_SENTINEL"
    write_state target_sentinel_after "$CASE_TARGET_SENTINEL_AFTER"
    write_state target_object_count "$CASE_TARGET_OBJECT_COUNT"
    write_state target_owner "$CASE_TARGET_OWNER"
    write_state catalog_fail "$CASE_CATALOG_FAIL"
    write_state restore_fail "$CASE_RESTORE_FAIL"
    write_state restore_block "$CASE_RESTORE_BLOCK"
    write_state teardown_fail "$CASE_TEARDOWN_FAIL"
    write_state identity_unavailable_service "${CASE_IDENTITY_UNAVAILABLE_SERVICE:-none}"
    write_state expected_connect_timeout 10
    write_state expected_target_name "$CASE_TARGET_NAME"
    write_state available_kib "$CASE_AVAILABLE_KIB"
    write_state mutate_source_after_copy "$CASE_MUTATE_SOURCE_AFTER_COPY"
    write_state provider_target_lease_conflict "$CASE_PROVIDER_TARGET_LEASE_CONFLICT"
    set +e
    BLUEY_ENV_DIR="$CASE_ENV_DIR" \
    BLUEY_SERVER_DB_BACKEND=postgres \
    BLUEY_BACKUP_DIR="$CASE_BACKUP_DIR" \
    BLUEY_RESTORE_DRILL_BACKUP_FILE="$CASE_BACKUP_FILE" \
    BLUEY_DATABASE_URL="$CASE_PRODUCTION_URL" \
    BLUEY_RESTORE_DRILL_DATABASE_URL="$CASE_TARGET_URL" \
    BLUEY_RESTORE_DRILL_DATABASE_NAME="$CASE_TARGET_NAME" \
    BLUEY_RESTORE_DRILL_CONFIRMATION="$CASE_CONFIRMATION" \
    BLUEY_RESTORE_DRILL_SENTINEL_TOKEN="$CASE_SENTINEL_TOKEN" \
    BLUEY_RESTORE_DRILL_TARGET_EXPIRES_AT_EPOCH="$CASE_EXPIRES_AT_EPOCH" \
    BLUEY_RESTORE_DRILL_TEARDOWN_MODE=drop \
    BLUEY_RESTORE_DRILL_PRODUCTION_CLUSTER_SENTINEL="$CASE_PRODUCTION_CLUSTER" \
    BLUEY_RESTORE_DRILL_TARGET_CLUSTER_SENTINEL="$CASE_TARGET_CLUSTER" \
    BLUEY_RESTORE_DRILL_AUTHORITY="$CASE_DRILL_AUTHORITY" \
    BLUEY_RESTORE_DRILL_DEADMAN_PROVIDER_IDENTITY="$CASE_DEADMAN_PROVIDER_IDENTITY" \
    BLUEY_RESTORE_DRILL_DEADMAN_MARKER_FILE="$DEADMAN_MARKER" \
    BLUEY_RESTORE_DRILL_MAX_BACKUP_BYTES="$CASE_MAX_BACKUP_BYTES" \
    BLUEY_RESTORE_DRILL_MIN_FREE_GB="$CASE_MIN_FREE_GB" \
    BLUEY_RESTORE_DRILL_REQUIRE_TRUSTED_PATHS=1 \
    BLUEY_RESTORE_DRILL_ALLOW_SAME_CLUSTER="$CASE_ALLOW_SAME_CLUSTER" \
    BLUEY_RESTORE_DRILL_SAME_CLUSTER_AUDIT_REF="$CASE_AUDIT_REF" \
    BLUEY_RESTORE_DRILL_CONNECT_TIMEOUT_SECONDS=10 \
    BLUEY_RESTORE_DRILL_RESTORE_TIMEOUT_SECONDS=900 \
    BLUEY_RESTORE_DRILL_LEASE_ACQUIRE_TIMEOUT_SECONDS=1 \
    TMPDIR="$TMP_AREA" \
    BLUEY_FAKE_SECRET=must-not-leak \
    AWS_SECRET_ACCESS_KEY=must-not-leak \
    PATH="$MOCK_BIN:$ORIGINAL_PATH" \
        "$SCRIPT" >"$STDOUT_FILE" 2>"$STDERR_FILE" &
    SCRIPT_PID=$!
    if [ "$CASE_RESTORE_BLOCK" = "1" ]; then
        blocked_pid_file="$TEST_ROOT/blocked-restore-pid"
        for _ in $(seq 1 100); do
            [ -s "$blocked_pid_file" ] && break
            kill -0 "$SCRIPT_PID" 2>/dev/null || break
            sleep 0.1
        done
        [ -s "$blocked_pid_file" ] || fail "blocking restore child did not start"
        BLOCKED_RESTORE_PID="$(cat "$blocked_pid_file")"
        kill -9 "$SCRIPT_PID" 2>/dev/null || true
    fi
    wait "$SCRIPT_PID" 2>/dev/null
    RUN_STATUS=$?
    set -e
}

if [ "${BLUEY_RESTORE_DRILL_REAL_ONLY:-0}" != "1" ]; then
# Unsafe paths and filenames fail before any database tooling.
reset_postgres_case
CASE_BACKUP_FILE='bluey-postgres-20260830T120000Z.pgdump'
run_postgres
assert_failure_before_restore "relative backup path" "must be an absolute path"

reset_postgres_case
CASE_BACKUP_FILE="$UNSAFE_BACKUP"
run_postgres
assert_failure_before_restore "unsafe backup name" "outside the Bluey policy"

reset_postgres_case
CASE_BACKUP_FILE="$SQLITE_BACKUP"
run_postgres
assert_failure_before_restore "explicit backend mismatch" \
    "backup extension does not match BLUEY_SERVER_DB_BACKEND"

reset_postgres_case
chmod 666 "$PG_BACKUP"
run_postgres
assert_failure_before_restore "writable backup" "backup file is not trusted"
chmod 600 "$PG_BACKUP"

reset_postgres_case
rm -f -- "${PG_BACKUP}.sha256"
ln -s "${PG_DAILY_BACKUP}.sha256" "${PG_BACKUP}.sha256"
run_postgres
assert_failure_before_restore "symlink checksum sidecar" "must not be a symbolic link"
rm -f -- "${PG_BACKUP}.sha256"
write_sidecar "$PG_BACKUP"
chmod 600 "${PG_BACKUP}.sha256"

reset_postgres_case
CASE_MAX_BACKUP_BYTES=1
run_postgres
assert_failure_before_restore "oversized restore snapshot" \
    "exceeds BLUEY_RESTORE_DRILL_MAX_BACKUP_BYTES"

reset_postgres_case
CASE_MIN_FREE_GB=1
CASE_AVAILABLE_KIB=512
run_postgres
assert_failure_before_restore "insufficient restore staging reserve" \
    "lacks the configured post-copy free-space reserve"

reset_postgres_case
CASE_MUTATE_SOURCE_AFTER_COPY=1
run_postgres
assert_failure_before_restore "backup copy TOCTOU" \
    "backup changed while creating its protected copy"

reset_postgres_case
CASE_DEADMAN_MODE=missing
run_postgres
assert_failure_before_restore "missing external dead-man lease" \
    "dead-man marker is not trusted"

reset_postgres_case
CASE_DEADMAN_MODE=mismatch
run_postgres
assert_failure_before_restore "mismatched external dead-man lease" \
    "cleanup attestation does not match"

reset_postgres_case
CASE_PRECREATE_TARGET_LOCK=1
run_postgres
assert_failure_before_restore "concurrent target lease" \
    "another restore drill already holds the target lease"

# Exact URL equality and distinct aliases resolving to one live server/database
# are rejected using accessible server/database identity, without pg_control.
reset_postgres_case
CASE_PRODUCTION_URL='postgresql://same-user:same%2Dsecret@same.invalid:5432/bluey_restore_drill_same?sslmode=require'
CASE_TARGET_URL="$CASE_PRODUCTION_URL"
CASE_PRODUCTION_CLUSTER='same-cluster-0000001'
CASE_TARGET_CLUSTER="$CASE_PRODUCTION_CLUSTER"
CASE_PRODUCTION_IDENTITY="10.3.0.1|5432|1725000200.300|$CASE_PRODUCTION_CLUSTER|17000|bluey_restore_drill_same"
CASE_PRODUCTION_IDENTITY_AFTER="$CASE_PRODUCTION_IDENTITY"
CASE_TARGET_IDENTITY="$CASE_PRODUCTION_IDENTITY"
CASE_TARGET_IDENTITY_AFTER="$CASE_TARGET_IDENTITY"
CASE_TARGET_ADMIN_IDENTITY="10.3.0.1|5432|1725000200.300|$CASE_TARGET_CLUSTER|5|postgres"
CASE_TARGET_NAME='bluey_restore_drill_same'
CASE_CONFIRMATION='restore:bluey_restore_drill_same'
CASE_TARGET_SENTINEL="bluey-restore-drill-disposable:v1:$CASE_TARGET_NAME:$CASE_EXPIRES_AT_EPOCH:$CASE_SENTINEL_TOKEN"
CASE_TARGET_SENTINEL_AFTER="$CASE_TARGET_SENTINEL"
run_postgres
assert_failure_before_restore "same URL" "database names must differ"

reset_postgres_case
CASE_PRODUCTION_URL='postgresql://prod-user:prod%2Dsecret@primary.invalid:5432/bluey_restore_drill_alias?sslmode=require'
CASE_TARGET_URL='postgresql://drill-user:drill%2Dsecret@replica.invalid:5432/bluey_restore_drill_alias?sslmode=require'
CASE_PRODUCTION_CLUSTER='alias-cluster-000001'
CASE_TARGET_CLUSTER="$CASE_PRODUCTION_CLUSTER"
CASE_PRODUCTION_IDENTITY="10.4.0.1|5432|1725000300.400|$CASE_PRODUCTION_CLUSTER|18000|bluey_restore_drill_alias"
CASE_PRODUCTION_IDENTITY_AFTER="$CASE_PRODUCTION_IDENTITY"
CASE_TARGET_IDENTITY="$CASE_PRODUCTION_IDENTITY"
CASE_TARGET_IDENTITY_AFTER="$CASE_TARGET_IDENTITY"
CASE_TARGET_ADMIN_IDENTITY="10.4.0.1|5432|1725000300.400|$CASE_TARGET_CLUSTER|5|postgres"
CASE_TARGET_NAME='bluey_restore_drill_alias'
CASE_CONFIRMATION='restore:bluey_restore_drill_alias'
CASE_TARGET_SENTINEL="bluey-restore-drill-disposable:v1:$CASE_TARGET_NAME:$CASE_EXPIRES_AT_EPOCH:$CASE_SENTINEL_TOKEN"
CASE_TARGET_SENTINEL_AFTER="$CASE_TARGET_SENTINEL"
run_postgres
assert_failure_before_restore "aliased live database" "database names must differ"

reset_postgres_case
CASE_TARGET_CLUSTER="$CASE_PRODUCTION_CLUSTER"
CASE_TARGET_IDENTITY="10.2.0.1|6543|1725000100.200|$CASE_TARGET_CLUSTER|16385|bluey_restore_drill_safe"
CASE_TARGET_IDENTITY_AFTER="$CASE_TARGET_IDENTITY"
CASE_TARGET_ADMIN_IDENTITY="10.2.0.1|6543|1725000100.200|$CASE_TARGET_CLUSTER|5|postgres"
run_postgres
assert_failure_before_restore "same logical cluster through distinct servers" \
    "require an explicit audited override"

reset_postgres_case
CASE_TARGET_CLUSTER='declared-target-cluster-wrong'
run_postgres
assert_failure_before_restore "target logical-cluster attestation mismatch" \
    "target logical-cluster attestation does not match authority"

reset_postgres_case
CASE_PRODUCTION_IDENTITY="10.1.0.1|5433|1725000000.100|$CASE_PRODUCTION_CLUSTER|16384|bluey_restore_drill_safe"
CASE_PRODUCTION_IDENTITY_AFTER="$CASE_PRODUCTION_IDENTITY"
CASE_PRODUCTION_IDENTITY_FINAL="$CASE_PRODUCTION_IDENTITY"
run_postgres
assert_failure_before_restore "equal database names across clusters" \
    "database names must differ"

# Unsafe authority inputs and live-name or identity failures are fail-closed.
reset_postgres_case
CASE_TARGET_NAME='bluey_prod'
CASE_CONFIRMATION='restore:bluey_prod'
run_postgres
assert_failure_before_restore "unsafe target name" "must start with bluey_restore_drill"

reset_postgres_case
CASE_CONFIRMATION='yes'
run_postgres
assert_failure_before_restore "unsafe confirmation" "must equal restore:bluey_restore_drill_safe"

reset_postgres_case
CASE_SENTINEL_TOKEN='short'
run_postgres
assert_failure_before_restore "unsafe sentinel token" "must be 32 to 128 hexadecimal"

reset_postgres_case
CASE_TARGET_URL='postgresql://drill-user:drill%ZZsecret@drill.invalid:6543/bluey_restore_drill_safe?sslmode=require'
run_postgres
assert_failure_before_restore "malformed target URI" \
    "could not be converted to a safe libpq service"

reset_postgres_case
CASE_TARGET_IDENTITY="10.2.0.1|6543|1725000100.200|$CASE_TARGET_CLUSTER|16385|bluey_restore_drill_other"
CASE_TARGET_IDENTITY_AFTER="$CASE_TARGET_IDENTITY"
run_postgres
assert_failure_before_restore "live target name mismatch" "live target database name does not match"

reset_postgres_case
CASE_IDENTITY_UNAVAILABLE_SERVICE='bluey_restore_production'
run_postgres
assert_failure_before_restore "unavailable production identity" "could not prove the live production"

reset_postgres_case
CASE_IDENTITY_UNAVAILABLE_SERVICE='bluey_restore_target'
run_postgres
assert_failure_before_restore "unavailable target identity" "could not prove the live restore-target"

reset_postgres_case
CASE_PRODUCTION_IDENTITY_AFTER="10.1.0.2|5433|1725000400.500|$CASE_PRODUCTION_CLUSTER|16384|bluey_prod"
run_postgres
assert_failure_before_restore "production identity drift" "identity drifted during preflight"

# Checksum source paths accept current basenames, live absolute hourly paths,
# optional binary markers, and exact same-day legacy daily remaps only.
reset_postgres_case
write_sidecar "$PG_BACKUP" "*/var/backups/bluey-api/hourly/$(basename "$PG_BACKUP")"
run_postgres
assert_postgres_success "absolute hourly sidecar"
grep -Fq "service=bluey_restore_production host=prod.invalid port=5433" \
    "$SERVICE_LOG" || fail "production URI components did not reach its libpq service"
grep -Fq "service=bluey_restore_target host=drill.invalid port=6543" \
    "$SERVICE_LOG" || fail "target URI components did not reach its libpq service"
grep -Fq "dbname=bluey_prod sslmode=require connect_timeout=10 passfile=set" \
    "$SERVICE_LOG" || fail "production libpq service fields are incomplete"
grep -Fq 'inet_server_addr' "$ARGV_LOG" || fail "accessible identity query was not used"
grep -Fq 'host(pg_catalog.inet_server_addr())' "$ARGV_LOG" ||
    fail "identity query did not canonicalize PostgreSQL inet output"
if [[ '127.0.0.1/32' =~ ^[0-9A-Fa-f:.]+$ ]]; then
    fail "legacy CIDR-suffixed inet output unexpectedly passed strict IP validation"
fi
[[ '127.0.0.1' =~ ^[0-9A-Fa-f:.]+$ ]] ||
    fail "canonical host(inet_server_addr()) output failed strict IP validation"
if grep -Fq 'pg_control_system' "$ARGV_LOG"; then
    fail "restore drill still depends on privileged pg_control_system"
fi
grep -Fq $'pg_restore\t--list\t--\t' "$ARGV_LOG" ||
    fail "catalog validation lacks its option terminator"
grep -Fq $'pg_restore\t--dbname=bluey_restore_drill_safe\t--clean' \
    "$ARGV_LOG" || fail "destructive restore did not connect to the validated target database"
grep -Fq $'timeout\t--signal=TERM\t--kill-after=5s\t60s\tpython3' \
    "$ARGV_LOG" || fail "libpq service parsing is not bounded"
grep -Fq $'timeout\t--signal=TERM\t--kill-after=10s\t300s\tcp' \
    "$ARGV_LOG" || fail "stable backup copy is not bounded"
grep -Fq $'timeout\t--signal=TERM\t--kill-after=10s\t60s\tpg_restore\t--list' \
    "$ARGV_LOG" || fail "PostgreSQL catalog validation is not bounded"
grep -Fq 'pg_advisory_lock' "$ARGV_LOG" ||
    fail "provider/target advisory lease holder was not exercised"
grep -Fq 'pg_try_advisory_lock' "$ARGV_LOG" ||
    fail "provider/target advisory lease ownership was not proved"

reset_postgres_case
CASE_PROVIDER_TARGET_LEASE_CONFLICT=1
run_postgres
assert_failure_before_restore "cross-host provider target lease" \
    "could not acquire the provider/target-scoped restore lease"

reset_postgres_case
CASE_EXPIRES_AT_EPOCH=$(( $(date -u +%s) + 1200 ))
CASE_TARGET_SENTINEL="bluey-restore-drill-disposable:v1:$CASE_TARGET_NAME:$CASE_EXPIRES_AT_EPOCH:$CASE_SENTINEL_TOKEN"
CASE_TARGET_SENTINEL_AFTER="$CASE_TARGET_SENTINEL"
run_postgres
assert_failure_before_restore "short restore lease horizon" \
    "expiry must cover restore, verification, and teardown bounds"

reset_postgres_case
CASE_ENV_DIR="$SECRET_ENV_DIR"
run_postgres
assert_postgres_success "sourced-secret child-environment scrub"

reset_postgres_case
CASE_BACKUP_FILE="$PG_DAILY_BACKUP"
write_sidecar "$PG_DAILY_BACKUP" \
    '/var/backups/bluey-api/hourly/bluey-postgres-20260830T235959Z.pgdump'
run_postgres
assert_postgres_success "legacy daily PostgreSQL sidecar"

reset_postgres_case
CASE_BACKUP_FILE=''
CASE_BACKUP_DIR="$TEST_ROOT/storage-policy-must-override-this"
CASE_EFFECTIVE_BACKUP_DIR="$MIXED_BACKUP_DIR"
CASE_ENV_DIR="$STORAGE_ENV_DIR"
run_postgres
assert_postgres_success "active-backend latest backup selection"
grep -Fq "backup_file=$MIXED_PG_BACKUP" "$STDOUT_FILE" ||
    fail "PostgreSQL discovery selected the newer legacy SQLite backup"

reset_postgres_case
write_sidecar "$PG_BACKUP" '/var/backups/bluey-api/hourly/not-bluey.pgdump'
run_postgres
assert_failure_before_restore "unsafe sidecar source" "unsafe source name"

reset_postgres_case
write_sidecar "$PG_BACKUP" '/var/backups/bluey-api/hourly/bluey-20260830T120000Z.db'
run_postgres
assert_failure_before_restore "sidecar extension mismatch" "source extension does not match"

reset_postgres_case
printf '%064d  %s\n' 0 "$(basename "$PG_BACKUP")" >"${PG_BACKUP}.sha256"
run_postgres
assert_failure_before_restore "checksum mismatch" "backup checksum does not match"

reset_postgres_case
CASE_CATALOG_FAIL=1
run_postgres
assert_failure_before_restore "catalog failure" "pg_restore rejected the backup catalog"

# Every target needs an exact target-only sentinel. Same-server/different-DB
# drills additionally need an explicit override and a safe audit reference.
reset_postgres_case
CASE_TARGET_SENTINEL='wrong-sentinel'
CASE_TARGET_SENTINEL_AFTER="$CASE_TARGET_SENTINEL"
run_postgres
assert_failure_before_restore "missing target sentinel" "lacks its exact disposable-target sentinel"

reset_postgres_case
CASE_TARGET_OBJECT_COUNT=3
run_postgres
assert_failure_before_restore "stale restore target" "contains stale user objects"

reset_postgres_case
CASE_TARGET_OWNER=0
run_postgres
assert_failure_before_restore "unowned restore target" "must directly own the disposable database"

reset_postgres_case
CASE_PRODUCTION_SENTINEL="$CASE_TARGET_SENTINEL"
run_postgres
assert_failure_before_restore "sentinel present on production" \
    "production unexpectedly carries the disposable-target sentinel"

reset_postgres_case
CASE_TARGET_CLUSTER="$CASE_PRODUCTION_CLUSTER"
CASE_TARGET_IDENTITY="10.1.0.1|5433|1725000000.100|$CASE_TARGET_CLUSTER|16385|bluey_restore_drill_safe"
CASE_TARGET_IDENTITY_AFTER="$CASE_TARGET_IDENTITY"
CASE_TARGET_ADMIN_IDENTITY="10.1.0.1|5433|1725000000.100|$CASE_TARGET_CLUSTER|5|postgres"
run_postgres
assert_failure_before_restore "same-server default" "require an explicit audited override"

reset_postgres_case
CASE_TARGET_CLUSTER="$CASE_PRODUCTION_CLUSTER"
CASE_TARGET_IDENTITY="10.1.0.1|5433|1725000000.100|$CASE_TARGET_CLUSTER|16385|bluey_restore_drill_safe"
CASE_TARGET_IDENTITY_AFTER="$CASE_TARGET_IDENTITY"
CASE_TARGET_ADMIN_IDENTITY="10.1.0.1|5433|1725000000.100|$CASE_TARGET_CLUSTER|5|postgres"
CASE_ALLOW_SAME_CLUSTER=1
run_postgres
assert_failure_before_restore "same-server missing audit" "requires a safe audit reference"

reset_postgres_case
CASE_TARGET_CLUSTER="$CASE_PRODUCTION_CLUSTER"
CASE_TARGET_IDENTITY="10.1.0.1|5433|1725000000.100|$CASE_TARGET_CLUSTER|16385|bluey_restore_drill_safe"
CASE_TARGET_IDENTITY_AFTER="$CASE_TARGET_IDENTITY"
CASE_TARGET_ADMIN_IDENTITY="10.1.0.1|5433|1725000000.100|$CASE_TARGET_CLUSTER|5|postgres"
CASE_ALLOW_SAME_CLUSTER=1
CASE_AUDIT_REF='FIX-restore-drill-20260830'
run_postgres
assert_postgres_success "audited same-server disposable target"
grep -Fq 'restore_authority=same-logical-cluster:FIX-restore-drill-20260830' "$STDOUT_FILE" ||
    fail "same-server audit authority is missing from the summary"

# A failed or drifting post-restore proof must clean the protected service,
# catalog, and error files even though pg_restore was attempted.
reset_postgres_case
CASE_RESTORE_FAIL=1
run_postgres
[ "$RUN_STATUS" -ne 0 ] || fail "pg_restore failure unexpectedly succeeded"
grep -Fq 'pg_restore failed for the verified drill target' "$STDERR_FILE" ||
    fail "pg_restore failure was not reported safely"
[ "$(wc -l <"$RESTORE_LOG" | tr -d ' ')" = "1" ] ||
    fail "pg_restore failure was not exercised"
[ "$(wc -l <"$TEARDOWN_LOG" | tr -d ' ')" = "1" ] ||
    fail "pg_restore failure did not tear down its disposable target"
assert_no_secret_output "pg_restore failure"
assert_no_temp_files "pg_restore failure"

reset_postgres_case
CASE_PRODUCTION_IDENTITY_FINAL="10.1.0.9|5433|1725000999.900|$CASE_PRODUCTION_CLUSTER|16384|bluey_prod"
run_postgres
[ "$RUN_STATUS" -ne 0 ] || fail "final production identity drift unexpectedly succeeded"
grep -Fq 'production PostgreSQL identity changed during the restore drill' "$STDERR_FILE" ||
    fail "final production non-impact drift was not reported"
[ "$(wc -l <"$RESTORE_LOG" | tr -d ' ')" = "1" ] ||
    fail "final production drift did not exercise restore"
[ "$(wc -l <"$TEARDOWN_LOG" | tr -d ' ')" = "1" ] ||
    fail "final production drift did not tear down the target"
assert_no_secret_output "final production identity drift"
assert_no_temp_files "final production identity drift"

reset_postgres_case
CASE_TARGET_SENTINEL_AFTER='sentinel-lost-after-restore'
run_postgres
[ "$RUN_STATUS" -ne 0 ] || fail "lost post-restore sentinel unexpectedly succeeded"
grep -Fq 'lost its disposable-target sentinel' "$STDERR_FILE" ||
    fail "lost post-restore sentinel was not reported"
[ "$(wc -l <"$RESTORE_LOG" | tr -d ' ')" = "1" ] ||
    fail "post-restore sentinel test did not exercise pg_restore"
assert_no_secret_output "lost post-restore sentinel"
assert_no_temp_files "lost post-restore sentinel"

# SQLite accepts live absolute and exact legacy daily source names while always
# deleting its scratch database on both failure and success.
run_sqlite() {
    local backup="$1" integrity="$2"
    : >"$ARGV_LOG"
    : >"$SERVICE_LOG"
    : >"$RESTORE_LOG"
    : >"$TEARDOWN_LOG"
    : >"$STDOUT_FILE"
    : >"$STDERR_FILE"
    rm -f -- "$TARGET_DROPPED_FILE"
    write_state sqlite_integrity "$integrity"
    set +e
    BLUEY_ENV_DIR="$EMPTY_ENV" \
    BLUEY_SERVER_DB_BACKEND=sqlite \
    BLUEY_BACKUP_DIR="$TEST_ROOT" \
    BLUEY_RESTORE_DRILL_BACKUP_FILE="$backup" \
    BLUEY_RESTORE_DRILL_MAX_BACKUP_BYTES=1048576 \
    BLUEY_RESTORE_DRILL_MIN_FREE_GB=0 \
    BLUEY_RESTORE_DRILL_REQUIRE_TRUSTED_PATHS=1 \
    BLUEY_RESTORE_DRILL_COMMAND_TIMEOUT_SECONDS=60 \
    BLUEY_RESTORE_DRILL_COPY_TIMEOUT_SECONDS=60 \
    TMPDIR="$TMP_AREA" \
    PATH="$MOCK_BIN:$ORIGINAL_PATH" \
        "$SCRIPT" >"$STDOUT_FILE" 2>"$STDERR_FILE"
    RUN_STATUS=$?
    set -e
}

write_sidecar "$SQLITE_BACKUP" \
    "*/var/backups/bluey-api/hourly/$(basename "$SQLITE_BACKUP")"
run_sqlite "$SQLITE_BACKUP" failed
[ "$RUN_STATUS" -ne 0 ] || fail "corrupt SQLite fixture unexpectedly succeeded"
assert_no_temp_files "failed SQLite drill"

run_sqlite "$SQLITE_BACKUP" ok
[ "$RUN_STATUS" = "0" ] || fail "absolute-hourly SQLite fixture failed"
grep -Fq 'backend=sqlite' "$STDOUT_FILE" || fail "SQLite summary is missing"
assert_no_temp_files "successful SQLite drill"

write_sidecar "$SQLITE_DAILY_BACKUP" \
    '/var/backups/bluey-api/hourly/bluey-20260830T235959Z.db'
run_sqlite "$SQLITE_DAILY_BACKUP" ok
[ "$RUN_STATUS" = "0" ] || fail "legacy daily SQLite fixture failed"
assert_no_temp_files "legacy daily SQLite drill"

# A hard-killed orchestrator must not leave the effectful pg_restore child
# running. Local files may remain for forensic recovery; the external provider
# TTL/drop mechanism remains the separate release gate for target cleanup.
reset_postgres_case
CASE_RESTORE_BLOCK=1
rm -f -- "$TEST_ROOT/blocked-restore-pid"
run_postgres
[ "$RUN_STATUS" -ne 0 ] || fail "SIGKILL containment scenario unexpectedly succeeded"
for _ in $(seq 1 50); do
    kill -0 "$BLOCKED_RESTORE_PID" 2>/dev/null || break
    sleep 0.1
done
if kill -0 "$BLOCKED_RESTORE_PID" 2>/dev/null; then
    fail "SIGKILL left the destructive pg_restore child running"
fi
find "$TMP_AREA" -mindepth 1 -maxdepth 1 \
    \( -name 'bluey-restore-drill-*' \) -exec rm -rf -- {} +
rm -rf -- "$TEST_ROOT/.restore-drill-locks"
fi

run_real_postgres_integration() {
    local real_root="$TEST_ROOT/real-postgres"
    local production_log="$real_root/production.log"
    local target_log="$real_root/target.log"
    local password_file="$real_root/superuser-password"
    local production_port target_port production_url target_url
    local production_count target_exists
    local safe_production_identity
    local backup_root="$real_root/backups"
    local real_backup="$backup_root/hourly/bluey-postgres-20260830T140000Z.pgdump"
    local real_deadman="$backup_root/deadman/bluey_restore_drill_real.lease"
    local real_stdout="$real_root/restore.stdout" real_stderr="$real_root/restore.stderr"
    local real_expiry=$(( $(date -u +%s) + 3600 ))
    local real_token='abcdef0123456789abcdef0123456789'
    local production_cluster='production-real-cluster-0001'
    local target_cluster='target-real-cluster-00000001'
    local drill_authority='audit:real-local-integration'
    local provider_identity='provider:real-local-test-monitor'
    local encoded_password='local%3Ap%5Cass%23word'
    local password='local:p\ass#word'
    local setup_env=(env PGPASSWORD="$password" PGCONNECT_TIMEOUT=5)
    local required

    for required in initdb pg_ctl psql pg_dump pg_restore createdb python3 timeout; do
        if ! PATH="$ORIGINAL_PATH" command -v "$required" >/dev/null 2>&1; then
            [ "${BLUEY_RESTORE_DRILL_REAL_ONLY:-0}" != "1" ] ||
                fail "REAL_ONLY requires PostgreSQL tool: $required"
            printf 'real_postgres_scenario=skipped missing=%s\n' "$required"
            return 0
        fi
    done
    mkdir -p "$real_root" \
        "$backup_root/hourly" "$backup_root/daily" "$backup_root/deadman"
    chmod 700 "$real_root" "$backup_root" \
        "$backup_root/hourly" "$backup_root/daily" "$backup_root/deadman"
    REAL_PRODUCTION_DATA="$real_root/production-data"
    REAL_TARGET_DATA="$real_root/target-data"
    printf '%s\n' "$password" > "$password_file"
    chmod 600 "$password_file"
    production_port="$(PATH="$ORIGINAL_PATH" python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"
    target_port="$(PATH="$ORIGINAL_PATH" python3 - <<'PY'
import socket
with socket.socket() as sock:
    sock.bind(("127.0.0.1", 0))
    print(sock.getsockname()[1])
PY
)"
    [ "$production_port" != "$target_port" ] || fail "real integration selected duplicate ports"

    PATH="$ORIGINAL_PATH" timeout 60s initdb -D "$REAL_PRODUCTION_DATA" \
        -U blueytest --auth-local=trust --auth-host=scram-sha-256 \
        --pwfile="$password_file" >/dev/null
    PATH="$ORIGINAL_PATH" timeout 60s initdb -D "$REAL_TARGET_DATA" \
        -U blueytest --auth-local=trust --auth-host=scram-sha-256 \
        --pwfile="$password_file" >/dev/null
    PATH="$ORIGINAL_PATH" timeout 60s pg_ctl -D "$REAL_PRODUCTION_DATA" \
        -l "$production_log" -o "-p $production_port -h 127.0.0.1 -k /tmp -c bluey.logical_cluster_id=$production_cluster" \
        -w start >/dev/null
    PATH="$ORIGINAL_PATH" timeout 60s pg_ctl -D "$REAL_TARGET_DATA" \
        -l "$target_log" -o "-p $target_port -h 127.0.0.1 -k /tmp -c bluey.logical_cluster_id=$target_cluster" \
        -w start >/dev/null

    PATH="$ORIGINAL_PATH" "${setup_env[@]}" createdb -h 127.0.0.1 \
        -p "$production_port" -U blueytest -T template0 bluey_prod
    PATH="$ORIGINAL_PATH" "${setup_env[@]}" createdb -h 127.0.0.1 \
        -p "$target_port" -U blueytest -T template0 bluey_restore_drill_real
    PATH="$ORIGINAL_PATH" "${setup_env[@]}" psql -X -v ON_ERROR_STOP=1 -q \
        -h 127.0.0.1 -p "$production_port" -U blueytest -d bluey_prod \
        -c 'CREATE TABLE accounts (id bigint PRIMARY KEY); INSERT INTO accounts VALUES (1), (2); CREATE TABLE usage_events (id bigint PRIMARY KEY); INSERT INTO usage_events VALUES (1), (2), (3);'
    PATH="$ORIGINAL_PATH" "${setup_env[@]}" psql -X -v ON_ERROR_STOP=1 -q \
        -h 127.0.0.1 -p "$target_port" -U blueytest -d postgres \
        -c "COMMENT ON DATABASE bluey_restore_drill_real IS 'bluey-restore-drill-disposable:v1:bluey_restore_drill_real:$real_expiry:$real_token';"
    PATH="$ORIGINAL_PATH" "${setup_env[@]}" pg_dump -Fc --no-owner --no-acl \
        -h 127.0.0.1 -p "$production_port" -U blueytest -d bluey_prod \
        -f "$real_backup"
    safe_production_identity="$(PATH="$ORIGINAL_PATH" "${setup_env[@]}" \
        psql -X -Atq -h 127.0.0.1 -p "$production_port" -U blueytest -d bluey_prod \
        -c "SELECT host(inet_server_addr()), inet_server_port()::text, EXTRACT(EPOCH FROM pg_postmaster_start_time())::text, current_setting('bluey.logical_cluster_id', true), d.oid::text, d.datname FROM pg_database AS d WHERE d.datname = current_database();")"
    chmod 600 "$real_backup"
    write_sidecar "$real_backup"
    chmod 600 "${real_backup}.sha256"
    printf 'bluey-restore-drill-deadman:v1:%s:%s:%s:%s:%s:%s\n' \
        "$target_cluster" bluey_restore_drill_real "$real_expiry" "$real_token" \
        "$drill_authority" "$provider_identity" > "$real_deadman"
    chmod 600 "$real_deadman"
    production_url="postgresql://blueytest:$encoded_password@127.0.0.1:$production_port/bluey_prod?sslmode=disable"
    target_url="postgresql://blueytest:$encoded_password@127.0.0.1:$target_port/bluey_restore_drill_real?sslmode=disable"

    if ! BLUEY_ENV_DIR="$EMPTY_ENV" BLUEY_SERVER_DB_BACKEND=postgres \
        BLUEY_BACKUP_DIR="$backup_root" BLUEY_RESTORE_DRILL_BACKUP_FILE="$real_backup" \
        BLUEY_DATABASE_URL="$production_url" BLUEY_RESTORE_DRILL_DATABASE_URL="$target_url" \
        BLUEY_RESTORE_DRILL_DATABASE_NAME=bluey_restore_drill_real \
        BLUEY_RESTORE_DRILL_CONFIRMATION=restore:bluey_restore_drill_real \
        BLUEY_RESTORE_DRILL_SENTINEL_TOKEN="$real_token" \
        BLUEY_RESTORE_DRILL_TARGET_EXPIRES_AT_EPOCH="$real_expiry" \
        BLUEY_RESTORE_DRILL_TEARDOWN_MODE=drop \
        BLUEY_RESTORE_DRILL_PRODUCTION_CLUSTER_SENTINEL="$production_cluster" \
        BLUEY_RESTORE_DRILL_TARGET_CLUSTER_SENTINEL="$target_cluster" \
        BLUEY_RESTORE_DRILL_AUTHORITY="$drill_authority" \
        BLUEY_RESTORE_DRILL_DEADMAN_PROVIDER_IDENTITY="$provider_identity" \
        BLUEY_RESTORE_DRILL_DEADMAN_MARKER_FILE="$real_deadman" \
        BLUEY_RESTORE_DRILL_MAX_BACKUP_BYTES=10485760 \
        BLUEY_RESTORE_DRILL_MIN_FREE_GB=0 BLUEY_RESTORE_DRILL_REQUIRE_TRUSTED_PATHS=1 \
        BLUEY_RESTORE_DRILL_CONNECT_TIMEOUT_SECONDS=5 \
        BLUEY_RESTORE_DRILL_COMMAND_TIMEOUT_SECONDS=30 \
        BLUEY_RESTORE_DRILL_COPY_TIMEOUT_SECONDS=30 \
        BLUEY_RESTORE_DRILL_RESTORE_TIMEOUT_SECONDS=120 \
        TMPDIR="$TMP_AREA" PATH="$ORIGINAL_PATH" \
        "$SCRIPT" > "$real_stdout" 2> "$real_stderr"; then
        tail -n 20 "$real_stderr" >&2 || true
        printf 'safe_production_identity=%s\n' "$safe_production_identity" >&2
        tail -n 20 "$production_log" >&2 || true
        tail -n 20 "$target_log" >&2 || true
        fail "real isolated PostgreSQL restore drill failed"
    fi
    grep -Fq 'backend=postgres' "$real_stdout" || fail "real restore summary is missing"
    grep -Fq 'accounts=2' "$real_stdout" || fail "real restored account count is wrong"
    grep -Fq 'usage_events=3' "$real_stdout" || fail "real restored usage count is wrong"
    [ ! -e "$real_deadman" ] || fail "real restore did not consume its local dead-man marker"
    production_count="$(PATH="$ORIGINAL_PATH" "${setup_env[@]}" psql -X -Atq \
        -h 127.0.0.1 -p "$production_port" -U blueytest -d bluey_prod \
        -c 'select count(*) from accounts')"
    [ "$production_count" = "2" ] || fail "real restore changed production data"
    target_exists="$(PATH="$ORIGINAL_PATH" "${setup_env[@]}" psql -X -Atq \
        -h 127.0.0.1 -p "$target_port" -U blueytest -d postgres \
        -c "select count(*) from pg_database where datname='bluey_restore_drill_real'")"
    [ "$target_exists" = "0" ] || fail "real disposable target was not dropped"
    assert_no_temp_files "real PostgreSQL integration"
    PATH="$ORIGINAL_PATH" timeout 60s pg_ctl -D "$REAL_PRODUCTION_DATA" -m fast -w stop \
        >/dev/null
    PATH="$ORIGINAL_PATH" timeout 60s pg_ctl -D "$REAL_TARGET_DATA" -m fast -w stop \
        >/dev/null
    REAL_PRODUCTION_DATA=""
    REAL_TARGET_DATA=""
    REAL_SCENARIO_EXECUTED=1
    echo "real_postgres_scenario=executed"
}

REAL_SCENARIO_EXECUTED=0
run_real_postgres_integration
[ "${BLUEY_RESTORE_DRILL_REAL_ONLY:-0}" != "1" ] ||
    [ "$REAL_SCENARIO_EXECUTED" = "1" ] ||
    fail "REAL_ONLY did not execute a real PostgreSQL restore scenario"

echo "test-restore-drill-bluey-db: PASS"
