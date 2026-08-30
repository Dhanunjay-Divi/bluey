#!/usr/bin/env bash

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
TEST_ROOT="$(mktemp -d "${TMPDIR:-/tmp}/bluey-storage-guards-test.XXXXXX")"
TRUST_TEST_ROOT="$(mktemp -d "$ROOT/.bluey-storage-trust-test.XXXXXX")"
export BLUEY_ENV_FILE="$TEST_ROOT/missing.env"
export BLUEY_STORAGE_ENV_FILE="$TEST_ROOT/missing-storage.env"
export BLUEY_EXTRA_ENV_FILES="$TEST_ROOT/missing-extra.env"
export BLUEY_BACKUP_REQUIRE_TRUSTED_PATHS=0
export BLUEY_LOG_ARCHIVE_REQUIRE_TRUSTED_PATHS=0
export BLUEY_DISK_GUARD_REQUIRE_TRUSTED_PATHS=0
export BLUEY_OPS_STATE_ROOT="$TEST_ROOT/ops-state"
export AWS_ACCESS_KEY_ID=test-backup-key
export AWS_SECRET_ACCESS_KEY=test-backup-secret
mkdir -p "$BLUEY_OPS_STATE_ROOT"

cleanup() {
    rm -rf "$TEST_ROOT" "$TRUST_TEST_ROOT"
}
trap cleanup EXIT

fail() {
    echo "test-bluey-storage-guards: FAIL: $*" >&2
    exit 1
}

sha256_file() {
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum "$1" | awk '{print $1}'
    else
        shasum -a 256 "$1" | awk '{print $1}'
    fi
}

file_size() {
    if stat -c%s "$1" >/dev/null 2>&1; then
        stat -c%s "$1"
    else
        stat -f%z "$1"
    fi
}

MOCK_BIN="$TEST_ROOT/mock-bin"
mkdir -p "$MOCK_BIN"

cat > "$MOCK_BIN/date" <<'SH'
#!/usr/bin/env bash
format="${!#}"
case "$format" in
    +%Y%m%dT%H%M%SZ) printf '%s\n' "${MOCK_DATE_TS:-20260830T120000Z}" ;;
    +%Y%m%d) printf '%s\n' "${MOCK_DATE_TS:-20260830T120000Z}" | cut -c1-8 ;;
    +%H) printf '%s\n' "${MOCK_DATE_HOUR:-12}" ;;
    +%FT%TZ) printf '%s\n' "${MOCK_DATE_ISO:-2026-08-30T12:00:00Z}" ;;
    +%s) printf '%s\n' "${MOCK_DATE_EPOCH:-1788091200}" ;;
    *) /bin/date "$@" ;;
esac
SH

cat > "$MOCK_BIN/flock" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "${MOCK_FLOCK_LOG:?}"
[ "${MOCK_FLOCK_ACTIVE_FD:-}" = "" ] || {
    case " $* " in
        *" -sn ${MOCK_FLOCK_ACTIVE_FD} "*) exit 1 ;;
    esac
}
[ "${MOCK_FLOCK_FAIL:-0}" != "1" ]
SH

cat > "$MOCK_BIN/df" <<'SH'
#!/usr/bin/env bash
if [ "${1:-}" = "-Pk" ]; then
    count=0
    if [ -f "${MOCK_DF_COUNT_FILE:?}" ]; then
        count="$(cat "$MOCK_DF_COUNT_FILE")"
    fi
    count=$((count + 1))
    printf '%s\n' "$count" > "$MOCK_DF_COUNT_FILE"
    mode="${MOCK_DF_MODE:-healthy}"
    if [ "$mode" = "transition" ] && [ "$count" -gt 1 ]; then
        mode=healthy
    fi
    echo 'Filesystem 1024-blocks Used Available Capacity Mounted on'
    if [ "$mode" = "high" ] || { [ "$mode" = "transition" ] && [ "$count" -le 1 ]; }; then
        echo '/dev/mock 60000000 51000000 5242880 85% /'
    elif [ "$mode" = "warning" ]; then
        echo '/dev/mock 60000000 43000000 12582912 72% /'
    else
        echo '/dev/mock 60000000 40000000 20971520 67% /'
    fi
else
    echo 'Filesystem Size Used Avail Capacity Mounted on'
    echo '/dev/mock 58G 39G 20G 67% /'
fi
SH

cat > "$MOCK_BIN/aws" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "${MOCK_AWS_LOG:?}"
if [ "${1:-}" = "--endpoint-url" ]; then
    shift 2
fi
case "${1:-} ${2:-}" in
    's3 cp')
        source="$3"
        destination="$4"
        if [ "$destination" = "-" ]; then
            local_path="${MOCK_S3_ROOT:?}/${source#s3://}"
            if [ "${MOCK_AWS_CORRUPT_READBACK:-0}" = "1" ] &&
                [[ "$source" != *.sha256 ]]; then
                printf 'corrupt-readback'
            else
                /bin/cat "$local_path"
            fi
        else
            local_path="${MOCK_S3_ROOT:?}/${destination#s3://}"
            if [ "${MOCK_AWS_DENY_OVERWRITE:-0}" = "1" ] && [ -e "$local_path" ]; then
                echo "immutable mock object cannot be overwritten: $destination" >&2
                exit 77
            fi
            mkdir -p "$(dirname "$local_path")"
            /bin/cp "$source" "$local_path"
        fi
        ;;
    's3api head-object')
        shift 2
        bucket=""
        key=""
        while [ "$#" -gt 0 ]; do
            case "$1" in
                --bucket) bucket="$2"; shift 2 ;;
                --key) key="$2"; shift 2 ;;
                *) shift ;;
            esac
        done
        target="${MOCK_S3_ROOT:?}/$bucket/$key"
        if stat -c%s "$target" >/dev/null 2>&1; then
            stat -c%s "$target"
        else
            stat -f%z "$target"
        fi
        ;;
    *)
        echo "unexpected mock aws command: $*" >&2
        exit 2
        ;;
esac
SH

cat > "$MOCK_BIN/journalctl" <<'SH'
#!/usr/bin/env bash
exit 0
SH

cat > "$MOCK_BIN/logrotate" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "${MOCK_LOGROTATE_LOG:?}"
exit 0
SH

cat > "$MOCK_BIN/curl" <<'SH'
#!/usr/bin/env bash
[ -z "${BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL:-}" ] || {
    echo "guard webhook leaked to curl environment" >&2
    exit 90
}
printf '%s\n' "$*" >> "${MOCK_CURL_LOG:?}"
if [ -n "${MOCK_CURL_STDIN_LOG:-}" ]; then
    /bin/cat >> "$MOCK_CURL_STDIN_LOG" || true
fi
[ "${MOCK_CURL_FAIL:-0}" != "1" ]
SH

cat > "$MOCK_BIN/sqlite3" <<'SH'
#!/usr/bin/env bash
[ -z "${AWS_SECRET_ACCESS_KEY:-}" ] || {
    echo "backup credential leaked to database helper" >&2
    exit 90
}
source_db="$1"
command="$2"
if [ "$command" = "PRAGMA quick_check;" ]; then
    [ "${MOCK_SQLITE_INVALID:-0}" != "1" ] || exit 1
    printf 'ok\n'
    exit 0
fi
target="${command#*\'}"
target="${target%\'*}"
[ -n "$source_db" ] && [ -n "$target" ]
/bin/cp "$source_db" "$target"
SH

cat > "$MOCK_BIN/pg_restore" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$*" >> "${MOCK_PG_RESTORE_LOG:?}"
[ "${MOCK_PG_RESTORE_INVALID:-0}" != "1" ]
SH

