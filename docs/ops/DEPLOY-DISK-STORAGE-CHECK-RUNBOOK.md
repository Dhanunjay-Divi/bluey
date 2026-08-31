# Bluey Deploy Disk And Storage Check Runbook

> **Codex preflight:** Load `$bluey-ops` from
> `/Users/uno/.codex/skills/bluey-ops/SKILL.md` before checking or changing
> production storage. Reconcile its memory against this runbook and live state.

Date: 2026-08-30

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
  exports/files, installers fall through to HTML, or `df -h /` reaches 70%.

## Stop-The-Line Thresholds

The installed guard emits a durable early warning at 70% used or below 16 GB
free. Those warning thresholds do not replace the hard release boundary; they
provide enough lead time to correct growth before the hard boundary is reached.

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

## Shared Storage Policy

Backup, log archive, and disk guard scripts load, in order,
`/etc/bluey-api/bluey-api.env`, the optional
`/etc/bluey-api/bluey-storage.env`, and the PostgreSQL or explicitly listed
extra environment fragments. This makes manual and cron runs use the same
policy. Install [the checked-in example](../../ops/bluey-storage.env.example)
as `root:root` mode `0600`, then set the real HTTPS alert receiver outside git.
Every root-run storage script rejects any symlinked, non-regular,
non-root-owned, or group/world-writable fragment before sourcing it.
The receiver is a secret and is passed to curl through stdin configuration,
never through process arguments.
Backup AWS credentials and dedicated `BLUEY_OPS_LOG_R2_*` credentials also live
only in this root-only fragment. Remove them from `bluey-api.env`; application
object storage uses distinct `BLUEY_OBJECT_*` credentials. The service unit has
no write or read grant for `/var/backups/bluey-api`.

The production profile for the current 58 GB host is:

```text
BLUEY_BACKUP_HOURLY_KEEP=4
BLUEY_BACKUP_DAILY_KEEP=7
BLUEY_BACKUP_MIN_HOURLY_KEEP=2
BLUEY_BACKUP_MIN_DAILY_KEEP=2
BLUEY_BACKUP_LOCAL_MAX_BYTES=12884901888
BLUEY_BACKUP_MAX_SNAPSHOT_BYTES=2147483648
BLUEY_BACKUP_MIN_FREE_GB=16
BLUEY_BACKUP_REQUIRE_OFFSITE=1
BLUEY_BACKUP_REQUIRE_STATUS=1
BLUEY_DISK_WARN_USED_PCT=70
BLUEY_DISK_WARN_MIN_FREE_GB=16
BLUEY_DISK_MAX_USED_PCT=80
BLUEY_DISK_MIN_FREE_GB=8
BLUEY_DISK_GUARD_REQUIRE_STATUS=1
BLUEY_DISK_GUARD_REQUIRE_ALERT=1
BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL=https://<secret-alert-receiver>
BLUEY_DISK_BACKUP_HEALTH_REQUIRED=1
BLUEY_DISK_BACKUP_RUN_STATUS_REQUIRED=1
BLUEY_DISK_BACKUP_WARN_AGE_MINUTES=120
BLUEY_DISK_BACKUP_HARD_AGE_MINUTES=180
BLUEY_DISK_BACKUP_WRITER_WARN_AGE_MINUTES=45
BLUEY_DISK_BACKUP_WRITER_HARD_AGE_MINUTES=90
BLUEY_DISK_BACKUP_DAILY_HEALTH_REQUIRED=1
BLUEY_DISK_BACKUP_DAILY_WARN_AGE_MINUTES=2160
BLUEY_DISK_BACKUP_DAILY_HARD_AGE_MINUTES=2880
BLUEY_DISK_BACKUP_REQUIRE_ROOT_OWNERSHIP=1
BLUEY_DISK_LOG_ARCHIVE_HEALTH_REQUIRED=1
BLUEY_DISK_LOG_ARCHIVE_WARN_AGE_MINUTES=120
BLUEY_DISK_LOG_ARCHIVE_HARD_AGE_MINUTES=180
BLUEY_DISK_LOG_ARCHIVE_WRITER_WARN_AGE_MINUTES=45
BLUEY_DISK_LOG_ARCHIVE_WRITER_HARD_AGE_MINUTES=90
BLUEY_DISK_WARN_INODE_USED_PCT=70
BLUEY_DISK_MAX_INODE_USED_PCT=80
BLUEY_PREFLIGHT_REQUIRE_DISK_GUARD=1
BLUEY_PREFLIGHT_REQUIRE_RESTORE_DEADMAN_PROVIDER=1
BLUEY_OPS_LOG_DIR=/var/log/bluey-ops
BLUEY_LOG_DIRS="/var/log/bluey-api /opt/bluey-api/logs /var/log/bluey-ops"
BLUEY_OPS_STATE_ROOT=/var/lib/bluey-ops
BLUEY_DISK_GUARD_TMP_ROOT=/var/lib/bluey-ops/tmp
BLUEY_LOG_ARCHIVE_WORK_DIR=/var/lib/bluey-ops/log-archive
BLUEY_LOG_ARCHIVE_STATUS_FILE=/var/lib/bluey-ops/log-archive.status
BLUEY_JOURNAL_SYSTEM_MAX_USE=1G
BLUEY_JOURNAL_SYSTEM_KEEP_FREE=16G
```

