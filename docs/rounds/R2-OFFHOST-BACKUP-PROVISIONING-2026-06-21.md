# R2 Off-Host Backup Provisioning - 2026-06-21

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
ok: OFFSITE_DESTINATION set
ok: BLUEY_BACKUP_S3_ENDPOINT_URL set (R2/S3-compatible backup endpoint)
ok: AWS_ACCESS_KEY_ID set (R2/S3 backup access key)
ok: AWS_SECRET_ACCESS_KEY set (R2/S3 backup secret)
ok: R2/S3 backup destination reachable
preflight passed: 2 warning(s)
```

The remaining warnings are expected for the current single-server alpha:

- `BLUEY_REDIS_URL` is unset.
- Redis strict mode is disabled, so Redis failures fall back to local process
  state.

## Current Truth

- Bluey production backups now have a real off-host R2 destination.
- The running API server is still SQLite-backed by design.
- R2 is only blob/object storage. It is not the source of truth for auth,
  balances, usage, idempotency, sessions, or vector search.
- Redis/Valkey is still pending until we run more than one server process or
  instance.

## Next

1. Keep `BLUEY_REDIS_URL` unset for one-server alpha, then enable managed
   Valkey/Redis before multi-server capacity.
2. Add R2 object APIs later for support zips, exports, and synced raw artifacts.
