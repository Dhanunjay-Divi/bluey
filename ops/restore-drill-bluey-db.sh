#!/usr/bin/env bash
# Bluey restore drill script.
#
# Restores the newest local backup into an explicit test target and prints a
# small verification summary. This script refuses to restore into the live
# BLUEY_DATABASE_URL.

set -Eeuo pipefail

ENV_DIR="${BLUEY_ENV_DIR:-/etc/bluey-api}"
for env_file in "$ENV_DIR/bluey-api.env" "$ENV_DIR/bluey-postgres.env"; do
    if [ -f "$env_file" ]; then
        set -a
        # shellcheck disable=SC1090
        . "$env_file"
        set +a
    fi
done

DB_BACKEND="${BLUEY_SERVER_DB_BACKEND:-sqlite}"
BACKUP_DIR="${BLUEY_BACKUP_DIR:-/var/backups/bluey-api}"
BACKUP_FILE="${BLUEY_RESTORE_DRILL_BACKUP_FILE:-}"
TARGET_DATABASE_URL="${BLUEY_RESTORE_DRILL_DATABASE_URL:-}"

fail() {
    echo "restore drill failed: $*" >&2
    exit 1
}

require_cmd() {
    command -v "$1" >/dev/null 2>&1 || fail "$1 is required"
}

latest_backup() {
    ls -1t "$BACKUP_DIR/hourly"/*.pgdump "$BACKUP_DIR/hourly"/*.db 2>/dev/null | head -n 1 || true
}

if [ -z "$BACKUP_FILE" ]; then
    BACKUP_FILE="$(latest_backup)"
fi

[ -n "$BACKUP_FILE" ] || fail "no backup file found in $BACKUP_DIR/hourly"
[ -f "$BACKUP_FILE" ] || fail "backup file does not exist: $BACKUP_FILE"

case "$BACKUP_FILE" in
    *.pgdump)
        require_cmd pg_restore
        require_cmd psql
        [ -n "$TARGET_DATABASE_URL" ] || fail "BLUEY_RESTORE_DRILL_DATABASE_URL is required for Postgres drills"
        if [ -n "${BLUEY_DATABASE_URL:-}" ] && [ "$TARGET_DATABASE_URL" = "$BLUEY_DATABASE_URL" ]; then
            fail "target database URL matches production BLUEY_DATABASE_URL"
        fi
        if [ -n "${BLUEY_POSTGRES_CA_CERT_PATH:-}" ]; then
            export PGSSLROOTCERT="$BLUEY_POSTGRES_CA_CERT_PATH"
        fi
        pg_restore --clean --if-exists --no-owner --no-acl --dbname "$TARGET_DATABASE_URL" "$BACKUP_FILE"
        accounts="$(psql "$TARGET_DATABASE_URL" -Atqc "select count(*) from accounts" 2>/dev/null || echo unknown)"
        usage_events="$(psql "$TARGET_DATABASE_URL" -Atqc "select count(*) from usage_events" 2>/dev/null || echo unknown)"
        backend="postgres"
        ;;
    *.db)
        require_cmd sqlite3
        scratch="${TMPDIR:-/tmp}/bluey-restore-drill-$(date -u +%Y%m%dT%H%M%SZ).db"
        cp "$BACKUP_FILE" "$scratch"
        integrity="$(sqlite3 "$scratch" "PRAGMA integrity_check;" 2>/dev/null || echo failed)"
        [ "$integrity" = "ok" ] || fail "sqlite integrity_check returned $integrity"
        accounts="$(sqlite3 "$scratch" "select count(*) from accounts;" 2>/dev/null || echo unknown)"
        usage_events="$(sqlite3 "$scratch" "select count(*) from usage_events;" 2>/dev/null || echo unknown)"
        rm -f "$scratch"
        backend="sqlite"
        ;;
    *)
        fail "unsupported backup extension: $BACKUP_FILE"
        ;;
esac

size_bytes="$(wc -c < "$BACKUP_FILE" | tr -d ' ')"
checksum="$(sha256sum "$BACKUP_FILE" 2>/dev/null | awk '{print $1}' || shasum -a 256 "$BACKUP_FILE" | awk '{print $1}')"

cat <<EOF
$(date -u +%FT%TZ) restore drill ok
backend=$backend
backup_file=$BACKUP_FILE
size_bytes=$size_bytes
sha256=$checksum
accounts=$accounts
usage_events=$usage_events
EOF