chmod +x "$MOCK_BIN"/*

MOCK_FLOCK_LOG="$TEST_ROOT/flock.log"
MOCK_DF_COUNT_FILE="$TEST_ROOT/df-count"
MOCK_S3_ROOT="$TEST_ROOT/s3"
MOCK_AWS_LOG="$TEST_ROOT/aws.log"
MOCK_CURL_LOG="$TEST_ROOT/curl.log"
MOCK_CURL_STDIN_LOG="$TEST_ROOT/curl-stdin.log"
MOCK_PG_RESTORE_LOG="$TEST_ROOT/pg-restore.log"
MOCK_LOGROTATE_LOG="$TEST_ROOT/logrotate.log"
export MOCK_FLOCK_LOG MOCK_DF_COUNT_FILE MOCK_S3_ROOT MOCK_AWS_LOG MOCK_CURL_LOG
export MOCK_CURL_STDIN_LOG MOCK_PG_RESTORE_LOG
export MOCK_LOGROTATE_LOG
: > "$MOCK_LOGROTATE_LOG"

BACKUP_ROOT="$TEST_ROOT/backups"
DB_PATH="$TEST_ROOT/bluey.db"
printf 'deterministic-sqlite-fixture' > "$DB_PATH"
mkdir -p "$BACKUP_ROOT/.staging/run.orphan" "$BACKUP_ROOT/hourly"
printf 'orphaned-partial' > "$BACKUP_ROOT/.staging/run.orphan/partial.db"
printf 'orphaned-marker' > "$BACKUP_ROOT/.staging/orphan.offsite-verified.tmp.dead"

run_backup() {
    local timestamp="$1"
    : > "$MOCK_DF_COUNT_FILE"
    PATH="$MOCK_BIN:$PATH" \
    BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
    BLUEY_EXTRA_ENV_FILES="$TEST_ROOT/missing-extra.env" \
    BLUEY_SERVER_DB_BACKEND=sqlite \
    BLUEY_DB_PATH="$DB_PATH" \
    BLUEY_BACKUP_DIR="$BACKUP_ROOT" \
    BLUEY_BACKUP_HOURLY_KEEP=3 \
    BLUEY_BACKUP_DAILY_KEEP=2 \
    BLUEY_BACKUP_MIN_HOURLY_KEEP=2 \
    BLUEY_BACKUP_MIN_DAILY_KEEP=1 \
    BLUEY_BACKUP_LOCAL_MAX_BYTES=0 \
    BLUEY_BACKUP_MIN_FREE_GB=8 \
    BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
    OFFSITE_DESTINATION=s3://bluey-test/backups/api/ \
    MOCK_DATE_TS="$timestamp" \
    MOCK_DF_MODE=healthy \
        "$ROOT/ops/backup-bluey-db.sh"
}

run_backup 20260830T120001Z >/dev/null
first_backup="$BACKUP_ROOT/hourly/bluey-20260830T120001Z.db"
[ -f "$first_backup" ] || fail "finalized backup is missing"
[ -f "${first_backup}.sha256" ] || fail "checksum sidecar is missing"
[ -f "${first_backup}.offsite-verified" ] || fail "offsite verification marker is missing"
[ -z "$(find "$BACKUP_ROOT/.staging" -mindepth 1 -print -quit)" ] ||
    fail "staging directory was not cleaned"
grep -Fqx 'status=ok' "$BACKUP_ROOT/.backup.status" ||
    fail "successful backup run status was not persisted"
grep -Fqx "sha256=$(sha256_file "$first_backup")" "${first_backup}.offsite-verified" ||
    fail "offsite marker does not bind the exact backup SHA"
grep -Fqx "bytes=$(file_size "$first_backup")" "${first_backup}.offsite-verified" ||
    fail "offsite marker does not bind the exact backup size"
grep -Eq '(^| )-n 9$' "$MOCK_FLOCK_LOG" || fail "backup did not take a nonblocking flock"

sed 's#^destination=.*#destination=s3://obsolete-prefix/bluey.db#' \
    "${first_backup}.offsite-verified" > "$TEST_ROOT/stale-marker"
mv "$TEST_ROOT/stale-marker" "${first_backup}.offsite-verified"
run_backup 20260830T120002Z >/dev/null
grep -Fqx "destination=s3://bluey-test/backups/api/hourly/$(basename "$first_backup")" \
    "${first_backup}.offsite-verified" ||
    fail "backup reconciliation trusted a marker from an obsolete destination"
run_backup 20260830T120003Z >/dev/null
run_backup 20260830T120004Z >/dev/null
[ ! -e "$first_backup" ] || fail "count retention did not remove the oldest archive"
[ ! -e "${first_backup}.sha256" ] || fail "count retention orphaned a checksum"
[ ! -e "${first_backup}.offsite-verified" ] || fail "count retention orphaned a proof marker"
[ "$(find "$BACKUP_ROOT/hourly" -type f -name '*.db' | wc -l | tr -d ' ')" = "3" ] ||
    fail "hourly count retention did not keep exactly three backups"

if MOCK_AWS_CORRUPT_READBACK=1 run_backup 20260830T120005Z >/dev/null 2>&1; then
    fail "corrupt offsite read-back was accepted"
fi
grep -Fqx 'status=fail' "$BACKUP_ROOT/.backup.status" ||
    fail "failed backup run status was not persisted"
corrupt_backup="$BACKUP_ROOT/hourly/bluey-20260830T120005Z.db"
[ -f "$corrupt_backup" ] || fail "failed offsite verification removed the local backup"
[ -f "${corrupt_backup}.sha256" ] || fail "failed offsite verification removed its checksum"
[ ! -e "${corrupt_backup}.offsite-verified" ] ||
    fail "failed offsite verification minted a proof marker"
touch -t 202001010000 "$corrupt_backup"
if MOCK_AWS_CORRUPT_READBACK=1 run_backup 20260830T120006Z >/dev/null 2>&1; then
    fail "persistent offsite outage created another backup"
fi
[ -f "$corrupt_backup" ] || fail "persistent outage removed the unverified local backup"
[ ! -e "$BACKUP_ROOT/hourly/bluey-20260830T120006Z.db" ] ||
    fail "persistent outage compounded the unverified backlog"
MOCK_AWS_DENY_OVERWRITE=1 run_backup 20260830T120006Z >/dev/null
recovered_backup="$BACKUP_ROOT/hourly/bluey-20260830T120006Z.db"
[ -f "${recovered_backup}.offsite-verified" ] ||
    fail "offsite recovery did not resume exact proof and new backups"
grep -Fqx 'status=ok' "$BACKUP_ROOT/.backup.status" ||
    fail "backup status did not recover after the offsite outage"
[ -f "$MOCK_S3_ROOT/bluey-test/backups/api/hourly/$(basename "$recovered_backup")" ] ||
    fail "new hourly backup did not use the hourly lifecycle prefix"
MOCK_DATE_HOUR=00 run_backup 20260831T000001Z >/dev/null
daily_backup="$BACKUP_ROOT/daily/bluey-20260831.db"
[ -f "$daily_backup" ] || fail "midnight run did not finalize a daily backup"
[ -f "$MOCK_S3_ROOT/bluey-test/backups/api/daily/$(basename "$daily_backup")" ] ||
    fail "new daily backup did not use the daily lifecycle prefix"
if find "$BACKUP_ROOT/hourly" -type f \( -name '*.db' -o -name '*.pgdump' \) \
    ! -exec test -f '{}.offsite-verified' \; -print | grep -q .; then
    fail "offsite recovery left an unverified finalized backlog"
fi

PARTIAL_REMOTE_ROOT="$TEST_ROOT/partial-remote-backups"
partial_remote_backup="$PARTIAL_REMOTE_ROOT/hourly/bluey-20260830T115900Z.db"
partial_remote_object="$MOCK_S3_ROOT/bluey-test/partial/hourly/$(basename "$partial_remote_backup")"
mkdir -p "$(dirname "$partial_remote_backup")" "$PARTIAL_REMOTE_ROOT/daily" \
    "$PARTIAL_REMOTE_ROOT/.staging" "$(dirname "$partial_remote_object")"
cp "$DB_PATH" "$partial_remote_backup"
partial_remote_sha="$(sha256_file "$partial_remote_backup")"
printf '%s  %s\n' "$partial_remote_sha" "$(basename "$partial_remote_backup")" \
    > "${partial_remote_backup}.sha256"
cp "$partial_remote_backup" "$partial_remote_object"
: > "$MOCK_DF_COUNT_FILE"
MOCK_AWS_DENY_OVERWRITE=1 PATH="$MOCK_BIN:$PATH" \
BLUEY_SERVER_DB_BACKEND=sqlite BLUEY_DB_PATH="$DB_PATH" \
BLUEY_BACKUP_DIR="$PARTIAL_REMOTE_ROOT" BLUEY_BACKUP_HOURLY_KEEP=4 \
BLUEY_BACKUP_DAILY_KEEP=2 BLUEY_BACKUP_MIN_HOURLY_KEEP=1 \
BLUEY_BACKUP_MIN_DAILY_KEEP=1 BLUEY_BACKUP_LOCAL_MAX_BYTES=0 \
BLUEY_BACKUP_MIN_FREE_GB=0 BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
OFFSITE_DESTINATION=s3://bluey-test/partial/ MOCK_DATE_TS=20260830T120000Z \
    "$ROOT/ops/backup-bluey-db.sh" >/dev/null
[ -f "${partial_remote_backup}.offsite-verified" ] ||
    fail "killed upload recovery did not mint proof for the surviving payload"
[ -f "${partial_remote_object}.sha256" ] ||
    fail "killed upload recovery did not upload only the missing sidecar"

COMPLETE_REMOTE_ROOT="$TEST_ROOT/complete-remote-backups"
complete_remote_backup="$COMPLETE_REMOTE_ROOT/hourly/bluey-20260830T115800Z.db"
complete_remote_object="$MOCK_S3_ROOT/bluey-test/complete/hourly/$(basename "$complete_remote_backup")"
mkdir -p "$(dirname "$complete_remote_backup")" "$COMPLETE_REMOTE_ROOT/daily" \
    "$COMPLETE_REMOTE_ROOT/.staging" "$(dirname "$complete_remote_object")"
cp "$DB_PATH" "$complete_remote_backup"
complete_remote_sha="$(sha256_file "$complete_remote_backup")"
printf '%s  %s\n' "$complete_remote_sha" "$(basename "$complete_remote_backup")" \
    > "${complete_remote_backup}.sha256"
cp "$complete_remote_backup" "$complete_remote_object"
cp "${complete_remote_backup}.sha256" "${complete_remote_object}.sha256"
: > "$MOCK_AWS_LOG"
: > "$MOCK_DF_COUNT_FILE"
MOCK_AWS_DENY_OVERWRITE=1 PATH="$MOCK_BIN:$PATH" \
BLUEY_SERVER_DB_BACKEND=sqlite BLUEY_DB_PATH="$DB_PATH" \
BLUEY_BACKUP_DIR="$COMPLETE_REMOTE_ROOT" BLUEY_BACKUP_HOURLY_KEEP=4 \
BLUEY_BACKUP_DAILY_KEEP=2 BLUEY_BACKUP_MIN_HOURLY_KEEP=1 \
BLUEY_BACKUP_MIN_DAILY_KEEP=1 BLUEY_BACKUP_LOCAL_MAX_BYTES=0 \
BLUEY_BACKUP_MIN_FREE_GB=0 BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
OFFSITE_DESTINATION=s3://bluey-test/complete/ MOCK_DATE_TS=20260830T120000Z \
    "$ROOT/ops/backup-bluey-db.sh" >/dev/null
[ -f "${complete_remote_backup}.offsite-verified" ] ||
    fail "complete immutable remote pair did not reconcile"
[ "$(grep -Fc "s3 cp s3://bluey-test/complete/hourly/$(basename "$complete_remote_backup") - --quiet" \
    "$MOCK_AWS_LOG")" = "1" ] ||
    fail "complete immutable pair was fully reread more than once"

if PATH="$MOCK_BIN:$PATH" \
    BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
    BLUEY_SERVER_DB_BACKEND=sqlite \
    BLUEY_DB_PATH="$DB_PATH" \
    BLUEY_BACKUP_DIR="$TEST_ROOT/invalid-config" \
    BLUEY_BACKUP_HOURLY_KEEP=not-a-number \
        "$ROOT/ops/backup-bluey-db.sh" >/dev/null 2>&1; then
    fail "invalid backup retention was accepted"
fi

if BLUEY_BACKUP_DIR='////' "$ROOT/ops/backup-bluey-db.sh" --check-config \
    >/dev/null 2>&1; then
    fail "backup accepted the filesystem root alias"
fi
if BLUEY_BACKUP_DIR='' "$ROOT/ops/backup-bluey-db.sh" --check-config \
    >/dev/null 2>&1; then
    fail "backup accepted an empty destructive root"
fi
if BLUEY_LOG_ARCHIVE_LOCAL_DIR='////' \
    BLUEY_LOG_ARCHIVE_WORK_DIR="$TEST_ROOT/safe/archive-work" \
    BLUEY_LOG_DIRS="$TEST_ROOT/safe/logs" \
    "$ROOT/ops/archive-bluey-logs.sh" --check-config >/dev/null 2>&1; then
    fail "log archive accepted the filesystem root alias"
fi
if BLUEY_LOG_ARCHIVE_LOCAL_DIR="$TEST_ROOT/safe/archives" \
    BLUEY_LOG_ARCHIVE_WORK_DIR='/var/tmp/../' \
    BLUEY_LOG_DIRS="$TEST_ROOT/safe/logs" \
    "$ROOT/ops/archive-bluey-logs.sh" --check-config >/dev/null 2>&1; then
    fail "log archive accepted an aliasing work root"
fi
if BLUEY_LOG_ARCHIVE_LOCAL_DIR="$TEST_ROOT/safe/archives" \
    BLUEY_LOG_ARCHIVE_WORK_DIR="$TEST_ROOT/safe/archive-work" \
    BLUEY_LOG_DIRS='/' \
    "$ROOT/ops/archive-bluey-logs.sh" --check-config >/dev/null 2>&1; then
    fail "log archive accepted a broad log prune root"
fi
if BLUEY_LOG_ARCHIVE_LOCAL_DIR="$TEST_ROOT/safe/archives" \
    BLUEY_LOG_ARCHIVE_WORK_DIR="$TEST_ROOT/safe/archive-work" \
    BLUEY_LOG_DIRS="$TEST_ROOT/safe/logs" \
    BLUEY_LOG_DIR_MAX_BYTES=not-a-number \
    "$ROOT/ops/archive-bluey-logs.sh" --check-config >/dev/null 2>&1; then
    fail "log archive accepted an invalid aggregate hot-log cap"
fi
if BLUEY_BACKUP_DIR="$TEST_ROOT/safe/backups" \
    BLUEY_DISK_GUARD_TMP_ROOT='/tmp/../' \
    BLUEY_DISK_GUARD_STATUS_FILE="$TEST_ROOT/safe/status/guard.status" \
    "$ROOT/ops/bluey-disk-guard.sh" --check-config >/dev/null 2>&1; then
    fail "disk guard accepted an aliasing temp prune root"
fi
if BLUEY_BACKUP_DIR="$TEST_ROOT/safe/backups" \
    BLUEY_DISK_GUARD_TMP_ROOT="$TEST_ROOT/safe/tmp" \
    BLUEY_DISK_GUARD_STATUS_FILE="$TEST_ROOT/safe/status/../escape/guard.status" \
    "$ROOT/ops/bluey-disk-guard.sh" --check-config >/dev/null 2>&1; then
    fail "disk guard accepted an aliasing status root"
fi
if BLUEY_BACKUP_DIR="$TEST_ROOT/safe/backups" \
    BLUEY_BACKUP_STATUS_FILE="$TEST_ROOT/safe/backups/.." \
    "$ROOT/ops/backup-bluey-db.sh" --check-config >/dev/null 2>&1; then
    fail "backup accepted an unsafe control-file basename"
fi
if BLUEY_BACKUP_DIR="$TEST_ROOT/safe/backups" \
    BLUEY_DISK_GUARD_TMP_ROOT="$TEST_ROOT/safe/tmp" \
    BLUEY_DISK_GUARD_STATUS_FILE="$TEST_ROOT/safe/status/." \
    "$ROOT/ops/bluey-disk-guard.sh" --check-config >/dev/null 2>&1; then
    fail "disk guard accepted an unsafe status-file basename"
fi
if BLUEY_API_ROOT='/' "$ROOT/ops/install-bluey-log-guards.sh" --check-config \
    >/dev/null 2>&1; then
    fail "installer accepted the filesystem root as API_ROOT"
fi
if BLUEY_API_ROOT="$TEST_ROOT/safe/api" BLUEY_API_LOG_DIR='/' \
    "$ROOT/ops/install-bluey-log-guards.sh" --check-config >/dev/null 2>&1; then
    fail "installer accepted the filesystem root as LOG_DIR"
fi

mkdir -p "$TEST_ROOT/lock-target" "$TEST_ROOT/lock-backups" \
    "$TEST_ROOT/lock-archive-work" "$TEST_ROOT/lock-archives" \
    "$TEST_ROOT/lock-status" "$TEST_ROOT/installer/api"
printf 'do-not-touch' > "$TEST_ROOT/lock-target/file"
ln -s "$TEST_ROOT/lock-target/file" "$TEST_ROOT/lock-backups/.backup.lock"
if PATH="$MOCK_BIN:$PATH" BLUEY_BACKUP_DIR="$TEST_ROOT/lock-backups" \
    BLUEY_SERVER_DB_BACKEND=sqlite BLUEY_DB_PATH="$DB_PATH" \
    BLUEY_BACKUP_LOCAL_MAX_BYTES=0 BLUEY_BACKUP_MIN_FREE_GB=0 \
    "$ROOT/ops/backup-bluey-db.sh" >/dev/null 2>&1; then
    fail "backup followed a malicious lock symlink"
fi
[ "$(cat "$TEST_ROOT/lock-target/file")" = 'do-not-touch' ] ||
    fail "backup lock rejection mutated the symlink target"
ln -s "$TEST_ROOT/lock-target/file" "$TEST_ROOT/lock-archive-work/.archive.lock"
if PATH="$MOCK_BIN:$PATH" \
    BLUEY_LOG_ARCHIVE_LOCAL_DIR="$TEST_ROOT/lock-archives" \
    BLUEY_LOG_ARCHIVE_WORK_DIR="$TEST_ROOT/lock-archive-work" \
    BLUEY_LOG_DIRS="$TEST_ROOT/safe/logs" \
    "$ROOT/ops/archive-bluey-logs.sh" --prune-only >/dev/null 2>&1; then
    fail "log archive followed a malicious lock symlink"
fi
ln -s "$TEST_ROOT/lock-target/file" "$TEST_ROOT/lock-status/disk-guard.lock"
if PATH="$MOCK_BIN:$PATH" BLUEY_BACKUP_DIR="$TEST_ROOT/safe/backups" \
    BLUEY_DISK_GUARD_TMP_ROOT="$TEST_ROOT/safe/tmp" \
    BLUEY_DISK_GUARD_STATUS_FILE="$TEST_ROOT/lock-status/guard.status" \
    BLUEY_DISK_GUARD_LOCK_FILE="$TEST_ROOT/lock-status/disk-guard.lock" \
    "$ROOT/ops/bluey-disk-guard.sh" check >/dev/null 2>&1; then
    fail "disk guard followed a malicious lock symlink"
fi
ln -s "$TEST_ROOT/lock-target" "$TEST_ROOT/installer/api/logs"
if BLUEY_API_ROOT="$TEST_ROOT/installer/api" \
    BLUEY_API_LOG_DIR="$TEST_ROOT/installer/logs" \
    "$ROOT/ops/install-bluey-log-guards.sh" --check-config >/dev/null 2>&1; then
    fail "installer accepted a symlinked service log directory"
fi
installer_lock_target="$TEST_ROOT/installer-lock-target"
installer_lock_link="$TEST_ROOT/installer-lock-link"
: > "$installer_lock_target"
ln -s "$installer_lock_target" "$installer_lock_link"
if BLUEY_LOG_GUARD_INSTALL_LOCK_FILE="$installer_lock_link" \
    "$ROOT/ops/install-bluey-log-guards.sh" --check-config >/dev/null 2>&1; then
    fail "installer accepted a symlinked process-fence lock"
fi
installer_owned_lock="$TEST_ROOT/installer-owned-lock"
: > "$installer_owned_lock"
chmod 0600 "$installer_owned_lock"
if BLUEY_LOG_GUARD_INSTALL_LOCK_FILE="$installer_owned_lock" \
    "$ROOT/ops/install-bluey-log-guards.sh" --check-config >/dev/null 2>&1; then
    fail "installer accepted a non-root-owned process-fence lock chain"
fi

# Deterministic post-open substitution tests use a non-root-only installer hook.
# The stat shim changes only ownership/mode reporting and normalizes macOS
# /dev/fd's pseudo-device ID; inode identity remains real.
LOCK_TEST_BIN="$TEST_ROOT/lock-test-bin"
LOCK_TEST_ROOT="$(cd -P "$TEST_ROOT" && pwd -P)/lock-open"
mkdir -p "$LOCK_TEST_BIN" "$LOCK_TEST_ROOT"
cat > "$LOCK_TEST_BIN/stat" <<'SH'
#!/usr/bin/env bash
args="$*"; path="${!#}"
case "$args" in
  *%u*) echo 0 ;;
  *%a*|*%Lp*)
    if [ -f "$path" ]; then /usr/bin/stat -f '%Lp' "$path"; else echo 755; fi ;;
  *%d:%i*) echo "1:$(/usr/bin/stat -f '%i' "$path")" ;;
  *) /usr/bin/stat "$@" ;;
esac
SH
cat > "$LOCK_TEST_BIN/flock" <<'SH'
#!/usr/bin/env bash
exit 0
SH
chmod 0755 "$LOCK_TEST_BIN/stat" "$LOCK_TEST_BIN/flock"

run_lock_swap_test() {
    local name="$1" hook="$2" lock
    lock="$LOCK_TEST_ROOT/$name/guard.lock"
    mkdir -p "$(dirname "$lock")"; : > "$lock"; chmod 0600 "$lock"
    if PATH="$LOCK_TEST_BIN:$PATH" BLUEY_INSTALLER_ENABLE_TEST_HOOK=1 \
        BLUEY_LOG_GUARD_INSTALL_LOCK_FILE="$lock" \
        BLUEY_INSTALLER_LOCK_POST_OPEN_TEST_HOOK_COMMAND="$hook" \
        "$ROOT/ops/install-bluey-log-guards.sh" --test-lock-open >/dev/null 2>&1; then
        fail "installer accepted post-open $name substitution"
    fi
}
leaf_lock="$LOCK_TEST_ROOT/leaf/guard.lock"
run_lock_swap_test leaf "mv '$leaf_lock' '${leaf_lock}.old'; : > '$leaf_lock'; chmod 0600 '$leaf_lock'"
mode_lock="$LOCK_TEST_ROOT/mode/guard.lock"
run_lock_swap_test mode "chmod 0666 '$mode_lock'"
ancestor_lock="$LOCK_TEST_ROOT/ancestor/guard.lock"
run_lock_swap_test ancestor "mv '$(dirname "$ancestor_lock")' '$(dirname "$ancestor_lock").old'; mkdir -p '$(dirname "$ancestor_lock")'; : > '$ancestor_lock'; chmod 0600 '$ancestor_lock'"
stable_lock="$LOCK_TEST_ROOT/stable/guard.lock"
mkdir -p "$(dirname "$stable_lock")"; : > "$stable_lock"; chmod 0600 "$stable_lock"
PATH="$LOCK_TEST_BIN:$PATH" BLUEY_INSTALLER_ENABLE_TEST_HOOK=1 \
BLUEY_LOG_GUARD_INSTALL_LOCK_FILE="$stable_lock" \
    "$ROOT/ops/install-bluey-log-guards.sh" --test-lock-open >/dev/null 2>&1 ||
    fail "installer rejected a stable attested lock across open"

ARCHIVE_STAGING_TARGET="$TEST_ROOT/archive-staging-target"
ARCHIVE_STAGING_ROOT="$TEST_ROOT/archive-staging-root"
mkdir -p "$ARCHIVE_STAGING_TARGET" "$ARCHIVE_STAGING_ROOT" \
    "$TEST_ROOT/archive-staging-work" "$TEST_ROOT/archive-staging-logs"
printf 'do-not-delete' > "$ARCHIVE_STAGING_TARGET/bluey-logs-unsafe.tar.gz.tmp.old"
ln -s "$ARCHIVE_STAGING_TARGET" "$ARCHIVE_STAGING_ROOT/.staging"
if PATH="$MOCK_BIN:$PATH" \
    BLUEY_LOG_ARCHIVE_LOCAL_DIR="$ARCHIVE_STAGING_ROOT" \
    BLUEY_LOG_ARCHIVE_WORK_DIR="$TEST_ROOT/archive-staging-work" \
    BLUEY_LOG_DIRS="$TEST_ROOT/archive-staging-logs" \
        "$ROOT/ops/archive-bluey-logs.sh" --prune-only >/dev/null 2>&1; then
    fail "log archive accepted a symlinked staging root"
fi
[ -f "$ARCHIVE_STAGING_TARGET/bluey-logs-unsafe.tar.gz.tmp.old" ] ||
    fail "log archive staging rejection mutated the symlink target"

INSTALLER_POISON_ENV="$TEST_ROOT/installer-poison-storage.env"
printf '%s\n' 'BLUEY_API_ROOT=/' > "$INSTALLER_POISON_ENV"
if BLUEY_STORAGE_ENV_FILE="$INSTALLER_POISON_ENV" \
    "$ROOT/ops/install-bluey-log-guards.sh" --check-config >/dev/null 2>&1; then
    fail "installer did not load and reject an unsafe shared storage policy"
fi

UNTRUSTED_ENV="$TEST_ROOT/untrusted-storage.env"
UNTRUSTED_EFFECT="$TEST_ROOT/untrusted-env-was-sourced"
printf 'touch %q\n' "$UNTRUSTED_EFFECT" > "$UNTRUSTED_ENV"
chmod 0666 "$UNTRUSTED_ENV"
for root_script in backup-bluey-db.sh archive-bluey-logs.sh \
    bluey-disk-guard.sh install-bluey-log-guards.sh; do
    if BLUEY_STORAGE_ENV_FILE="$UNTRUSTED_ENV" \
        "$ROOT/ops/$root_script" --check-config >/dev/null 2>&1; then
        fail "$root_script accepted a group/world-writable root env fragment"
    fi
done
[ ! -e "$UNTRUSTED_EFFECT" ] ||
    fail "a root operations script sourced the untrusted env fragment"

TRUSTED_ENV_TARGET="$TEST_ROOT/trusted-env-target"
printf '%s\n' 'BLUEY_BACKUP_HOURLY_KEEP=4' > "$TRUSTED_ENV_TARGET"
chmod 0600 "$TRUSTED_ENV_TARGET"
SYMLINK_ENV="$TEST_ROOT/symlink-storage.env"
ln -s "$TRUSTED_ENV_TARGET" "$SYMLINK_ENV"
for root_script in backup-bluey-db.sh archive-bluey-logs.sh \
    bluey-disk-guard.sh install-bluey-log-guards.sh; do
    if BLUEY_STORAGE_ENV_FILE="$SYMLINK_ENV" \
        "$ROOT/ops/$root_script" --check-config >/dev/null 2>&1; then
        fail "$root_script followed a symlinked root env fragment"
    fi
done

# Exercise the production trust policy on a non-sticky path rooted in the repo;
# the normal fixtures intentionally disable ownership checks for portability.
mkdir -p "$TRUST_TEST_ROOT/backups" "$TRUST_TEST_ROOT/state" \
    "$TRUST_TEST_ROOT/work" "$TRUST_TEST_ROOT/logs"
chmod 0700 "$TRUST_TEST_ROOT" "$TRUST_TEST_ROOT"/*
: > "$MOCK_DF_COUNT_FILE"
PATH="$MOCK_BIN:$PATH" BLUEY_BACKUP_REQUIRE_TRUSTED_PATHS=1 \
BLUEY_BACKUP_DIR="$TRUST_TEST_ROOT/backups" BLUEY_SERVER_DB_BACKEND=sqlite \
BLUEY_DB_PATH="$DB_PATH" BLUEY_BACKUP_LOCAL_MAX_BYTES=0 BLUEY_BACKUP_MIN_FREE_GB=0 \
BLUEY_BACKUP_REQUIRE_OFFSITE=0 MOCK_DATE_TS=20260830T125957Z \
    "$ROOT/ops/backup-bluey-db.sh" >/dev/null
PATH="$MOCK_BIN:$PATH" BLUEY_LOG_ARCHIVE_REQUIRE_TRUSTED_PATHS=1 \
BLUEY_OPS_STATE_ROOT="$TRUST_TEST_ROOT/state" \
BLUEY_LOG_ARCHIVE_LOCAL_DIR="$TRUST_TEST_ROOT/backups/logs" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$TRUST_TEST_ROOT/work" \
BLUEY_LOG_DIRS="$TRUST_TEST_ROOT/logs" BLUEY_LOG_ARCHIVE_SERVICES='' \
    "$ROOT/ops/archive-bluey-logs.sh" >/dev/null
grep -Fq '/var/lib/bluey-ops/log-archive' "$ROOT/ops/archive-bluey-logs.sh" ||
    fail "production archive work default still uses a sticky temp root"

INVALID_SNAPSHOT_ROOT="$TEST_ROOT/invalid-snapshot"
if MOCK_SQLITE_INVALID=1 PATH="$MOCK_BIN:$PATH" \
    BLUEY_SERVER_DB_BACKEND=sqlite \
    BLUEY_DB_PATH="$DB_PATH" \
    BLUEY_BACKUP_DIR="$INVALID_SNAPSHOT_ROOT" \
    BLUEY_BACKUP_LOCAL_MAX_BYTES=0 \
    BLUEY_BACKUP_MIN_FREE_GB=0 \
    MOCK_DATE_TS=20260830T125959Z \
        "$ROOT/ops/backup-bluey-db.sh" >/dev/null 2>&1; then
    fail "structurally invalid SQLite snapshot was finalized"
fi
[ -z "$(find "$INVALID_SNAPSHOT_ROOT/hourly" -type f -name '*.db' -print -quit)" ] ||
    fail "invalid SQLite snapshot escaped staging"
[ -z "$(find "$INVALID_SNAPSHOT_ROOT/.staging" -mindepth 1 -print -quit)" ] ||
    fail "invalid SQLite snapshot left staging data"

OVERSIZED_ROOT="$TEST_ROOT/oversized-snapshot"
OVERSIZED_DB="$TEST_ROOT/oversized.db"
dd if=/dev/zero of="$OVERSIZED_DB" bs=4096 count=1 2>/dev/null
if PATH="$MOCK_BIN:$PATH" \
    BLUEY_SERVER_DB_BACKEND=sqlite \
    BLUEY_DB_PATH="$OVERSIZED_DB" \
    BLUEY_BACKUP_DIR="$OVERSIZED_ROOT" \
    BLUEY_BACKUP_LOCAL_MAX_BYTES=0 \
    BLUEY_BACKUP_MAX_SNAPSHOT_BYTES=1024 \
    BLUEY_BACKUP_MIN_FREE_GB=0 \
    MOCK_DATE_TS=20260830T125958Z \
        "$ROOT/ops/backup-bluey-db.sh" >/dev/null 2>&1; then
    fail "oversized database writer escaped the hard snapshot bound"
fi
[ -z "$(find "$OVERSIZED_ROOT/hourly" -type f -name '*.db' -print -quit)" ] ||
    fail "oversized database writer finalized a snapshot"

MIDNIGHT_ROOT="$TEST_ROOT/midnight-reserve"
if PATH="$MOCK_BIN:$PATH" \
    BLUEY_SERVER_DB_BACKEND=sqlite \
    BLUEY_DB_PATH="$DB_PATH" \
    BLUEY_BACKUP_DIR="$MIDNIGHT_ROOT" \
    BLUEY_BACKUP_LOCAL_MAX_BYTES=3072 \
    BLUEY_BACKUP_MAX_SNAPSHOT_BYTES=2048 \
    BLUEY_BACKUP_MIN_FREE_GB=0 \
    MOCK_DATE_TS=20260831T000000Z \
        "$ROOT/ops/backup-bluey-db.sh" >/dev/null 2>&1; then
    fail "midnight backup ignored the simultaneous hourly/daily writer reserve"
fi
[ -z "$(find "$MIDNIGHT_ROOT/hourly" -type f -name '*.db' -print -quit)" ] ||
    fail "midnight reserve failure still finalized an hourly snapshot"

SCOPE_ROOT="$TEST_ROOT/scope"
mkdir -p "$SCOPE_ROOT/round-rollbacks"
dd if=/dev/zero of="$SCOPE_ROOT/round-rollbacks/retained-evidence.bin" \
    bs=1024 count=1024 2>/dev/null
: > "$MOCK_DF_COUNT_FILE"
PATH="$MOCK_BIN:$PATH" \
BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
BLUEY_SERVER_DB_BACKEND=sqlite \
BLUEY_DB_PATH="$DB_PATH" \
BLUEY_BACKUP_DIR="$SCOPE_ROOT" \
BLUEY_BACKUP_LOCAL_MAX_BYTES=131072 \
BLUEY_BACKUP_MAX_SNAPSHOT_BYTES=65536 \
BLUEY_BACKUP_MIN_FREE_GB=0 \
MOCK_DATE_TS=20260830T130001Z \
MOCK_DF_MODE=healthy \
    "$ROOT/ops/backup-bluey-db.sh" >/dev/null
[ -f "$SCOPE_ROOT/hourly/bluey-20260830T130001Z.db" ] ||
    fail "whole-tree rollback evidence was incorrectly charged to the DB hot cap"
[ -f "$SCOPE_ROOT/round-rollbacks/retained-evidence.bin" ] ||
    fail "backup capacity handling mutated unrelated rollback evidence"

BOOTSTRAP_ROOT="$TEST_ROOT/bootstrap"
legacy_hourly="$BOOTSTRAP_ROOT/hourly/bluey-postgres-20260829T230000Z.pgdump"
legacy_daily="$BOOTSTRAP_ROOT/daily/bluey-postgres-20260829.pgdump"
mkdir -p "$(dirname "$legacy_hourly")" "$(dirname "$legacy_daily")"
printf 'legacy-exact-backup-bytes' > "$legacy_hourly"
cp "$legacy_hourly" "$legacy_daily"
legacy_sha="$(sha256_file "$legacy_hourly")"
printf '%s  /var/backups/bluey-api/hourly/%s\n' \
    "$legacy_sha" "$(basename "$legacy_hourly")" > "${legacy_hourly}.sha256"
cp "${legacy_hourly}.sha256" "${legacy_daily}.sha256"
legacy_remote="$MOCK_S3_ROOT/bluey-test/backups/api/$(basename "$legacy_hourly")"
mkdir -p "$(dirname "$legacy_remote")"
cp "$legacy_hourly" "$legacy_remote"
cp "${legacy_hourly}.sha256" "${legacy_remote}.sha256"
if MOCK_PG_RESTORE_INVALID=1 PATH="$MOCK_BIN:$PATH" \
    BLUEY_BACKUP_DIR="$BOOTSTRAP_ROOT" \
    BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
    OFFSITE_DESTINATION=s3://bluey-test/backups/api/ \
        "$ROOT/ops/backup-bluey-db.sh" --verify-existing >/dev/null 2>&1; then
    fail "legacy marker bootstrap accepted an invalid PostgreSQL archive"
fi
[ ! -e "${legacy_hourly}.offsite-verified" ] ||
    fail "invalid legacy archive received an offsite marker"
PATH="$MOCK_BIN:$PATH" \
BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
BLUEY_BACKUP_DIR="$BOOTSTRAP_ROOT" \
BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
OFFSITE_DESTINATION=s3://bluey-test/backups/api/ \
    "$ROOT/ops/backup-bluey-db.sh" --verify-existing >/dev/null
[ -f "${legacy_hourly}.offsite-verified" ] ||
    fail "legacy hourly backup did not receive a verified marker"
[ -f "${legacy_daily}.offsite-verified" ] ||
    fail "legacy daily backup did not receive a verified marker"
grep -Fqx "destination=s3://bluey-test/backups/api/$(basename "$legacy_hourly")" \
    "${legacy_daily}.offsite-verified" ||
    fail "legacy daily marker did not bind its original hourly R2 object"

CAPACITY_ROOT="$TEST_ROOT/capacity"
mkdir -p "$CAPACITY_ROOT/hourly" "$CAPACITY_ROOT/daily"
make_capacity_pair() {
    local name="$1" verified="$2" path sha bytes remote
    path="$CAPACITY_ROOT/hourly/$name.db"
    printf '%0100d' "${name##*-}" > "$path"
    sha="$(sha256_file "$path")"
    bytes="$(file_size "$path")"
    printf '%s  %s\n' "$sha" "$(basename "$path")" > "${path}.sha256"
    if [ "$verified" = "1" ]; then
        printf 'schema=1\nbytes=%s\nsha256=%s\ndestination=s3://bluey-test/capacity/hourly/%s\nlayout=class-prefix\n' \
            "$bytes" "$sha" "$(basename "$path")" > "${path}.offsite-verified"
        remote="$MOCK_S3_ROOT/bluey-test/capacity/hourly/$(basename "$path")"
        mkdir -p "$(dirname "$remote")"
        cp "$path" "$remote"
        cp "${path}.sha256" "${remote}.sha256"
    fi
}
make_capacity_pair bluey-1 1
make_capacity_pair bluey-2 1
make_capacity_pair bluey-3 1
touch -t 202001010001 "$CAPACITY_ROOT/hourly/bluey-1.db"
touch -t 202001010002 "$CAPACITY_ROOT/hourly/bluey-2.db"
touch -t 202001010003 "$CAPACITY_ROOT/hourly/bluey-3.db"
: > "$MOCK_DF_COUNT_FILE"
if PATH="$MOCK_BIN:$PATH" \
    BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
    BLUEY_SERVER_DB_BACKEND=sqlite \
    BLUEY_DB_PATH="$DB_PATH" \
    BLUEY_BACKUP_DIR="$CAPACITY_ROOT" \
    BLUEY_BACKUP_HOURLY_KEEP=3 \
    BLUEY_BACKUP_DAILY_KEEP=1 \
    BLUEY_BACKUP_MIN_HOURLY_KEEP=1 \
    BLUEY_BACKUP_MIN_DAILY_KEEP=1 \
    BLUEY_BACKUP_LOCAL_MAX_BYTES=0 \
    BLUEY_BACKUP_MAX_SNAPSHOT_BYTES=50 \
    BLUEY_BACKUP_MIN_FREE_GB=6 \
    BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
    OFFSITE_DESTINATION=s3://bluey-test/capacity/ \
    MOCK_DF_MODE=high \
        "$ROOT/ops/backup-bluey-db.sh" >/dev/null 2>&1; then
    fail "unrecoverable capacity pressure did not fail closed"
fi
[ ! -e "$CAPACITY_ROOT/hourly/bluey-1.db" ] ||
    fail "capacity pruning did not remove the oldest re-proven pair"
[ ! -e "$CAPACITY_ROOT/hourly/bluey-2.db" ] ||
    fail "capacity pruning did not remove a second re-proven pair"
[ -f "$CAPACITY_ROOT/hourly/bluey-3.db" ] ||
    fail "capacity pruning violated the configured local minimum"

ARCHIVE_ROOT="$TEST_ROOT/log-archives"
WORK_ROOT="$TEST_ROOT/log-work"
mkdir -p "$ARCHIVE_ROOT" "$WORK_ROOT/bluey-logs-old-one" \
    "$WORK_ROOT/bluey-logs-old-two" "$WORK_ROOT/unrelated" "$ARCHIVE_ROOT/.staging"
printf 'orphaned-tar' > "$ARCHIVE_ROOT/.staging/bluey-logs-old.tar.gz.tmp.dead"
touch -t 202001010001 "$WORK_ROOT/bluey-logs-old-one" "$WORK_ROOT/bluey-logs-old-two"
PATH="$MOCK_BIN:$PATH" \
BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
BLUEY_LOG_ARCHIVE_LOCAL_DIR="$ARCHIVE_ROOT" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$WORK_ROOT" \
BLUEY_LOG_DIRS="$TEST_ROOT/no-logs" \
BLUEY_LOG_WORK_RETENTION_MINUTES=60 \
BLUEY_LOG_WORK_MAX_DIRS=1 \
BLUEY_LOG_WORK_ROOT_MAX_BYTES=1024 \
    "$ROOT/ops/archive-bluey-logs.sh" --prune-only >/dev/null
[ ! -e "$WORK_ROOT/bluey-logs-old-one" ] || fail "old log workdir survived pruning"
[ ! -e "$WORK_ROOT/bluey-logs-old-two" ] || fail "second old log workdir survived pruning"
[ -d "$WORK_ROOT/unrelated" ] || fail "work pruning crossed the Bluey workdir prefix"
[ ! -e "$ARCHIVE_ROOT/.staging/bluey-logs-old.tar.gz.tmp.dead" ] ||
    fail "orphaned archive staging data survived the lock-held sweep"

PATH="$MOCK_BIN:$PATH" \
BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
BLUEY_LOG_ARCHIVE_LOCAL_DIR="$ARCHIVE_ROOT" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$WORK_ROOT" \
BLUEY_LOG_DIRS="$TEST_ROOT/no-logs" \
BLUEY_LOG_ARCHIVE_SERVICES="" \
BLUEY_LOG_WORK_RETENTION_MINUTES=1440 \
    "$ROOT/ops/archive-bluey-logs.sh" >/dev/null 2>&1
successful_archive="$(find "$ARCHIVE_ROOT" -maxdepth 1 -type f \
    -name 'bluey-logs-*.tar.gz' -print -quit)"
[ -n "$successful_archive" ] || fail "atomic log archive was not finalized"
[ -f "${successful_archive}.sha256" ] || fail "log archive checksum was not finalized"
[ -z "$(find "$ARCHIVE_ROOT/.staging" -mindepth 1 -type f -print -quit)" ] ||
    fail "successful log archive left staging bytes"
grep -Fqx 'status=ok' "$BLUEY_OPS_STATE_ROOT/log-archive.status" ||
    fail "successful log archive did not persist durable status"

FAIL_BIN="$TEST_ROOT/fail-bin"
mkdir -p "$FAIL_BIN"
cat > "$FAIL_BIN/tar" <<'SH'
#!/usr/bin/env bash
exit 19
SH
chmod +x "$FAIL_BIN/tar"
if PATH="$FAIL_BIN:$MOCK_BIN:$PATH" \
    BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
    BLUEY_LOG_ARCHIVE_LOCAL_DIR="$ARCHIVE_ROOT" \
    BLUEY_LOG_ARCHIVE_WORK_DIR="$WORK_ROOT" \
    BLUEY_LOG_DIRS="$TEST_ROOT/no-logs" \
    BLUEY_LOG_ARCHIVE_SERVICES="" \
    BLUEY_LOG_WORK_RETENTION_MINUTES=1440 \
        "$ROOT/ops/archive-bluey-logs.sh" >/dev/null 2>&1; then
    fail "archive unexpectedly succeeded with a failed tar"
fi
[ -z "$(find "$WORK_ROOT" -mindepth 1 -maxdepth 1 -type d \
    -name 'bluey-logs-*' -print -quit)" ] || fail "EXIT trap left an archive workdir"
[ -z "$(find "$ARCHIVE_ROOT/.staging" -mindepth 1 -type f -print -quit 2>/dev/null)" ] ||
    fail "EXIT trap left a partial archive"
grep -Fqx 'status=fail' "$BLUEY_OPS_STATE_ROOT/log-archive.status" ||
    fail "failed log archive did not persist durable status"
grep -Eq '(^| )-n 8$' "$MOCK_FLOCK_LOG" || fail "log archive did not take a flock"

mkdir -p "$WORK_ROOT/bluey-logs-lock-protected"
if MOCK_FLOCK_FAIL=1 PATH="$MOCK_BIN:$PATH" \
    BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
    BLUEY_LOG_ARCHIVE_LOCAL_DIR="$ARCHIVE_ROOT" \
    BLUEY_LOG_ARCHIVE_WORK_DIR="$WORK_ROOT" \
        "$ROOT/ops/archive-bluey-logs.sh" --prune-only >/dev/null 2>&1; then
    fail "overlapping log archive process acquired the lock"
fi
[ -d "$WORK_ROOT/bluey-logs-lock-protected" ] ||
    fail "lock failure mutated another archive process's workdir"
rm -rf "$WORK_ROOT/bluey-logs-lock-protected"

PROOF_ARCHIVE_ROOT="$TEST_ROOT/proof-log-archives"
PROOF_WORK_ROOT="$TEST_ROOT/proof-log-work"
PROOF_LOG_ROOT="$TEST_ROOT/proof-hot-logs"
mkdir -p "$PROOF_ARCHIVE_ROOT" "$PROOF_WORK_ROOT" "$PROOF_LOG_ROOT"
rotated_log="$PROOF_LOG_ROOT/bluey.log.1"
printf 'retained-until-offhost-proof' > "$rotated_log"
touch -t 202001010000 "$rotated_log"
if MOCK_AWS_CORRUPT_READBACK=1 PATH="$MOCK_BIN:$PATH" \
    BLUEY_LOG_ARCHIVE_LOCAL_DIR="$PROOF_ARCHIVE_ROOT" \
    BLUEY_LOG_ARCHIVE_WORK_DIR="$PROOF_WORK_ROOT" \
    BLUEY_LOG_DIRS="$PROOF_LOG_ROOT" \
    BLUEY_LOG_ARCHIVE_SERVICES='' \
    BLUEY_LOG_LOCAL_RETENTION_DAYS=0 \
    BLUEY_LOG_ARCHIVE_DESTINATION=s3://bluey-test/logs \
    BLUEY_OPS_LOG_R2_ACCESS_KEY_ID=test-log-key \
    BLUEY_OPS_LOG_R2_SECRET_ACCESS_KEY=test-log-secret \
    BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST=1 \
        "$ROOT/ops/archive-bluey-logs.sh" >/dev/null 2>&1; then
    fail "corrupt offhost log read-back was accepted"
fi
[ -f "$rotated_log" ] ||
    fail "hot log was pruned before a new archive had exact offhost proof"
unverified_archive="$(find "$PROOF_ARCHIVE_ROOT" -maxdepth 1 -type f \
    -name 'bluey-logs-*.tar.gz' -print -quit)"
[ -n "$unverified_archive" ] && [ ! -e "${unverified_archive}.offsite-verified" ] ||
    fail "failed log upload did not leave the local archive unverified"
PATH="$MOCK_BIN:$PATH" \
BLUEY_LOG_ARCHIVE_LOCAL_DIR="$PROOF_ARCHIVE_ROOT" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$PROOF_WORK_ROOT" \
BLUEY_LOG_DIRS="$PROOF_LOG_ROOT" \
BLUEY_LOG_ARCHIVE_SERVICES='' \
BLUEY_LOG_LOCAL_RETENTION_DAYS=0 \
BLUEY_LOG_ARCHIVE_DESTINATION=s3://bluey-test/logs \
BLUEY_OPS_LOG_R2_ACCESS_KEY_ID=test-log-key \
BLUEY_OPS_LOG_R2_SECRET_ACCESS_KEY=test-log-secret \
BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST=1 \
    "$ROOT/ops/archive-bluey-logs.sh" >/dev/null
[ -f "$rotated_log" ] ||
    fail "v1 archive unexpectedly auto-deleted a rotated log without exact capture mapping"
while IFS= read -r proven_archive; do
    [ -f "${proven_archive}.offsite-verified" ] ||
        fail "log archive recovery left an unverified local archive"
done < <(find "$PROOF_ARCHIVE_ROOT" -maxdepth 1 -type f -name 'bluey-logs-*.tar.gz')
PATH="$MOCK_BIN:$PATH" \
BLUEY_LOG_ARCHIVE_LOCAL_DIR="$PROOF_ARCHIVE_ROOT" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$PROOF_WORK_ROOT" \
BLUEY_LOG_DIRS="$PROOF_LOG_ROOT" \
BLUEY_LOG_ARCHIVE_DESTINATION=s3://bluey-test/new-log-prefix \
BLUEY_OPS_LOG_R2_ACCESS_KEY_ID=test-log-key \
BLUEY_OPS_LOG_R2_SECRET_ACCESS_KEY=test-log-secret \
BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST=1 \
    "$ROOT/ops/archive-bluey-logs.sh" --prune-only >/dev/null
if find "$PROOF_ARCHIVE_ROOT" -maxdepth 1 -type f -name '*.offsite-verified' \
    -exec grep -L '^destination=s3://bluey-test/new-log-prefix/' {} + | grep -q .; then
    fail "log archive marker from an obsolete destination was reused"
fi

GUARD_ROOT="$TEST_ROOT/guard"
mkdir -p "$GUARD_ROOT/root" "$GUARD_ROOT/tmp" "$GUARD_ROOT/api" \
    "$GUARD_ROOT/web/releases" "$GUARD_ROOT/backups/hourly" "$GUARD_ROOT/logs" \
    "$GUARD_ROOT/build/bluey-build-stale"
guard_backup="$GUARD_ROOT/backups/hourly/bluey-postgres-20260830T110000Z.pgdump"
printf 'metadata-only-backup-health-fixture' > "$guard_backup"
guard_sha="$(printf 'a%.0s' {1..64})"
guard_bytes="$(file_size "$guard_backup")"
printf '%s  %s\n' "$guard_sha" "$(basename "$guard_backup")" > "${guard_backup}.sha256"
printf 'schema=1\nbytes=%s\nsha256=%s\ndestination=s3://bluey-test/backups/hourly/%s\nlayout=class-prefix\n' \
    "$guard_bytes" "$guard_sha" "$(basename "$guard_backup")" \
    > "${guard_backup}.offsite-verified"
if stat -c%Y "$guard_backup" >/dev/null 2>&1; then
    guard_backup_mtime="$(stat -c%Y "$guard_backup")"
else
    guard_backup_mtime="$(stat -f%m "$guard_backup")"
fi
guard_ok_epoch=$((guard_backup_mtime + 60 * 60))
GUARD_ARCHIVE_PRUNE="$TEST_ROOT/mock-archive-prune"
cat > "$GUARD_ARCHIVE_PRUNE" <<'SH'
#!/usr/bin/env bash
[ "${1:-}" = "--prune-only" ]
SH
chmod +x "$GUARD_ARCHIVE_PRUNE"
STATUS_FILE="$GUARD_ROOT/status/disk.status"
: > "$MOCK_DF_COUNT_FILE"
PATH="$MOCK_BIN:$PATH" \
BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
BLUEY_DISK_GUARD_PATH="$GUARD_ROOT/root" \
BLUEY_API_ROOT="$GUARD_ROOT/api" \
BLUEY_WEB_ROOT="$GUARD_ROOT/web" \
BLUEY_BACKUP_DIR="$GUARD_ROOT/backups" \
BLUEY_API_LOG_DIR="$GUARD_ROOT/logs" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$GUARD_ROOT/log-work" \
BLUEY_DISK_BUILD_SCAN_ROOT="$GUARD_ROOT/build" \
BLUEY_DISK_GUARD_TMP_ROOT="$GUARD_ROOT/tmp" \
BLUEY_LOG_ARCHIVE_SCRIPT="$GUARD_ARCHIVE_PRUNE" \
BLUEY_DISK_GUARD_STATUS_FILE="$STATUS_FILE" \
BLUEY_DISK_GUARD_REQUIRE_STATUS=1 \
BLUEY_SERVER_DB_BACKEND=postgres \
BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
OFFSITE_DESTINATION=s3://bluey-test/backups/ \
BLUEY_DISK_BACKUP_HEALTH_REQUIRED=1 \
BLUEY_DISK_BACKUP_REQUIRE_ROOT_OWNERSHIP=0 \
BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL=https://alerts.example.test/bluey \
BLUEY_DISK_GUARD_REQUIRE_ALERT=1 \
MOCK_DATE_EPOCH="$guard_ok_epoch" \
MOCK_DF_MODE=transition \
    "$ROOT/ops/bluey-disk-guard.sh" --prune > "$GUARD_ROOT/prune.out"
grep -Fqx 'status=ok' "$STATUS_FILE" || fail "post-prune recovery did not persist ok"
grep -Fq 'component=backups bytes=' "$GUARD_ROOT/prune.out" ||
    fail "disk guard omitted backup component bytes"
[ ! -s "$MOCK_LOGROTATE_LOG" ] ||
    fail "disk guard forced logrotate without a fresh archive-proof gate"

: > "$MOCK_DF_COUNT_FILE"
PATH="$MOCK_BIN:$PATH" \
BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
BLUEY_DISK_GUARD_PATH="$GUARD_ROOT/root" \
BLUEY_API_ROOT="$GUARD_ROOT/api" \
BLUEY_WEB_ROOT="$GUARD_ROOT/web" \
BLUEY_BACKUP_DIR="$GUARD_ROOT/backups" \
BLUEY_API_LOG_DIR="$GUARD_ROOT/logs" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$GUARD_ROOT/log-work" \
BLUEY_DISK_BUILD_SCAN_ROOT="$GUARD_ROOT/build" \
BLUEY_DISK_GUARD_STATUS_FILE="$STATUS_FILE" \
BLUEY_SERVER_DB_BACKEND=postgres \
BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
OFFSITE_DESTINATION=s3://bluey-test/backups/ \
BLUEY_DISK_BACKUP_HEALTH_REQUIRED=1 \
BLUEY_DISK_BACKUP_REQUIRE_ROOT_OWNERSHIP=0 \
BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL=https://alerts.example.test/bluey \
BLUEY_DISK_GUARD_REQUIRE_ALERT=1 \
MOCK_DATE_EPOCH="$guard_ok_epoch" \
MOCK_DF_MODE=warning \
    "$ROOT/ops/bluey-disk-guard.sh" check >/dev/null
grep -Fqx 'status=warn' "$STATUS_FILE" || fail "early warning status was not durable"
grep -Fq '"state":"warn"' "$MOCK_CURL_LOG" || fail "early warning alert was not sent"

: > "$MOCK_DF_COUNT_FILE"
if PATH="$MOCK_BIN:$PATH" \
    BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
    BLUEY_DISK_GUARD_PATH="$GUARD_ROOT/root" \
    BLUEY_API_ROOT="$GUARD_ROOT/api" \
    BLUEY_WEB_ROOT="$GUARD_ROOT/web" \
    BLUEY_BACKUP_DIR="$GUARD_ROOT/backups" \
    BLUEY_API_LOG_DIR="$GUARD_ROOT/logs" \
    BLUEY_LOG_ARCHIVE_WORK_DIR="$GUARD_ROOT/log-work" \
    BLUEY_DISK_BUILD_SCAN_ROOT="$GUARD_ROOT/build" \
    BLUEY_DISK_GUARD_STATUS_FILE="$STATUS_FILE" \
    BLUEY_SERVER_DB_BACKEND=postgres \
    BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
    OFFSITE_DESTINATION=s3://bluey-test/backups/ \
    BLUEY_DISK_BACKUP_HEALTH_REQUIRED=1 \
    BLUEY_DISK_BACKUP_REQUIRE_ROOT_OWNERSHIP=0 \
    BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL=https://alerts.example.test/bluey \
    BLUEY_DISK_GUARD_REQUIRE_ALERT=1 \
    BLUEY_DISK_GUARD_ALERT_REPEAT_MINUTES=0 \
    MOCK_DATE_EPOCH="$guard_ok_epoch" \
    MOCK_DF_MODE=high \
        "$ROOT/ops/bluey-disk-guard.sh" check >/dev/null 2>&1; then
    fail "high disk usage passed the guard"
fi
grep -Fqx 'status=fail' "$STATUS_FILE" || fail "disk failure status was not durable"
grep -Fq '"state":"fail"' "$MOCK_CURL_LOG" || fail "disk failure alert was not sent"

: > "$MOCK_DF_COUNT_FILE"
PATH="$MOCK_BIN:$PATH" \
BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
BLUEY_DISK_GUARD_PATH="$GUARD_ROOT/root" \
BLUEY_API_ROOT="$GUARD_ROOT/api" \
BLUEY_WEB_ROOT="$GUARD_ROOT/web" \
BLUEY_BACKUP_DIR="$GUARD_ROOT/backups" \
BLUEY_API_LOG_DIR="$GUARD_ROOT/logs" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$GUARD_ROOT/log-work" \
BLUEY_DISK_BUILD_SCAN_ROOT="$GUARD_ROOT/build" \
BLUEY_DISK_GUARD_STATUS_FILE="$STATUS_FILE" \
BLUEY_SERVER_DB_BACKEND=postgres \
BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
OFFSITE_DESTINATION=s3://bluey-test/backups/ \
BLUEY_DISK_BACKUP_HEALTH_REQUIRED=1 \
BLUEY_DISK_BACKUP_REQUIRE_ROOT_OWNERSHIP=0 \
BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL=https://alerts.example.test/bluey \
BLUEY_DISK_GUARD_REQUIRE_ALERT=1 \
MOCK_DATE_EPOCH="$guard_ok_epoch" \
MOCK_DF_MODE=healthy \
    "$ROOT/ops/bluey-disk-guard.sh" check >/dev/null
grep -Fqx 'status=ok' "$STATUS_FILE" || fail "disk recovery status was not durable"
grep -Fq '"state":"recovered"' "$MOCK_CURL_LOG" || fail "disk recovery alert was not sent"

run_backup_health_guard() {
    local observed_epoch="$1"
    : > "$MOCK_DF_COUNT_FILE"
    PATH="$MOCK_BIN:$PATH" \
    BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
    BLUEY_DISK_GUARD_PATH="$GUARD_ROOT/root" \
    BLUEY_API_ROOT="$GUARD_ROOT/api" \
    BLUEY_WEB_ROOT="$GUARD_ROOT/web" \
    BLUEY_BACKUP_DIR="$GUARD_ROOT/backups" \
    BLUEY_API_LOG_DIR="$GUARD_ROOT/logs" \
    BLUEY_LOG_ARCHIVE_WORK_DIR="$GUARD_ROOT/log-work" \
    BLUEY_DISK_BUILD_SCAN_ROOT="$GUARD_ROOT/build" \
    BLUEY_DISK_GUARD_STATUS_FILE="$STATUS_FILE" \
    BLUEY_SERVER_DB_BACKEND=postgres \
    BLUEY_BACKUP_REQUIRE_OFFSITE=1 \
    OFFSITE_DESTINATION=s3://bluey-test/backups/ \
    BLUEY_DISK_BACKUP_HEALTH_REQUIRED=1 \
    BLUEY_DISK_BACKUP_REQUIRE_ROOT_OWNERSHIP=0 \
    BLUEY_DISK_LOG_ARCHIVE_HEALTH_REQUIRED=1 \
    BLUEY_LOG_ARCHIVE_STATUS_FILE="$GUARD_LOG_ARCHIVE_STATUS" \
    BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL=https://alerts.example.test/bluey \
    BLUEY_DISK_GUARD_REQUIRE_ALERT=1 \
    BLUEY_DISK_GUARD_ALERT_REPEAT_MINUTES=0 \
    MOCK_DATE_EPOCH="$observed_epoch" \
    MOCK_DF_MODE=healthy \
        "$ROOT/ops/bluey-disk-guard.sh" check
}

GUARD_LOG_ARCHIVE_STATUS="$BLUEY_OPS_STATE_ROOT/guard-log-archive.status"
write_guard_log_archive_status() {
    local state="$1" started="$2" updated="$3" last_success="$4" exit_code="$5"
    mkdir -p "$(dirname "$GUARD_LOG_ARCHIVE_STATUS")"
    {
        printf 'schema=1\n'
        printf 'status=%s\n' "$state"
        printf 'started_at_epoch=%s\n' "$started"
        printf 'updated_at_epoch=%s\n' "$updated"
        printf 'last_success_epoch=%s\n' "$last_success"
        printf 'exit_code=%s\n' "$exit_code"
        printf 'archive=bluey-logs-test.tar.gz\n'
    } > "$GUARD_LOG_ARCHIVE_STATUS"
    chmod 0600 "$GUARD_LOG_ARCHIVE_STATUS"
}
write_guard_log_archive_status ok "$((guard_ok_epoch - 60))" "$guard_ok_epoch" \
    "$((guard_ok_epoch - 60 * 60))" 0

mv "$GUARD_LOG_ARCHIVE_STATUS" "$GUARD_ROOT/log-archive-status.saved"
if run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1; then
    fail "missing durable log archive status passed the guard"
fi
grep -Fq 'log_archive_status_missing' "$STATUS_FILE" ||
    fail "missing log archive status reason was not durable"
mv "$GUARD_ROOT/log-archive-status.saved" "$GUARD_LOG_ARCHIVE_STATUS"

write_guard_log_archive_status fail "$((guard_ok_epoch - 60))" "$guard_ok_epoch" \
    "$((guard_ok_epoch - 60 * 60))" 19
if run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1; then
    fail "failed log archive run passed the guard"
fi
grep -Fqx 'log_archive_run_status=fail' "$STATUS_FILE" ||
    fail "failed log archive run state was not durable"

write_guard_log_archive_status ok "$((guard_ok_epoch - 60))" "$guard_ok_epoch" \
    "$((guard_ok_epoch - 120 * 60))" 0
run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1
grep -Fqx 'status=warn' "$STATUS_FILE" ||
    fail "log archive warning-age boundary was not durable"
grep -Fqx 'log_archive_age_minutes=120' "$STATUS_FILE" ||
    fail "log archive warning boundary is not inclusive"

write_guard_log_archive_status ok "$((guard_ok_epoch - 60))" "$guard_ok_epoch" \
    "$((guard_ok_epoch - 180 * 60))" 0
if run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1; then
    fail "hard-stale log archive passed the guard"
fi
grep -Fqx 'log_archive_age_minutes=180' "$STATUS_FILE" ||
    fail "log archive hard boundary is not inclusive"

write_guard_log_archive_status running "$((guard_ok_epoch - 45 * 60))" \
    "$guard_ok_epoch" "$((guard_ok_epoch - 60 * 60))" 0
run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1
grep -Fq 'log_archive_writer_slow' "$STATUS_FILE" ||
    fail "log archive writer warning reason was omitted"
write_guard_log_archive_status running "$((guard_ok_epoch - 90 * 60))" \
    "$guard_ok_epoch" "$((guard_ok_epoch - 60 * 60))" 0
if run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1; then
    fail "stuck log archive writer passed the guard"
fi
grep -Fq 'log_archive_writer_stuck' "$STATUS_FILE" ||
    fail "log archive writer hard-failure reason was omitted"
write_guard_log_archive_status ok "$((guard_ok_epoch - 60))" "$guard_ok_epoch" \
    "$((guard_ok_epoch - 60 * 60))" 0

guard_lock="$GUARD_ROOT/backups/.backup.lock"
partial_backup="$GUARD_ROOT/backups/hourly/bluey-postgres-20260830T115500Z.pgdump"
printf 'legitimate-writer-partial' > "$partial_backup"
partial_sha="$(printf 'b%.0s' {1..64})"
printf '%s  %s\n' "$partial_sha" "$(basename "$partial_backup")" \
    > "${partial_backup}.sha256"
writer_lock_epoch=$((guard_ok_epoch - 10 * 60))
partial_epoch=$((guard_ok_epoch - 5 * 60))
: > "$guard_lock"
perl -e 'utime $ARGV[0], $ARGV[0], $ARGV[1]' "$writer_lock_epoch" "$guard_lock"
perl -e 'utime $ARGV[0], $ARGV[0], $ARGV[1]' "$partial_epoch" "$partial_backup" \
    "${partial_backup}.sha256"
MOCK_FLOCK_ACTIVE_FD=7 run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1
grep -Fqx 'status=ok' "$STATUS_FILE" ||
    fail "in-progress backup caused a false production failure"
grep -Fqx 'backup_writer_active=1' "$STATUS_FILE" ||
    fail "guard did not persist the active backup writer"
grep -Fqx 'backup_status=in_progress' "$STATUS_FILE" ||
    fail "guard did not defer backup-tree inspection while writer held the lock"
grep -Fqx 'backup_name=none' "$STATUS_FILE" ||
    fail "guard scanned mutable backup names without the shared reader lock"
grep -Fqx 'backup_checksum_status=pending' "$STATUS_FILE" ||
    fail "guard did not mark checksum inspection pending during overlap"
grep -Fqx 'backup_unverified_count=0' "$STATUS_FILE" ||
    fail "guard classified the current writer snapshot as an unverified backlog"

if run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1; then
    fail "partial finalized snapshot passed after the writer lock released"
fi
grep -Fqx 'backup_offsite_status=missing' "$STATUS_FILE" ||
    fail "released partial snapshot was not classified as missing proof"

writer_warn_epoch=$((guard_ok_epoch - 45 * 60))
perl -e 'utime $ARGV[0], $ARGV[0], $ARGV[1]' "$writer_warn_epoch" "$guard_lock"
MOCK_FLOCK_ACTIVE_FD=7 run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1
grep -Fqx 'status=warn' "$STATUS_FILE" ||
    fail "backup writer warning boundary was not durable"
grep -Fq 'backup_writer_slow' "$STATUS_FILE" ||
    fail "backup writer warning reason was omitted"

writer_hard_epoch=$((guard_ok_epoch - 90 * 60))
perl -e 'utime $ARGV[0], $ARGV[0], $ARGV[1]' "$writer_hard_epoch" "$guard_lock"
if MOCK_FLOCK_ACTIVE_FD=7 run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1; then
    fail "backup writer hard-age boundary passed"
fi
grep -Fq 'backup_writer_stuck' "$STATUS_FILE" ||
    fail "backup writer hard failure reason was omitted"
rm -f "$partial_backup" "${partial_backup}.sha256" "$guard_lock"

backup_warn_epoch=$((guard_backup_mtime + 120 * 60))
run_backup_health_guard "$backup_warn_epoch" >/dev/null 2>&1
grep -Fqx 'status=warn' "$STATUS_FILE" || fail "stale backup warning was not durable"
grep -Fqx 'backup_status=warn' "$STATUS_FILE" || fail "backup warning status is missing"
grep -Fqx 'backup_age_minutes=120' "$STATUS_FILE" ||
    fail "backup warning boundary is not inclusive"
grep -Fq '"backupStatus":"warn"' "$MOCK_CURL_LOG" ||
    fail "stale backup warning alert omitted backup state"

backup_hard_epoch=$((guard_backup_mtime + 180 * 60))
if run_backup_health_guard "$backup_hard_epoch" >/dev/null 2>&1; then
    fail "hard-stale database backup passed the guard"
fi
grep -Fqx 'status=fail' "$STATUS_FILE" || fail "hard-stale backup did not fail"
grep -Fqx 'backup_status=fail' "$STATUS_FILE" || fail "hard backup state is missing"
grep -Fqx 'backup_age_minutes=180' "$STATUS_FILE" ||
    fail "backup hard boundary is not inclusive"
grep -Fq '"backupStatus":"fail"' "$MOCK_CURL_LOG" ||
    fail "hard-stale backup alert omitted backup state"

mv "${guard_backup}.offsite-verified" "$GUARD_ROOT/marker.missing"
: > "$MOCK_CURL_LOG"
if run_backup_health_guard "$((backup_hard_epoch + 60))" >/dev/null 2>&1; then
    fail "missing offsite proof marker passed the guard"
fi
grep -Fqx 'backup_offsite_status=missing' "$STATUS_FILE" ||
    fail "missing offsite marker was not classified"
grep -Fq '"backupStatus":"fail"' "$MOCK_CURL_LOG" ||
    fail "missing offsite marker did not alert"
mv "$GUARD_ROOT/marker.missing" "${guard_backup}.offsite-verified"

cp "${guard_backup}.offsite-verified" "$GUARD_ROOT/marker.good"
sed 's/^bytes=.*/bytes=999999/' "$GUARD_ROOT/marker.good" \
    > "${guard_backup}.offsite-verified"
