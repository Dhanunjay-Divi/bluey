# Round 263 - Production Postgres and R2 Storage Setup

## Trigger

The owner asked to move Bluey toward the production storage shape needed for
100+ sessions now and 10k+ sessions later:

- Managed Postgres as the main database.
- R2 for backups and large blobs.
- `BLUEY_OBJECT_*` enabled for original docs/screenshots restore.
- Transcripts, answers, session metadata, account/auth/billing state in
  Postgres.
- Release binaries/backups/exports/support bundles in R2 where applicable.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Live State Before This Round

The live systemd unit already loaded:

- `/etc/bluey-api/bluey-api.env`
- `/etc/bluey-api/bluey-valkey.env`
- `/etc/bluey-api/bluey-postgres.env`

Runtime verification showed:

```text
BLUEY_SERVER_DB_BACKEND=postgres
BLUEY_DATABASE_URL=<set>
BLUEY_REDIS_URL=<set>
BLUEY_RATE_LIMIT_REDIS_STRICT=1
```

The June 30 startup logs confirmed the server was already booting with
`db_backend=Postgres`.

The missing production gap was separate R2 object-byte sync env. Another
important issue surfaced during the setup: the hourly backup script was still
SQLite-only even though the runtime had moved to Postgres.

## Live Changes Made

Enabled R2 object-byte sync on the production API host by adding:

```text
BLUEY_OBJECT_BUCKET=bluey-prod
BLUEY_OBJECT_ENDPOINT_URL=<set>
BLUEY_OBJECT_REGION=auto
BLUEY_OBJECT_KEY_PREFIX=bluey-cloud
BLUEY_OBJECT_RETENTION_DAYS=365
BLUEY_OBJECT_MAX_BYTES=26214400
BLUEY_REQUIRE_OBJECT_STORAGE=1
```

The previous env file was backed up on the droplet at:

```text
/etc/bluey-api/bluey-api.env.bak.20260630T213447Z
```

Restarted `bluey-api.service`; it came back active on port `8080`.

Confirmed runtime env after restart:

```text
BLUEY_SERVER_DB_BACKEND=postgres
BLUEY_DATABASE_URL=<set>
BLUEY_REDIS_URL=<set>
BLUEY_RATE_LIMIT_REDIS_STRICT=1
BLUEY_OBJECT_BUCKET=bluey-prod
BLUEY_OBJECT_ENDPOINT_URL=<set>
BLUEY_OBJECT_REGION=auto
BLUEY_OBJECT_KEY_PREFIX=bluey-cloud
BLUEY_OBJECT_RETENTION_DAYS=365
BLUEY_OBJECT_MAX_BYTES=26214400
BLUEY_REQUIRE_OBJECT_STORAGE=1
```

Verified object storage with a live put/list/delete preflight:

```text
object_storage_r2_preflight=ok
key=bluey-cloud/preflight/object-storage-20260630T213605Z.txt
```

Verified the unauthenticated object upload route fails closed:

```text
POST /sync/artifacts/probe-object/object -> 401
```

## Backup Fix

`ops/backup-bluey-db.sh` was upgraded from a SQLite-only script to a backend
aware database backup script:

- `BLUEY_SERVER_DB_BACKEND=sqlite`
  - Uses SQLite `.backup`.
  - Produces `.db` snapshots.
- `BLUEY_SERVER_DB_BACKEND=postgres`
  - Loads `/etc/bluey-api/bluey-postgres.env` by default.
  - Uses `pg_dump --format=custom --no-owner --no-acl`.
  - Produces `.pgdump` snapshots.

The updated script was installed to:

```text
/usr/local/sbin/backup-bluey-db.sh
```

During forced backup testing, the droplet's original `pg_dump` was version 16
while the managed Postgres server was `18.4`, which failed with a server-version
mismatch. Installed `postgresql-client-18` from the official PostgreSQL APT
repo. After that:

```text
pg_dump (PostgreSQL) 18.4
pg_restore (PostgreSQL) 18.4
```

Forced backup succeeded:

