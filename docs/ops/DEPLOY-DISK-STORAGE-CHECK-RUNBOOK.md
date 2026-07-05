# Bluey Deploy Disk And Storage Check Runbook

Date: 2026-07-02

## Purpose

Run this before production promotes and after risky deploys so Bluey does not
ship into a droplet that is already near a disk or storage failure. The API host
should keep only hot operational data: runtime config, current release files,
short local logs, backups, and rollback metadata. Durable backups, release
mirrors, support bundles, and large original artifacts belong in private R2 or a
managed database/object-storage service.

## When To Run

- Before every production promote.
- Before any manual production hotfix.
- After deploys that touch logs, backups, release publishing, R2/object storage,
  billing, exports/deletes, diagnostics, or database cleanup.
- Immediately if API health reports database/storage errors, users see missing
  exports/files, installers fall through to HTML, or `df -h /` crosses 75%.

## Stop-The-Line Thresholds

Do not start or continue a deploy until the storage issue is understood if any
of these are true:

- `/` is 80% used or higher.
- `/` has less than 8 GB free.
- `/var/www/bluey/releases` is growing without an intentional retention plan.
- `/var/log/bluey-api`, `/opt/bluey-api/logs`, or uploaded diagnostics exceed
  the configured local hot-cache cap.
- `/var/backups/bluey-api` has no fresh backup or is growing without pruning.
- `/tmp`, `/root`, `/opt`, or the deploy user's home contains large source/build
  trees not part of release storage.
- Journald is using multiple GB and was not intentionally configured that way.
- Recent logs contain `database or disk is full`, `no space left`, R2
  `AccessDenied`, `SignatureDoesNotMatch`, failed backup uploads, failed object
  deletes, or repeated provider billing/reload errors.

## Quick Check

Set the host/root variables for the target environment.

```bash
BLUEY_HOST=root@165.227.77.152
BLUEY_API_ROOT=/opt/bluey-api
BLUEY_WEB_ROOT=/var/www/bluey
BLUEY_BACKUP_ROOT=/var/backups/bluey-api

ssh "$BLUEY_HOST" "
set -euo pipefail
echo '== disk =='
df -h /
echo '== top /opt dirs =='
du -xhd1 /opt 2>/dev/null | sort -h | tail -30
echo '== top web dirs =='
du -xhd1 $BLUEY_WEB_ROOT 2>/dev/null | sort -h | tail -30
echo '== bluey hot data =='
du -sh $BLUEY_API_ROOT $BLUEY_BACKUP_ROOT /var/log/bluey-api $BLUEY_WEB_ROOT/releases 2>/dev/null || true
echo '== tmp/root large files =='
find /tmp /root -xdev -type f -size +100M -printf '%s %TY-%Tm-%Td %p\n' 2>/dev/null | sort -n | tail -30 || true
echo '== journal disk =='
journalctl --disk-usage || true
echo '== recent storage errors =='
journalctl -u bluey-api -u caddy --since '24 hours ago' --no-pager |
  grep -Ei 'database or disk is full|no space|AccessDenied|SignatureDoesNotMatch|backup failed|failed backup|backup upload|object delete|export failed|panic|archive failed|failed archive' |
  tail -80 || true
"
```

## Backup And Restore Check

```bash
ssh "$BLUEY_HOST" "
set -euo pipefail
echo '== newest backups =='
find /var/backups/bluey-api/hourly -maxdepth 1 -type f \( -name '*.db' -o -name '*.pgdump' \) \
  -printf '%TY-%Tm-%Td %TH:%TM %s %p\n' 2>/dev/null | sort | tail -10 || true
echo '== backup checksums =='
find /var/backups/bluey-api/hourly -maxdepth 1 -type f -name '*.sha256' \
  -printf '%TY-%Tm-%Td %TH:%TM %s %p\n' 2>/dev/null | sort | tail -10 || true
"
```

For a release touching schema, exports, deletes, storage, billing, or account
state, run a restore drill before production promote:

```bash
ssh "$BLUEY_HOST" "BLUEY_RESTORE_DRILL_DATABASE_URL='<non-prod-drill-db-url>' /usr/local/sbin/restore-drill-bluey-db.sh"
```

SQLite deployments can run the drill without a target database. Postgres
deployments must set `BLUEY_RESTORE_DRILL_DATABASE_URL` and the script must
refuse to restore into the live `BLUEY_DATABASE_URL`.

## Database Size Check

SQLite:

```bash
ssh "$BLUEY_HOST" "
sqlite3 /opt/bluey-api/bluey.db '
  PRAGMA page_count;
  PRAGMA page_size;
  PRAGMA freelist_count;
  SELECT \"accounts\", count(*) FROM accounts;
  SELECT \"usage_events\", count(*) FROM usage_events;
'
"
```