if run_backup_health_guard "$((backup_hard_epoch + 120))" >/dev/null 2>&1; then
    fail "offsite marker/stat mismatch passed the guard"
fi
grep -Fqx 'backup_offsite_status=invalid' "$STATUS_FILE" ||
    fail "offsite marker mismatch was not classified"
mv "$GUARD_ROOT/marker.good" "${guard_backup}.offsite-verified"

mv "${guard_backup}.sha256" "$GUARD_ROOT/checksum.saved"
if run_backup_health_guard "$((backup_hard_epoch + 180))" >/dev/null 2>&1; then
    fail "missing checksum sidecar passed the guard"
fi
grep -Fqx 'backup_checksum_status=missing' "$STATUS_FILE" ||
    fail "missing checksum was not classified"
mv "$GUARD_ROOT/checksum.saved" "${guard_backup}.sha256"

mkdir -p "$GUARD_ROOT/snapshot.saved"
mv "$guard_backup" "${guard_backup}.sha256" "${guard_backup}.offsite-verified" \
    "$GUARD_ROOT/snapshot.saved/"
: > "$MOCK_CURL_LOG"
if run_backup_health_guard "$((backup_hard_epoch + 240))" >/dev/null 2>&1; then
    fail "missing active database backup passed the guard"
fi
grep -Fqx 'backup_name=none' "$STATUS_FILE" || fail "missing backup name is not durable"
grep -Fqx 'backup_status=fail' "$STATUS_FILE" || fail "missing backup did not fail"
grep -Fq '"backupStatus":"fail"' "$MOCK_CURL_LOG" || fail "missing backup did not alert"
mv "$GUARD_ROOT/snapshot.saved/"* "$GUARD_ROOT/backups/hourly/"

