# Round 262 - Production R2 Storage Audit

## Trigger

The owner asked what Bluey production stores in Cloudflare R2 and how the R2 path works.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Live Production Findings

The live droplet env currently has the backup R2 path configured:

- `OFFSITE_DESTINATION=s3://bluey-prod/backups/api/`
- `BLUEY_BACKUP_S3_ENDPOINT_URL=<set>`
- `AWS_ACCESS_KEY_ID=<set>`
- `AWS_SECRET_ACCESS_KEY=<set>`
- `AWS_DEFAULT_REGION=auto`

The live env check did not show separate `BLUEY_OBJECT_*` artifact-object settings, so production R2 is currently confirmed for off-host API database backups. The code supports raw synced artifact object storage, but the live droplet does not appear to have that object-byte sync lane enabled through env at the time of this audit.

The backup schedule is installed at:

```text
/etc/cron.d/bluey-api-backup
0 * * * * root /usr/local/sbin/backup-bluey-db.sh >> /var/log/bluey-api/backup.log 2>&1
```

R2 backup listing summary at audit time:

```text
Latest observed backup objects:
backups/api/bluey-20260630T190001Z.db
backups/api/bluey-20260630T200001Z.db
backups/api/bluey-20260630T200001Z.db.sha256
backups/api/bluey-20260630T210001Z.db
backups/api/bluey-20260630T210001Z.db.sha256

Total Objects: 470
Total Size: 246984702 bytes
```

No R2 secret values were printed into this document.

## What R2 Stores Today

Confirmed live:

- Hourly API database backup files.
- Matching `.sha256` checksum files.

Not confirmed enabled live:

- Raw synced document/screenshot/object bytes for user context.
- Support zips/log exports.
- Cloud exports.
- Release artifacts from R2. Current release publish path deploys signed release files to the Bluey web droplet path.

## How Backups Work

The backup script uses SQLite's online `.backup` command so it can run while `bluey-api.service` is live. It writes local hourly/daily backup files, writes a SHA256 checksum, rotates local retention, then uploads the latest hourly `.db` and `.sha256` files to the configured R2/S3-compatible destination.

Source: `ops/backup-bluey-db.sh`

## Object Sync Architecture

Bluey also has a separate object-storage implementation for synced artifact bytes:

- Desktop cloud sync uploads raw artifact bytes through:
  - `POST /sync/artifacts/:artifact_id/object`
- Desktop hydration downloads raw artifact bytes through:
  - `GET /sync/artifacts/:artifact_id/object`
- The server stores bytes in an S3-compatible object key:
  - `bluey-cloud/accounts/<account_id>/context/<artifact_id>` by default
- The server returns metadata:
  - `object_key`
  - `size_bytes`
  - `sha256`
  - `content_type`
  - `expires_at_ms`
- The normal `/sync/batch` path stores metadata, transcript segments, answers, context artifact rows, and RAG chunks in the database.

Source files:

- `server/src/object_storage.rs`
- `server/src/api/sync.rs`
- `crates/cue-daemon/src/cloud/sync.rs`
- `crates/cue-cloud-client/src/client.rs`

## What Does Not Belong In R2

R2 is not the source of truth for:

- account identity
- auth tokens
- billing balances
- ledger entries
- idempotency records
- usage rows
- provider routing state
- vector search
- transcript/RAG metadata

Those belong in Postgres/SQLite/cloud sync tables and, for realtime capacity, Redis/Valkey when enabled.

## Security Notes

- Object keys are account-scoped.
- Downloads check that the stored object key belongs to the authenticated account before serving bytes.
- Uploads enforce non-empty body and max object size.
- Default object retention in code is `365` days if object storage is enabled.
- Default max object size in code is `25 MiB` if object storage is enabled.
- Expired objects are lazily deleted on download attempts.

## Current State

Production R2 is actively receiving hourly Bluey API database backups.

Raw artifact object storage is implemented in code but was not observed as enabled in the live droplet env during this audit.

## Recommended Next Steps

- Decide whether production should enable `BLUEY_OBJECT_*` for raw docs/screenshots restore across devices.
- If enabled, keep Postgres as the source of truth and R2 as blob storage only.
- Add an admin storage dashboard showing backup freshness, object counts, total size, failed uploads, and last restore drill.
- Add a retention/delete worker so account deletion removes object bytes eagerly, not only lazily.
