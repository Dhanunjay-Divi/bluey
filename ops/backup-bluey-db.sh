#!/bin/bash
# Bluey API SQLite backup script.
#
# Uses sqlite3's `.backup` (online backup API) so it is safe to run
# while bluey-api.service is live. Rotates 14 hourly + 14 daily local
# snapshots. If OFFSITE_DESTINATION is set, ships the latest hourly to
# that destination (S3 path or rsync target).
#
# Install: cp ops/backup-bluey-db.sh /usr/local/sbin/backup-bluey-db.sh
#          chmod 750 /usr/local/sbin/backup-bluey-db.sh
# Cron:    0 * * * * root /usr/local/sbin/backup-bluey-db.sh

set -euo pipefail

DB_PATH="${BLUEY_DB_PATH:-/opt/bluey-api/bluey.db}"
BACKUP_DIR="${BLUEY_BACKUP_DIR:-/var/backups/bluey-api}"
HOURLY_KEEP=14
DAILY_KEEP=14

# Optional off-host destination. Examples:
#   OFFSITE_DESTINATION=s3://my-bucket/bluey-api-backups/
#   OFFSITE_DESTINATION=user@backuphost:/srv/backups/bluey-api/
OFFSITE_DESTINATION="${OFFSITE_DESTINATION:-}"

if [ ! -f "$DB_PATH" ]; then
    echo "no DB at $DB_PATH" >&2
    exit 1
fi

mkdir -p "$BACKUP_DIR/hourly" "$BACKUP_DIR/daily"

ts="$(date -u +%Y%m%dT%H%M%SZ)"
hourly_target="$BACKUP_DIR/hourly/bluey-${ts}.db"

# Online backup using SQLite's .backup command.
sqlite3 "$DB_PATH" ".backup '$hourly_target'"
chmod 600 "$hourly_target"

# Compute checksum for integrity verification.
sha256sum "$hourly_target" > "${hourly_target}.sha256"

# Rotate hourly: keep last $HOURLY_KEEP.
ls -1t "$BACKUP_DIR/hourly/"*.db 2>/dev/null | tail -n +$((HOURLY_KEEP + 1)) | xargs -r rm -f
ls -1t "$BACKUP_DIR/hourly/"*.sha256 2>/dev/null | tail -n +$((HOURLY_KEEP + 1)) | xargs -r rm -f

# Once per day at 00:xx, also write a daily snapshot.
hour="$(date -u +%H)"
if [ "$hour" = "00" ]; then
    daily_target="$BACKUP_DIR/daily/bluey-$(date -u +%Y%m%d).db"
    cp -f "$hourly_target" "$daily_target"
    cp -f "$hourly_target.sha256" "${daily_target}.sha256"
    # Rotate daily.
    ls -1t "$BACKUP_DIR/daily/"*.db 2>/dev/null | tail -n +$((DAILY_KEEP + 1)) | xargs -r rm -f
    ls -1t "$BACKUP_DIR/daily/"*.sha256 2>/dev/null | tail -n +$((DAILY_KEEP + 1)) | xargs -r rm -f
fi

# Ship to off-host destination if configured.
if [ -n "$OFFSITE_DESTINATION" ]; then
    case "$OFFSITE_DESTINATION" in
        s3://*)
            # Requires aws cli + credentials.
            if command -v aws >/dev/null; then
                aws s3 cp "$hourly_target" "$OFFSITE_DESTINATION" --quiet
                aws s3 cp "${hourly_target}.sha256" "$OFFSITE_DESTINATION" --quiet
            fi
            ;;
        *)
            # Treat as rsync-able destination (user@host:/path).
            rsync -a --quiet "$hourly_target" "${hourly_target}.sha256" "$OFFSITE_DESTINATION" || true
            ;;
    esac
fi

echo "$(date -u +%FT%TZ) backup ok: $hourly_target ($(stat -c%s "$hourly_target") bytes)"
