# ROUND-373 - R2 Log Disk Guard

Date: 2026-07-05
Branch: `codex/bluey-web-ui-parallel-20260704`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Problem

Bluey had off-host database backups and local desktop log rotation, but it did
not yet have the Pinky-style production guard for server operational logs:
bounded hot logs on the API droplet, off-host R2/S3 archive, explicit disk
guard checks, and deploy evidence that proves logs/backups are not filling the
box.

The user asked whether all needed logs are uploaded to R2 and whether we are
protected from disk growth like Pinky.

## Boundary

Do not upload raw desktop logs, transcripts, screen contents, documents,
clipboard contents, or keystroke/input content automatically.

This round archives server operational diagnostics only:

- `journalctl` output for `bluey-api` and `caddy`
- Bluey API file logs under `/var/log/bluey-api` and `/opt/bluey-api/logs`
- a small manifest describing host/time/window

Desktop evidence remains user-triggered through `bluey support` or
`bluey logs export`, both redacted by default.

## Changes

- Added `ops/archive-bluey-logs.sh`.
  - Builds a redacted operational log bundle.
  - Writes a SHA-256 sidecar.
  - Uploads to `BLUEY_LOG_ARCHIVE_DESTINATION` or
    `BLUEY_LOG_R2_BUCKET`/`BLUEY_OBJECT_BUCKET` using date-partitioned keys.
  - Keeps only short local archives and prunes rotated hot logs by age/size.
- Added `ops/bluey-disk-guard.sh`.
  - Reports root disk usage, Bluey hot-data sizes, journal size, and recent
    storage/archive/R2 errors.
  - `--prune` safely vacuums journald, forces logrotate, removes old temp
    files, and prunes Bluey log hot caches.
- Added `ops/install-bluey-log-guards.sh`.
  - Installs the archive and disk guard scripts to `/usr/local/sbin`.
  - Installs `/etc/logrotate.d/bluey-api`.
  - Installs `/etc/cron.d/bluey-log-guards`.
- Added production log archive env examples to `ops/bluey-api.env.example`.
- Updated `scripts/bluey-cloud-preflight.sh` to check log archive destination,
  endpoint, credentials, reachability where possible, and hot-cache caps.
- Updated the disk/storage runbook with install, smoke, evidence, and privacy
  boundary instructions.

## Production Env Contract

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

## Verification

```bash
bash -n ops/archive-bluey-logs.sh ops/bluey-disk-guard.sh ops/install-bluey-log-guards.sh scripts/bluey-cloud-preflight.sh
```

Passed.

```bash
BLUEY_ENV_FILE=/tmp/bluey-missing.env \
BLUEY_LOG_DIRS="$tmp/logs" \
BLUEY_LOG_ARCHIVE_LOCAL_DIR="$tmp/archive" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$tmp/work" \
BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST=0 \
BLUEY_LOG_ARCHIVE_SERVICES= \
./ops/archive-bluey-logs.sh
```

Passed with a local temp archive. The smoke included a fake API key, email, and
`bluey://` link; extracted archive content contained redaction markers and did
not contain the raw fake secret/email/link.

```bash
BLUEY_ENV_FILE=/tmp/bluey-missing.env \
BLUEY_DISK_MAX_USED_PCT=100 \
BLUEY_DISK_MIN_FREE_GB=0 \
./ops/bluey-disk-guard.sh check
```

Passed locally.

## Live Prod Install Evidence

Host: `bluey-brain` (`root@165.227.77.152`)

- Installed:
  - `/usr/local/sbin/archive-bluey-logs.sh`
  - `/usr/local/sbin/bluey-disk-guard.sh`
  - `/etc/logrotate.d/bluey-api`
  - `/etc/cron.d/bluey-log-guards`
- Added production env guards in `/etc/bluey-api/bluey-api.env` using the
  existing R2 backup credentials:
  - `BLUEY_REQUIRE_LOG_ARCHIVE=1`
  - `BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST=1`
  - `BLUEY_LOG_ARCHIVE_DESTINATION=s3://bluey-prod/prod/logs/api`
  - `BLUEY_LOG_R2_ENDPOINT_URL=<existing R2 endpoint>`
  - `BLUEY_LOG_LOCAL_RETENTION_DAYS=7`
  - `BLUEY_LOG_ARCHIVE_LOCAL_RETENTION_DAYS=7`
  - `BLUEY_LOG_DIR_MAX_BYTES=536870912`
  - `BLUEY_LOG_ROOT_MAX_BYTES=2147483648`
- First disk guard correctly failed before cleanup:
  - `/` was `88%` used with about `7.0G` free.
  - Largest pressure was stale build/source trees under `/opt` and `/tmp`, not
    live Bluey logs.
- Removed stale build/source leftovers:
  - `/tmp/bluey-src*`, `/tmp/bluey-build*`, `/tmp/bluey-web`
  - `/opt/bluey-build`
  - `/opt/bluey-build-codex-*`
  - Kept `/opt/bluey-api`, `/var/backups/bluey-api`,
    `/var/www/bluey/releases`, and `/opt/bluey-builds`.
- Disk after cleanup:
  - `/` is `18%` used with about `48G` free.
- Live R2 archive smoke passed:
  - `bluey-logs-bluey-brain-20260705T091139Z.tar.gz`
  - `bluey-logs-bluey-brain-20260705T091139Z.tar.gz.sha256`
- Final live disk guard passed with no recent storage-error matches.

## Follow-Up

- Install the same guard on preprod if it runs on a separate host/env.
- Add this to the standard deploy promote checklist alongside DB backup and
  restore-drill evidence.
