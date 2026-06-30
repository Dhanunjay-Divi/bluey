#!/bin/bash
# Bluey API database backup script.
#
# Uses sqlite3's `.backup` for SQLite or pg_dump's custom format for Postgres,
# so it is safe to run while bluey-api.service is live. Rotates 14 hourly + 14
# daily local snapshots. If OFFSITE_DESTINATION is set, ships the latest hourly
# to that destination (S3-compatible path or rsync target).
#
# Install: cp ops/backup-bluey-db.sh /usr/local/sbin/backup-bluey-db.sh
#          chmod 750 /usr/local/sbin/backup-bluey-db.sh
# Cron:    0 * * * * root /usr/local/sbin/backup-bluey-db.sh

set -euo pipefail

# Cron runs this script with a minimal environment. Load the Bluey API env plus
# the Postgres env by default so backend selection, database URL, and R2/S3
# credentials are available there too.
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

DB_PATH="${BLUEY_DB_PATH:-/opt/bluey-api/bluey.db}"
BACKUP_DIR="${BLUEY_BACKUP_DIR:-/var/backups/bluey-api}"
HOURLY_KEEP=14
DAILY_KEEP=14
DB_BACKEND="$(printf '%s' "${BLUEY_SERVER_DB_BACKEND:-sqlite}" | tr '[:upper:]' '[:lower:]')"

# Optional off-host destination. Examples:
#   OFFSITE_DESTINATION=s3://my-bucket/bluey-api-backups/
#   OFFSITE_DESTINATION=user@backuphost:/srv/backups/bluey-api/
#
# For Cloudflare R2 or any S3-compatible object store, set:
#   BLUEY_BACKUP_S3_ENDPOINT_URL=https://<account-id>.r2.cloudflarestorage.com
# The aws CLI also needs credentials in its normal env/config:
#   AWS_ACCESS_KEY_ID=...
#   AWS_SECRET_ACCESS_KEY=...
#   AWS_DEFAULT_REGION=auto
OFFSITE_DESTINATION="${OFFSITE_DESTINATION:-}"
BLUEY_BACKUP_S3_ENDPOINT_URL="${BLUEY_BACKUP_S3_ENDPOINT_URL:-}"

file_size_bytes() {
    if stat -c%s "$1" >/dev/null 2>&1; then
        stat -c%s "$1"
    else
        stat -f%z "$1"
    fi
}

if [ ! -f "$DB_PATH" ]; then
    if [ "$DB_BACKEND" = "sqlite" ]; then
        echo "no SQLite DB at $DB_PATH" >&2
        exit 1
    fi
fi

mkdir -p "$BACKUP_DIR/hourly" "$BACKUP_DIR/daily"

ts="$(date -u +%Y%m%dT%H%M%SZ)"

case "$DB_BACKEND" in
    sqlite | "")
        hourly_target="$BACKUP_DIR/hourly/bluey-${ts}.db"
        # Online backup using SQLite's .backup command.
        sqlite3 "$DB_PATH" ".backup '$hourly_target'"
        ;;
    postgres | postgresql)
        : "${BLUEY_DATABASE_URL:?BLUEY_DATABASE_URL is required for Postgres backups}"
        if ! command -v pg_dump >/dev/null; then
            echo "BLUEY_SERVER_DB_BACKEND=postgres but pg_dump is not installed" >&2
            exit 1
        fi
        if [ -n "${BLUEY_POSTGRES_CA_CERT_PATH:-}" ] && [ -z "${PGSSLROOTCERT:-}" ]; then
            export PGSSLROOTCERT="$BLUEY_POSTGRES_CA_CERT_PATH"
        fi
        hourly_target="$BACKUP_DIR/hourly/bluey-postgres-${ts}.pgdump"
        pg_dump \
            --format=custom \
            --no-owner \
            --no-acl \
            --file "$hourly_target" \
            "$BLUEY_DATABASE_URL"
        ;;
    *)
        echo "unsupported BLUEY_SERVER_DB_BACKEND=$BLUEY_SERVER_DB_BACKEND" >&2
        exit 1
        ;;
esac
chmod 600 "$hourly_target"

# Compute checksum for integrity verification.
sha256sum "$hourly_target" > "${hourly_target}.sha256"

backup_ext="${hourly_target##*.}"

# Rotate hourly backups of the active backend: keep last $HOURLY_KEEP.
ls -1t "$BACKUP_DIR/hourly/"*."$backup_ext" 2>/dev/null | tail -n +$((HOURLY_KEEP + 1)) | xargs -r rm -f
ls -1t "$BACKUP_DIR/hourly/"*."$backup_ext".sha256 2>/dev/null | tail -n +$((HOURLY_KEEP + 1)) | xargs -r rm -f

# Once per day at 00:xx, also write a daily snapshot.
hour="$(date -u +%H)"
if [ "$hour" = "00" ]; then
    if [ "$backup_ext" = "pgdump" ]; then
        daily_target="$BACKUP_DIR/daily/bluey-postgres-$(date -u +%Y%m%d).pgdump"
    else
        daily_target="$BACKUP_DIR/daily/bluey-$(date -u +%Y%m%d).db"
    fi
    cp -f "$hourly_target" "$daily_target"
    cp -f "$hourly_target.sha256" "${daily_target}.sha256"
    # Rotate daily.
    ls -1t "$BACKUP_DIR/daily/"*."$backup_ext" 2>/dev/null | tail -n +$((DAILY_KEEP + 1)) | xargs -r rm -f
    ls -1t "$BACKUP_DIR/daily/"*."$backup_ext".sha256 2>/dev/null | tail -n +$((DAILY_KEEP + 1)) | xargs -r rm -f
fi

# Ship to off-host destination if configured.
if [ -n "$OFFSITE_DESTINATION" ]; then
    case "$OFFSITE_DESTINATION" in
        s3://*)
            # Requires aws cli + credentials. BLUEY_BACKUP_S3_ENDPOINT_URL
            # makes this work with Cloudflare R2 and other S3-compatible stores.
            if command -v aws >/dev/null; then
                aws_args=()
                if [ -n "$BLUEY_BACKUP_S3_ENDPOINT_URL" ]; then
                    aws_args+=(--endpoint-url "$BLUEY_BACKUP_S3_ENDPOINT_URL")
                fi
                aws "${aws_args[@]}" s3 cp "$hourly_target" "$OFFSITE_DESTINATION" --quiet
                aws "${aws_args[@]}" s3 cp "${hourly_target}.sha256" "$OFFSITE_DESTINATION" --quiet
            else
                echo "OFFSITE_DESTINATION is s3:// but aws CLI is not installed" >&2
                exit 1
            fi
            ;;
        *)
            # Treat as rsync-able destination (user@host:/path).
            if command -v rsync >/dev/null; then
                rsync -a --quiet "$hourly_target" "${hourly_target}.sha256" "$OFFSITE_DESTINATION"
            else
                echo "OFFSITE_DESTINATION requires rsync, but rsync is not installed" >&2
                exit 1
            fi
            ;;
    esac
fi

echo "$(date -u +%FT%TZ) backup ok: $hourly_target ($(file_size_bytes "$hourly_target") bytes, backend=$DB_BACKEND)"