The local byte ceiling applies only to database hot storage (`hourly`, `daily`,
and in-progress `.staging`), not release/bin/round rollback evidence or log
archives that happen to share `/var/backups/bluey-api`. The free-space reserve
still covers the filesystem, and the disk guard separately reports the whole
backup tree. Breaching either backup boundary makes the script prune only exact
offsite-verified DB pairs. It never crosses the hourly/daily minimum. A hard
disk threshold blocks release; an early warning alerts without pretending the
release stop line was crossed. The exclusive lock makes every pre-existing
`.staging/run.*` directory orphaned, so all such runs are removed before the
capacity decision. A midnight run reserves two maximum-size snapshots because
the finalized hourly file and staged daily copy coexist briefly.

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

Every finalized snapshot must have a `.sha256` sidecar. New offsite snapshots
also receive a local `.offsite-verified` proof marker only after remote HEAD
size, remote sidecar, and a full streamed read-back hash all match. When an
offsite destination is configured, count retention and capacity retention both
retain any local snapshot without that exact marker and return non-zero. Each
snapshot passes SQLite quick-check or PostgreSQL archive-list validation before
finalization. Markers are bound to the configured destination, and every
deletion candidate gets a fresh local hash plus remote full read-back.

Before activating the capacity ceiling on a host with legacy snapshots, mint
markers without uploading or deleting anything:

```bash
ssh "$BLUEY_HOST" "/usr/local/sbin/backup-bluey-db.sh --verify-existing"
```

The migration derives the original object basename from each existing checksum
sidecar. This covers daily files whose historical sidecar still names the
hourly object. It performs S3 HEAD, sidecar comparison, and full object
read-back before atomically writing a marker. Stop on the first mismatch. Only
after every retained file is marked may the 12 GB ceiling and 4-hourly/7-daily
count policy be activated.

The 15-minute disk guard also closes the cron-log blind spot. The installer
creates the canonical lock inode, and the guard holds a shared backup lock for
its complete metadata scan: while a legitimate writer is active it checks the last fully
completed snapshot, warns if the writer reaches 45 minutes, and fails at 90.
After lock release, an incomplete finalized pair fails immediately. In
production it requires the newest active-backend hourly snapshot, warns at 120
minutes, and fails at 180 minutes. Missing snapshots or checksum sidecars fail immediately.
When offsite backup is configured or required, a missing or inconsistent
`.offsite-verified` marker also fails. This check is intentionally metadata
only: it compares marker bytes with `stat` and marker SHA-256 with the checksum
sidecar, trusting only root-owned files without group/world write permission.
It does not reread and hash the roughly 0.93 GB dump every 15 minutes. New
non-production installs may set `BLUEY_DISK_BACKUP_HEALTH_REQUIRED=0`; the
production profile must keep it at `1`. It also fails on a persisted
failed/interrupted run, an unverified backlog, DB hot storage over cap, or a
missing/stale required daily snapshot (36-hour warning, 48-hour hard limit).
Disk, inode, and backup reason codes participate in alert deduplication.

