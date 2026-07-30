# Round 583 - R2 PostgreSQL Backup Recovery

Date: 2026-07-30

Status: recovered and verified

## Scope

This round restores Bluey's hourly PostgreSQL replication to the
`bluey-prod` Cloudflare R2 bucket. Before this work, local PostgreSQL dumps and
restore catalogs were healthy, but the configured R2 credential returned
`AccessDenied` for object writes.

No Bluey API binary, Jobs binary, portal asset, native release, Caddy
configuration, database schema, or feature flag changed. No service restart
was required.

## Root Cause

The backup script, endpoint, bucket, database export, checksum generation, and
cron configuration were valid. A production-equivalent disposable-object
probe reproduced the failure only with the previous R2 credential.

The fault was therefore isolated to credential authorization, not PostgreSQL
backup generation or the replication script.

## Credential Recovery

A new account-owned Cloudflare token was created with object read/write access
limited to the single `bluey-prod` bucket:

| Field | Value |
| --- | --- |
| Token name | `bluey-prod-backup-20260730` |
| Token ID prefix | `0a30bf759f34` |
| Bucket | `bluey-prod` |
| Scope | R2 object read/write for this bucket only |
| Region | `auto` |

The replacement credential first passed a disposable write, read, checksum,
and delete probe outside production. It was then installed atomically in
`/etc/bluey-api/bluey-api.env`.

The prior environment was retained at:

```text
/etc/bluey-api/bluey-api.env.r2-backup-20260730T195045Z
```

Both files remain owned by `root:root` with mode `0640`. The backup script
loads this environment on every run, so no API restart was needed.

Two broad Cloudflare API tokens disclosed during recovery were revoked after
the bucket-scoped credential passed production verification. The temporary
local credential file was deleted. No token or secret is stored in this
document.

## Production Verification

The installed production environment passed a second disposable R2
write/read/checksum/delete cycle.

The real backup script then completed successfully:

```text
/usr/local/sbin/backup-bluey-db.sh
```

Fresh backup evidence:

| Field | Value |
| --- | --- |
| Local path | `/var/backups/bluey-api/hourly/bluey-postgres-20260730T195149Z.pgdump` |
| R2 key | `s3://bluey-prod/backups/api/bluey-postgres-20260730T195149Z.pgdump` |
| Size | 731,580,883 bytes |
| SHA-256 | `1b2435744c88ca7541cd0fc36b3ed8a942722cb5a031af772e4a6b2c42913524` |
| PostgreSQL restore-list entries | 481 |

Verification covered all of the following:

- `pg_restore -l` successfully opened the local custom-format archive;
- the local object and R2 `ContentLength` were both 731,580,883 bytes;
- the uploaded `.sha256` sidecar matched the local archive;
- a complete R2 download streamed through `sha256sum` matched the local
  archive exactly;
- the new credential still read the object after the broad recovery tokens
  were revoked.

## Preserved Rollback Backup

The verified pre-recovery local dump remains intact:

| Field | Value |
| --- | --- |
| Path | `/var/backups/bluey-api/hourly/bluey-postgres-20260730T180001Z.pgdump` |
| Size | 731,540,758 bytes |
| SHA-256 | `109d91b32c5b02f7dba840eadca2144f8b44d4ce3594e6a522fc06dcbccf0dd9` |
| Restore catalog | valid |

This file was not replaced or removed during credential recovery.

## Runtime And Scheduling

`/etc/cron.d/bluey-api-backup` remains installed as `root:root` with mode
`0644`. Its next scheduled run completed independently of the manual recovery
run:

| Field | Value |
| --- | --- |
| Local archive | `bluey-postgres-20260730T200001Z.pgdump` |
| Size | 731,499,734 bytes |
| SHA-256 | `1ee929d063acfa9eff7af7449d0bfbe7adea367d8054f5d5c24935d2ce73dee9` |
| PostgreSQL restore catalog | valid |
| R2 object size | 731,499,734 bytes |
| R2 sidecar checksum | exact match |

This proves cron loaded the rotated credential and replicated a new archive
without manual invocation.

The Bluey API remained active throughout recovery with zero restarts:

```text
NRestarts=0
ActiveState=active
SubState=running
```

## Rollback

If the replacement credential must be rolled back:

1. preserve the current environment and fresh verified dump;
2. restore non-credential settings from
   `/etc/bluey-api/bluey-api.env.r2-backup-20260730T195045Z`;
3. provision a new bucket-scoped R2 credential instead of restoring the
   previous denied credential;
4. run a disposable object round trip;
5. run `/usr/local/sbin/backup-bluey-db.sh`;
6. verify the uploaded object by size, sidecar hash, and full read-back hash.

## Result

Bluey's PostgreSQL backup pipeline again produces valid local rollback media
and independently verified off-host R2 copies. The former `AccessDenied`
waiver is closed.