run_backup_health_guard "$guard_ok_epoch" >/dev/null 2>&1
grep -Fqx 'status=ok' "$STATUS_FILE" || fail "backup-health recovery did not persist"
grep -Fqx 'backup_checksum_status=ok' "$STATUS_FILE" ||
    fail "recovered checksum status is missing"
grep -Fqx 'backup_offsite_status=ok' "$STATUS_FILE" ||
    fail "recovered offsite status is missing"

NONPROD_STATUS="$GUARD_ROOT/status/nonprod.status"
mkdir -p "$GUARD_ROOT/nonprod-empty-backups"
: > "$MOCK_DF_COUNT_FILE"
PATH="$MOCK_BIN:$PATH" \
BLUEY_ENV_FILE="$TEST_ROOT/missing.env" \
BLUEY_DISK_GUARD_PATH="$GUARD_ROOT/root" \
BLUEY_API_ROOT="$GUARD_ROOT/api" \
BLUEY_WEB_ROOT="$GUARD_ROOT/web" \
BLUEY_BACKUP_DIR="$GUARD_ROOT/nonprod-empty-backups" \
BLUEY_API_LOG_DIR="$GUARD_ROOT/logs" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$GUARD_ROOT/log-work" \
BLUEY_DISK_BUILD_SCAN_ROOT="$GUARD_ROOT/build" \
BLUEY_DISK_GUARD_STATUS_FILE="$NONPROD_STATUS" \
BLUEY_SERVER_DB_BACKEND=postgres \
BLUEY_DISK_BACKUP_HEALTH_REQUIRED=0 \
BLUEY_DISK_GUARD_REQUIRE_ALERT=0 \
MOCK_DATE_EPOCH="$guard_ok_epoch" \
MOCK_DF_MODE=healthy \
    "$ROOT/ops/bluey-disk-guard.sh" check >/dev/null