```text
2026-06-30T21:43:57Z backup ok:
/var/backups/bluey-api/hourly/bluey-postgres-20260630T214349Z.pgdump
(11211157 bytes, backend=postgres)
```

`pg_restore -l` could read the archive and showed:

```text
Dumped from database version: 18.4
Dumped by pg_dump version: 18.4
Format: CUSTOM
```

The `.pgdump` and `.pgdump.sha256` appeared in R2 under the existing backup
prefix:

```text
s3://bluey-prod/backups/api/bluey-postgres-20260630T214349Z.pgdump
s3://bluey-prod/backups/api/bluey-postgres-20260630T214349Z.pgdump.sha256
```

The failed zero-byte local `pgdump` from the first attempt was removed.

## Release Artifact Mirror

Mirrored the currently published signed release files into R2 for durability:

```text
s3://bluey-prod/releases/bluey-sh/install.sh
s3://bluey-prod/releases/bluey-sh/install.ps1
s3://bluey-prod/releases/bluey-sh/latest.json
s3://bluey-prod/releases/bluey-sh/latest.json.sig
s3://bluey-prod/releases/bluey-sh/releases/v0.1.21/
```

The public installer/update path still serves from `https://bluey.sh` and still
relies on signed `latest.json`, `latest.json.sig`, and pinned artifact hashes.
R2 is a durable mirror, not a signature bypass.

## Repo Changes

- `ops/backup-bluey-db.sh`
  - Added Postgres-aware backup mode.
  - Added default loading of `/etc/bluey-api/bluey-postgres.env`.
  - Added `.pgdump` snapshot naming and rotation.
- `scripts/publish-bluey-release.sh`
  - Added optional R2/S3 release mirror via
    `BLUEY_RELEASE_MIRROR_DESTINATION`.
- `docs/PRODUCTION-DEPLOY-RUNBOOK.md`
  - Updated backup and restore guidance for Postgres.
- `docs/RELEASE-RUNBOOK.md`
  - Documented release mirror env.
- `ops/bluey-api.env.example`
  - Documented release mirror env.

## Verification

Local:

```text
bash -n ops/backup-bluey-db.sh
bash -n scripts/publish-bluey-release.sh
SQLite smoke with temporary DB passed.
```

Production:

```text
systemctl is-active bluey-api.service -> active
curl http://127.0.0.1:8080/health -> {"status":"ok",...}
R2 object put/list/delete preflight -> ok
Unauthenticated object upload -> 401
Forced Postgres pgdump backup -> ok
pg_restore -l latest.pgdump -> readable custom-format archive
R2 backup listing includes latest .pgdump and .sha256
R2 release mirror includes latest.json, latest.json.sig, installers, and v0.1.21 artifact
```

Final verification generated one more live backup:

```text
2026-06-30T21:52:39Z backup ok:
/var/backups/bluey-api/hourly/bluey-postgres-20260630T215230Z.pgdump
(11211157 bytes, backend=postgres)
```

R2 showed both the `.pgdump` and `.pgdump.sha256` for that final backup.

## Current Storage Contract

Postgres is the source of truth for:

- accounts and auth
- credits, balances, ledgers, idempotency
- usage billing
- transcripts
- answers
- session metadata
- synced context metadata
- RAG/vector metadata

R2 is object/blob storage for:

- Postgres logical backups
- signed release mirrors
- original synced docs/screenshots/images through the authenticated object sync
  endpoints
- future support bundles and exports when those jobs are wired

R2 is not the source of truth for billing, auth, transcripts, or session state.

## Remaining Gates

- Add an admin storage/backup dashboard: last Postgres backup, R2 object counts,
  backup size, failed object uploads, and restore-drill status.
- Add a retention/delete worker that eagerly removes R2 object bytes on account
  deletion instead of relying only on lazy object expiration.
- Add a scheduled release mirror step to the production publish environment by
  setting `BLUEY_RELEASE_MIRROR_DESTINATION`.
- Add a restore drill for the new Postgres `.pgdump` path into a disposable
  database.