New backups use `hourly/` and `daily/` object sub-prefixes. A separate R2
management token must prove bucket locks (recommended minimum 7/35 days) and
longer lifecycle expiration (recommended 14/90 days) for those prefixes plus a
reviewed legacy-flat policy. The host key must be list/head/get/put only and be
denied delete and bucket administration. The host currently cannot read
lifecycle configuration; `AccessDenied` there is expected and is not policy
evidence. At roughly 0.93 GiB per full dump, missing hourly lifecycle creates
about 22 GiB/day or 0.67 TiB/month of remote growth.

Also require a real webhook transition canary, an external alarm when the
root-written guard status is older than 30–45 minutes, and a DigitalOcean agent
70% root Disk Utilization alert with proven operator delivery. A local cron job
cannot detect its own host, cron, network, or configuration failure.

The exact `bluey-brain` target has an edit-verified 2026-08-30 memory policy,
`Bluey memory above 85% for 10 minutes`, ID
`022ce0ab-510c-4067-8488-8b03a0dc8f7e`, using the verified account email
notification. The retained disk-policy ID is
`9c0edae0-c820-46b3-b8d7-5330779ac5c8`; release evidence must still record its
current 70% threshold, target, duration, and an actual notification receipt.

For a release touching schema, exports, deletes, storage, billing, or account
state, a production restore drill remains mandatory. Phase 622 intentionally
does not provide a runnable production command: its external dead-man provider
has not been selected or implemented, and operators must not self-issue the
lease marker to bypass that control.

The provider must provision the empty `template0` target, own a bounded cleanup
registration independent of this host, and issue this exact one-line marker as
a regular `root:root` mode-0600 file directly under the root-owned mode-0700
`$BLUEY_BACKUP_DIR/deadman` directory:

```text
bluey-restore-drill-deadman:v1:<target_cluster_sentinel>:<database>:<expiry_epoch>:<hex_token>:<drill_authority>:<provider_identity>
```

The restore invocation must bind that line to
`BLUEY_RESTORE_DRILL_PRODUCTION_CLUSTER_SENTINEL`,
`BLUEY_RESTORE_DRILL_TARGET_CLUSTER_SENTINEL`,
`BLUEY_RESTORE_DRILL_AUTHORITY`,
`BLUEY_RESTORE_DRILL_DEADMAN_PROVIDER_IDENTITY`, and
`BLUEY_RESTORE_DRILL_DEADMAN_MARKER_FILE`, in addition to the target URL,
safe database name, confirmation, token, expiry, and `drop` teardown mode.

SQLite deployments can run the drill without a target database. Postgres
deployments require an empty `template0` target on a separate disposable server,
direct target ownership, the exact expiring database-comment sentinel, matching
confirmation, and provider-side TTL/drop monitoring. That provider must remain
armed across `SIGKILL` or complete host loss, act no later than the at-most-24h
expiry, alert on cleanup failure, and retain an external audit record. The script selects only
the active backend, validates checksum/catalog, compares live and target
server/database authority, and must prove target teardown. URLs remain in a
mode-0600 libpq service file rather than process arguments. See the production
runbook for provisioning and the exceptional audited same-server override.
After target-absence proof, deleting the local marker is only lease consumption;
it is not provider deregistration. The provider must independently observe
absence and close its registration.

The production storage profile fixes
`BLUEY_PREFLIGHT_REQUIRE_RESTORE_DEADMAN_PROVIDER=1`. The current preflight
therefore fails closed until a later reviewed phase replaces the explicit stop
with live provider status. Do not set it to `0` for production promotion.