Postgres:

```bash
ssh "$BLUEY_HOST" "
set -a
. /etc/bluey-api/bluey-api.env
[ -f /etc/bluey-api/bluey-postgres.env ] && . /etc/bluey-api/bluey-postgres.env
set +a
psql \"\$BLUEY_DATABASE_URL\" -Atqc '
  select current_database(), pg_size_pretty(pg_database_size(current_database()));
  select \"accounts\", count(*) from accounts;
  select \"usage_events\", count(*) from usage_events;
'
"
```

## Safe Cleanup Order

Use the least destructive option that returns the target to a safe margin.

1. Delete obvious temp downloads/build outputs in `/tmp` older than two days.
2. Vacuum journald to a bounded window if journald is large.
3. Force logrotate for Bluey logs if logrotate failed.
4. Confirm local diagnostic hot-cache caps are set.
5. Confirm backup pruning is running and off-host backup upload succeeds.
6. Remove stale release artifacts only after verifying the current release and
   rollback target exist in `latest.json`, `/var/www/bluey/releases`, and R2
   release mirror if enabled.
7. Do not delete the current database, env files, active service unit, current
   release directory, newest verified backups, or R2 objects manually unless the
   exact object keys are known expired and indexed.

Useful commands:

```bash
ssh "$BLUEY_HOST" "
set -euo pipefail
find /tmp -maxdepth 1 -type f -name 'bluey-*' -mtime +2 -print -delete
journalctl --vacuum-time=14d
logrotate -f /etc/logrotate.d/bluey-api 2>/dev/null || true
df -h /
"
```

## Log Archive Guard

Production should keep server logs as a short local hot cache and archive
operational diagnostics off-host. This follows the Pinky pattern: R2 is the
durable diagnostic archive; the droplet is only for fresh incident grep.

Install the host guards after deploying a release that includes `ops/`:

```bash
ssh "$BLUEY_HOST" "cd /opt/bluey-api/current 2>/dev/null || cd /opt/bluey-api && sudo ops/install-bluey-log-guards.sh"
```

Required production env:

```text
BLUEY_REQUIRE_LOG_ARCHIVE=1
BLUEY_LOG_ARCHIVE_DESTINATION=s3://bluey-prod/prod/logs/api
BLUEY_LOG_R2_ENDPOINT_URL=https://<cloudflare-account-id>.r2.cloudflarestorage.com
BLUEY_LOG_R2_ACCESS_KEY_ID=<log-bucket write key>
BLUEY_LOG_R2_SECRET_ACCESS_KEY=<log-bucket write secret>
BLUEY_LOG_R2_REGION=auto
BLUEY_LOG_LOCAL_RETENTION_DAYS=7
BLUEY_LOG_ARCHIVE_LOCAL_RETENTION_DAYS=7
BLUEY_LOG_DIR_MAX_BYTES=536870912
BLUEY_LOG_ROOT_MAX_BYTES=2147483648
```

Manual smoke:

```bash
ssh "$BLUEY_HOST" "
set -euo pipefail
/usr/local/sbin/archive-bluey-logs.sh
/usr/local/sbin/bluey-disk-guard.sh
tail -80 /var/log/bluey-api/log-archive-cron.log 2>/dev/null || true
"
```

Boundary: this archive is for server operational logs and journald output. Do
not auto-upload raw desktop logs, transcripts, screen contents, documents,
clipboard contents, or keystrokes. Desktop evidence should go through
`bluey support` or `bluey logs export`, which redacts by default.

## Deploy Evidence To Record

For every meaningful deploy or hotfix, record the following in the round doc or
deploy review:

- `df -h /` before and after.
- `du -sh` for Bluey logs, backups, release directory, and API root.
- Newest backup filename, size, checksum, and whether off-host upload exists.
- Whether restore drill ran and its output summary.
- Whether object storage is configured for original docs/screenshots.
- Whether `/usr/local/sbin/archive-bluey-logs.sh` uploaded the current log
  bundle and wrote a `.sha256`.
- Whether `/usr/local/sbin/bluey-disk-guard.sh` passed.
- Recent journal check showing no disk/R2/archive/export/delete errors.
- If cleanup was performed, exactly what was deleted and how much space was
  recovered.

## What Should Remain On The Droplet

- Current API runtime config and systemd units.
- Current release files and rollback metadata.
- Current database connection config.
- Short hot operational logs.
- Recent verified backups.

## What Should Not Accumulate On The Droplet

- Source checkouts.
- Loose build trees.
- Unbounded `/tmp` downloads.
- Months of uploaded diagnostics.
- Old release artifacts not mirrored or not referenced by rollback docs.
- Long-term original docs/screenshots/session exports; those belong in private
  object storage and should be served only through authenticated API paths.
