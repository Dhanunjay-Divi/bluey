# Round 093 - R2 Off-Host Backup Provisioning - 2026-06-21

## Goal

Close the paid-alpha off-host backup gap without moving Bluey to AWS or changing
the current SQLite runtime.

## What Changed

- Created a Bluey-owned Cloudflare R2 bucket:
  - `bluey-prod`
  - current production backup prefix: `backups/api/`
- Wired the production droplet environment with the R2/S3-compatible backup
  destination:
  - `OFFSITE_DESTINATION=s3://bluey-prod/backups/api/`
  - `BLUEY_BACKUP_S3_ENDPOINT_URL=https://<cloudflare-account-id>.r2.cloudflarestorage.com`
  - S3-compatible R2 credentials are stored only in `/etc/bluey-api/bluey-api.env`
    on the droplet.
- Installed AWS CLI v2 on the droplet only as an S3-compatible upload client for
  Cloudflare R2. No AWS infrastructure was provisioned.
- Installed and enabled `redis-server` on the droplet as the current
  Valkey/Redis-compatible shared capacity ledger for the single-server alpha.
  This is not a managed multi-server Redis deployment; it is the production
  droplet-local ledger for provider cooldown/rate-limit state until we add a
  second API server.
- Set:
  - `BLUEY_REDIS_URL=redis://127.0.0.1:6379`
  - `BLUEY_REDIS_NAMESPACE=bluey-prod`
  - `BLUEY_RATE_LIMIT_REDIS_STRICT=0`
- Updated `ops/backup-bluey-db.sh` so cron runs load
  `/etc/bluey-api/bluey-api.env` by default. Without this, manual local backups
  worked but off-host upload variables were missing in the cron environment.
- Reinstalled `/usr/local/sbin/backup-bluey-db.sh` on the droplet.
- Confirmed `/etc/cron.d/bluey-api-backup` exists and `cron` is active.
- Marked the off-host backup checklist item complete in
  `docs/PRELAUNCH-CHECKLIST.md`.

## Verification

- `bash -n ops/backup-bluey-db.sh` passed.
- Forced a production droplet backup:

```text
2026-06-21T03:53:00Z backup ok: /var/backups/bluey-api/hourly/bluey-20260621T035256Z.db (942080 bytes)
```

- Verified the local checksum with `sha256sum -c`.
- Verified the R2 objects from the Mac:

```text
bluey-20260621T035256Z.db
bluey-20260621T035256Z.db.sha256
```

- Restore drill from R2 passed:

```text
sha256 matched downloaded DB
sqlite3 PRAGMA integrity_check -> ok
sqlite_master object count -> 56
```

- Reran the production cloud preflight:

```text
ok: BLUEY_REDIS_URL set (shared capacity ledger)
ok: Redis/Valkey ping succeeded
ok: OFFSITE_DESTINATION set
ok: BLUEY_BACKUP_S3_ENDPOINT_URL set (R2/S3-compatible backup endpoint)
ok: AWS_ACCESS_KEY_ID set (R2/S3 backup access key)
ok: AWS_SECRET_ACCESS_KEY set (R2/S3 backup secret)
ok: R2/S3 backup destination reachable
preflight passed: 1 warning(s)
```

The remaining warning is expected for the current single-server alpha:

- Redis strict mode is disabled, so Redis failures fall back to local process
  state.

## Current Truth

- Bluey production backups now have a real off-host R2 destination.
- The running API server snapshot for this round was SQLite-backed by design.
  A later Postgres adapter foundation exists, but production still needs a
  managed Postgres cutover/backfill/smoke before changing the live database.
- R2 is only blob/object storage. It is not the source of truth for auth,
  balances, usage, idempotency, sessions, or vector search.
- Redis/Valkey-compatible state is enabled locally on the production droplet.
  Before multi-server, replace this with managed Redis/Valkey reachable by every
  API server instance.

## Next

1. Replace droplet-local Redis with managed Redis/Valkey before running more
   than one API server instance.
2. Add R2 object APIs later for support zips, exports, and synced raw artifacts.