On 2026-08-30, a full read-only stream of
`bluey-postgres-20260830T140001Z.pgdump` restored into a temporary local
PostgreSQL 18 + pgvector cluster: 28 accounts, 170,128 global candidates, 77
public tables, and zero invalid indexes. The cluster was removed; production
received no writes. This validates the backup bytes and isolated restore path;
it does not prove provider-side TTL/drop or clear the release stop line.

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
PGDATABASE=\"\$BLUEY_DATABASE_URL\" psql -Atqc '
  select current_database(), pg_size_pretty(pg_database_size(current_database()));
  select \"accounts\", count(*) from accounts;
  select \"usage_events\", count(*) from usage_events;
'
"
```

### Global candidate archive eligibility

The cold-candidate worker can reduce future logical-dump growth, but it is not
a backup and remains disabled until a one-row R2 canary passes. Its read-only
preflight must mirror the code's current membership schema:

```sql
SELECT count(*) AS eligible_candidates
FROM jobs_global_candidates candidate
WHERE candidate.availability_status = 'expired'
  AND candidate.updated_at_ms <=
      (extract(epoch FROM now()) * 1000)::bigint - (30 * 86400 * 1000)
  AND (
      (candidate.archive_state IN ('hot', 'retry')
       AND candidate.archive_next_attempt_at_ms <=
           (extract(epoch FROM now()) * 1000)::bigint)
      OR
      (candidate.archive_state = 'archiving'
       AND coalesce(candidate.archive_lease_expires_at_ms, 0) <=
           (extract(epoch FROM now()) * 1000)::bigint)
  )
  AND NOT EXISTS (
      SELECT 1
      FROM jobs_global_candidate_memberships membership
      WHERE membership.candidate_id = candidate.id
        AND membership.availability_status <> 'expired'
  )
  AND NOT EXISTS (
      SELECT 1
      FROM jobs_global_candidate_materializations materialization
      WHERE materialization.candidate_id = candidate.id
  );
```

Do not use the obsolete timestamp-style membership-expiry predicate: the live
membership authority is `availability_status`. After an exact one-row
PUT/GET/hash/tombstone canary, expand in bounded batches while monitoring
database size, TOAST size, retry state, and source freshness. Use ordinary
`VACUUM (ANALYZE)` as indicated; any online repack is a separately planned
maintenance action, and `VACUUM FULL` is not an ad hoc disk fix.

## Safe Cleanup Order

Use the least destructive option that returns the target to a safe margin.

1. Run the installed guard's bounded prune against its dedicated validated temp
   root. Never configure `/`, `/tmp`, or a path containing `..` as a prune root.
2. Vacuum journald to a bounded window if journald is large.
3. Do not force logrotate. If archive status is stale or failed, disable Bluey's
   logrotate policy and repair/prove the offhost archive before the seven-day hot
   retention window can discard unarchived evidence.
4. Confirm local diagnostic hot-cache caps and the durable archive dead-man are
   set.
5. Confirm off-host backup full read-back succeeds and every deletion candidate
   has a matching `.offsite-verified` marker.
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
/usr/local/sbin/bluey-disk-guard.sh --prune
journalctl --vacuum-time=14d
cat /var/lib/bluey-ops/log-archive.status
df -h /
"
```

## Log Archive Guard

Production should keep server logs as a short local hot cache and archive
operational diagnostics off-host. This follows the Pinky pattern: R2 is the
durable diagnostic archive; the droplet is only for fresh incident grep.

Run `--prepare` only after the API, backup/archive/guard cron, and Bluey's
logrotate policy are stopped. It migrates root ownership and installs scripts
without enabling schedules. Bootstrap and run all canaries, then use
`--activate` as the final step:

```bash
ssh "$BLUEY_HOST" "cd /opt/bluey-api/current 2>/dev/null || cd /opt/bluey-api && sudo ops/install-bluey-log-guards.sh --prepare"
# After backup, archive, alert, R2-policy, and ownership-migration proof. This
# activates the storage hotfix only; it does not clear the product release gate.
ssh "$BLUEY_HOST" "cd /opt/bluey-api/current 2>/dev/null || cd /opt/bluey-api && sudo ops/install-bluey-log-guards.sh --activate"
```

Required production env:

```text
BLUEY_REQUIRE_LOG_ARCHIVE=1
BLUEY_OPS_LOG_ARCHIVE_DESTINATION=s3://bluey-prod/prod/logs/api
BLUEY_OPS_LOG_R2_BUCKET=bluey-prod
BLUEY_OPS_LOG_R2_ENDPOINT_URL=https://<cloudflare-account-id>.r2.cloudflarestorage.com
BLUEY_OPS_LOG_R2_ACCESS_KEY_ID=<root-only prefix-scoped log key>
BLUEY_OPS_LOG_R2_SECRET_ACCESS_KEY=<root-only prefix-scoped log secret>
BLUEY_OPS_LOG_R2_REGION=auto
BLUEY_LOG_STORAGE_PREFIX=prod
BLUEY_OPS_LOG_DIR=/var/log/bluey-ops
BLUEY_LOG_DIRS="/var/log/bluey-api /opt/bluey-api/logs /var/log/bluey-ops"
BLUEY_UPLOAD_LOG_RETENTION_DAYS=180
BLUEY_UPLOAD_LOG_MAX_BYTES=33554432
BLUEY_LOG_LOCAL_RETENTION_DAYS=7
BLUEY_LOG_ARCHIVE_LOCAL_RETENTION_DAYS=7
BLUEY_LOG_DIR_MAX_BYTES=536870912
BLUEY_LOG_ROOT_MAX_BYTES=2147483648
BLUEY_LOG_ARCHIVE_BUNDLE_MAX_BYTES=134217728
BLUEY_LOG_ARCHIVE_MIN_FREE_GB=16
BLUEY_LOG_WORK_RETENTION_MINUTES=1440
BLUEY_LOG_ARCHIVE_WORK_DIR=/var/lib/bluey-ops/log-archive
BLUEY_LOG_WORK_MAX_DIRS=4
BLUEY_LOG_WORK_ROOT_MAX_BYTES=268435456
BLUEY_LOG_ARCHIVE_LOCK_WAIT_SECONDS=0
BLUEY_LOG_ARCHIVE_REQUIRE_STATUS=1
BLUEY_LOG_ARCHIVE_STATUS_FILE=/var/lib/bluey-ops/log-archive.status
BLUEY_DISK_GUARD_STATUS_FILE=/var/lib/bluey-ops/disk-guard.status
BLUEY_DISK_GUARD_REQUIRE_STATUS=1
BLUEY_DISK_GUARD_REQUIRE_ALERT=1
BLUEY_DISK_GUARD_ALERT_WEBHOOK_URL=https://<secret-alert-receiver>
BLUEY_DISK_BACKUP_HEALTH_REQUIRED=1
BLUEY_DISK_BACKUP_RUN_STATUS_REQUIRED=1
BLUEY_DISK_BACKUP_WARN_AGE_MINUTES=120
BLUEY_DISK_BACKUP_HARD_AGE_MINUTES=180
BLUEY_DISK_BACKUP_WRITER_WARN_AGE_MINUTES=45
BLUEY_DISK_BACKUP_WRITER_HARD_AGE_MINUTES=90
BLUEY_DISK_BACKUP_DAILY_HEALTH_REQUIRED=1
BLUEY_DISK_BACKUP_DAILY_WARN_AGE_MINUTES=2160
BLUEY_DISK_BACKUP_DAILY_HARD_AGE_MINUTES=2880
BLUEY_DISK_BACKUP_REQUIRE_ROOT_OWNERSHIP=1
BLUEY_DISK_LOG_ARCHIVE_HEALTH_REQUIRED=1
BLUEY_DISK_LOG_ARCHIVE_WARN_AGE_MINUTES=120
BLUEY_DISK_LOG_ARCHIVE_HARD_AGE_MINUTES=180
BLUEY_DISK_LOG_ARCHIVE_WRITER_WARN_AGE_MINUTES=45
BLUEY_DISK_LOG_ARCHIVE_WRITER_HARD_AGE_MINUTES=90
```