grep -Fqx 'status=ok' "$NONPROD_STATUS" || fail "non-production backup opt-out failed"
grep -Fqx 'backup_status=disabled' "$NONPROD_STATUS" ||
    fail "non-production backup opt-out is not durable"
grep -Fqx 'backup_health_required=0' "$NONPROD_STATUS" ||
    fail "non-production backup requirement switch is not durable"

grep -Fq '*/15 * * * * root /usr/local/sbin/bluey-disk-guard.sh check' \
    "$ROOT/ops/install-bluey-log-guards.sh" || fail "installer lacks the 15-minute check"
grep -Fq '27 * * * * root /usr/local/sbin/bluey-disk-guard.sh --prune' \
    "$ROOT/ops/install-bluey-log-guards.sh" || fail "installer lacks hourly bounded prune"
grep -Fq 'secure_root_directory "$RESTORE_DEADMAN_ROOT" 0700' \
    "$ROOT/ops/install-bluey-log-guards.sh" ||
    fail "installer does not secure the restore dead-man marker root"
grep -Fq 'secure_root_directory "$RESTORE_LOCK_ROOT" 0700' \
    "$ROOT/ops/install-bluey-log-guards.sh" ||
    fail "installer does not secure the restore-drill lock root"
grep -Fq 'touch "$BACKUP_ROOT/.backup.lock"' \
    "$ROOT/ops/install-bluey-log-guards.sh" ||
    fail "installer does not establish the shared backup/guard lock inode"
