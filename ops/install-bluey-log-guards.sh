#!/usr/bin/env bash
# Install Bluey production log rotation, off-host log archive, and disk guard
# cron jobs on an API host.
#
# Run on the droplet from a checked-out release repo:
#   sudo ops/install-bluey-log-guards.sh

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SERVICE_NAME="${BLUEY_API_SERVICE_NAME:-bluey-api}"
API_ROOT="${BLUEY_API_ROOT:-/opt/bluey-api}"
LOG_DIR="${BLUEY_API_LOG_DIR:-/var/log/bluey-api}"
ARCHIVE_ROOT="${BLUEY_LOG_ARCHIVE_LOCAL_DIR:-/var/backups/bluey-api/logs}"
USER_NAME="${BLUEY_API_USER:-bluey}"
GROUP_NAME="${BLUEY_API_GROUP:-bluey}"

if [ "$(id -u)" != "0" ]; then
    echo "install-bluey-log-guards.sh must run as root" >&2
    exit 1
fi

install -m 0750 "$ROOT/ops/archive-bluey-logs.sh" /usr/local/sbin/archive-bluey-logs.sh
install -m 0750 "$ROOT/ops/bluey-disk-guard.sh" /usr/local/sbin/bluey-disk-guard.sh

mkdir -p "$LOG_DIR" "$API_ROOT/logs" "$ARCHIVE_ROOT"
cron_logs=(
    "$LOG_DIR/log-archive-cron.log"
    "$LOG_DIR/disk-guard-cron.log"
)
for cron_log in "${cron_logs[@]}"; do
    touch "$cron_log"
    chmod 0640 "$cron_log"
done
if id "$USER_NAME" >/dev/null 2>&1; then
    chown -R "$USER_NAME:$GROUP_NAME" "$LOG_DIR" "$API_ROOT/logs"
fi
chmod 0750 "$LOG_DIR" "$API_ROOT/logs" "$ARCHIVE_ROOT"

if command -v logrotate >/dev/null 2>&1 && [ -d /etc/logrotate.d ]; then
    if id "$USER_NAME" >/dev/null 2>&1; then
        su_line="    su ${USER_NAME} ${GROUP_NAME}"
    else
        su_line=""
    fi
    cat > "/etc/logrotate.d/${SERVICE_NAME}" <<ROTATE
$LOG_DIR/*.log
$API_ROOT/logs/*.log
$API_ROOT/logs/*/*.log {
    daily
    rotate 7
    maxsize 50M
    missingok
    notifempty
    compress
    delaycompress
    dateext
    copytruncate
$su_line
}
ROTATE
    chmod 0644 "/etc/logrotate.d/${SERVICE_NAME}"
else
    echo "warn: logrotate not installed; skipping /etc/logrotate.d/${SERVICE_NAME}" >&2
fi

cat > /etc/cron.d/bluey-log-guards <<'CRON'
SHELL=/bin/bash
PATH=/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin

# Archive production operational logs off-host every hour.
17 * * * * root /usr/local/sbin/archive-bluey-logs.sh >> /var/log/bluey-api/log-archive-cron.log 2>&1

# Keep the droplet hot cache bounded overnight.
27 3 * * * root /usr/local/sbin/bluey-disk-guard.sh --prune >> /var/log/bluey-api/disk-guard-cron.log 2>&1
CRON
chmod 0644 /etc/cron.d/bluey-log-guards

echo "Bluey log guards installed."
echo "Next checks:"
echo "  /usr/local/sbin/archive-bluey-logs.sh"
echo "  /usr/local/sbin/bluey-disk-guard.sh"