Archive and backup jobs each hold a nonblocking host-wide flock. Overlapping
cron/manual attempts fail before creating or pruning work. The log archive
also removes its own current work directory on every exit and prunes only
bounded, prefixed stale work directories.
Each run has a 128 MiB bundle cap and requires 16 GiB free plus room for both
bounded staging copies. New archives are structurally validated, uploaded, and
proven through remote HEAD, sidecar, and full read-back. The v1 archive script
does not delete hot/rotated source logs because it cannot prove an exact capture
mapping; daily logrotate provides a seven-day bound. The 15-minute guard warns
at two hours and fails at three hours since the last successful archive, and
warns/fails a running writer at 45/90 minutes. During an archive incident,
disable logrotate before that seven-day window expires. Local archive deletion
repeats remote proof.

Root cron output lives in `/var/log/bluey-ops`, is included in the next archive
run, and has its own root-owned seven-day/20 MiB logrotate policy. It is never
written through the service-owned API log directory.

Manual smoke:

```bash
ssh "$BLUEY_HOST" "
set -euo pipefail
/usr/local/sbin/archive-bluey-logs.sh
/usr/local/sbin/bluey-disk-guard.sh
cat /var/lib/bluey-ops/disk-guard.status
cat /var/lib/bluey-ops/log-archive.status
tail -80 /var/log/bluey-ops/log-archive-cron.log 2>/dev/null || true
"
```

Boundary: this archive is for server operational logs and journald output. Its
redaction is best-effort and bundles may still contain user content, so keep the
bucket private and access-audited. Do
not auto-upload raw desktop logs, transcripts, screen contents, documents,
clipboard contents, or keystrokes. Desktop evidence should go through
`bluey support` or `bluey logs export`, which redacts by default.

When `BLUEY_DATABASE_URL` and `psql` are available on the host, the archive
script also records a `diagnostic_log_chunks` row for each uploaded log bundle.
That row stores kind, storage, object key, size, checksum, timestamps, and
expiry only. It does not store the log body in Postgres. This optional index
write uses `psql -X` plus connection, statement, and wall-clock timeouts so it
cannot hold the archive lock indefinitely.

## Deploy Evidence To Record

For every meaningful deploy or hotfix, record the following in the round doc or
deploy review:

- `df -h /` before and after.
- `du -sh` for Bluey logs, backups, release directory, and API root.
- Newest backup filename, size, checksum, and whether off-host upload exists.
- Whether the isolated restore drill ran and its output summary, explicitly
  distinguishing backup-byte validation from external dead-man evidence.
- Whether object storage is configured for original docs/screenshots.
- Whether `/usr/local/sbin/archive-bluey-logs.sh` uploaded the current log
  bundle, wrote a `.sha256` plus proof marker, and persisted `status=ok` in
  `/var/lib/bluey-ops/log-archive.status`.
- Whether `/usr/local/sbin/bluey-disk-guard.sh` passed.
- The durable disk-guard state, early-warning/failure alert receipt, and the
  reported backup, log-work, release, API, and stale-build candidate sizes.
- Newest active-backend backup basename, age, byte size, checksum status, and
  offsite-proof status from `/var/lib/bluey-ops/disk-guard.status`.
- Recent journal check showing no disk/R2/archive/export/delete errors.
- R2 management-token read-back of bucket-lock and lifecycle rules for hourly,
  daily, and legacy prefixes; host-key delete/admin denial; and old-key revocation.
- The real webhook receipt, external 30–45 minute status dead-man, and
  DigitalOcean 70% Disk Utilization alert receipt.
- External restore-target provider evidence: exact registration identity,
  independent armed/expiry observation, host-loss cleanup behavior, provider
  target-absence proof, closure record, and release-preflight result. Phase 622
  has no provider integration, so this item and production promotion remain
  blocked even though the storage hotfix schedules may be active.
- DigitalOcean policy evidence for the exact `bluey-brain` target, including
  disk policy `9c0edae0-c820-46b3-b8d7-5330779ac5c8` and the edit-verified
  85%-for-10-minutes memory policy
  `022ce0ab-510c-4067-8488-8b03a0dc8f7e`; never paste notification addresses.
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