grep -Fq 'backup_reader_lock_held=1' "$ROOT/ops/bluey-disk-guard.sh" ||
    fail "disk guard does not retain its shared backup lock through metadata inspection"
grep -Fq '/etc/logrotate.d/bluey-ops' \
    "$ROOT/ops/install-bluey-log-guards.sh" ||
    fail "installer lacks a root-owned ops-log rotation policy"
grep -Fq 'maxsize 20M' "$ROOT/ops/install-bluey-log-guards.sh" ||
    fail "installer does not bound each root-owned cron log"
for policy_script in backup-bluey-db.sh archive-bluey-logs.sh bluey-disk-guard.sh; do
    grep -Fq '/etc/bluey-api/bluey-storage.env' "$ROOT/ops/$policy_script" ||
        fail "$policy_script does not load the shared storage policy"
done
[ -z "$(grep -E 'test-backup-secret|test-log-secret|test-backup-key|test-log-key' "$MOCK_AWS_LOG" || true)" ] ||
    fail "credential material appeared in provider launcher argv"
if grep -E 'run_bounded .*env .*AWS_|run_bounded .*env PGDATABASE=' \
    "$ROOT/ops/backup-bluey-db.sh" "$ROOT/ops/archive-bluey-logs.sh" >/dev/null; then
    fail "storage launcher still places credential assignments in env argv"
fi

echo "test-bluey-storage-guards: PASS"
