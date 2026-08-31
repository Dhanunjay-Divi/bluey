# Bluey Cloud Infrastructure Skeleton

This directory contains implementation contracts for the managed backend. It is
intentionally provider-neutral so the production stack can target managed
Postgres, object storage, Redis-compatible capacity state, and workers without
binding the desktop code to one cloud vendor.

Normal Bluey customers do not install anything from this directory. Their
laptop gets Bluey, local SQLite files, and a local RAG cache only. Redis,
Postgres, pgvector, R2/S3, provider keys, and billing secrets are server-side
operator concerns.

The `bluey-server` codebase now has a Postgres runtime adapter foundation. The
Postgres schema below is the cutover contract; do not set
`BLUEY_SERVER_DB_BACKEND=postgres` in production until managed Postgres is
provisioned, migrations/backfill have run, and live smoke has passed.

Files:

- `openapi.yaml`: public API and streaming contract.
- `migrations/001_initial_cloud_schema.sql`: initial Postgres schema outline with `pgvector`.
- `queues/workers.yaml`: queue names, retry budgets, payload shapes, and worker ownership.

Primary architecture doc:

- `../docs/deploy/PRODUCTION-CLOUD-ARCHITECTURE.md`

Paid-alpha preflight:

- `../scripts/bluey-cloud-preflight.sh`

Suggested first deployment:

- Alpha API: one DigitalOcean droplet running `bluey-server` behind Caddy.
- Alpha database: SQLite on the droplet with hourly online backups until the
  Postgres cutover smoke passes.
- Scaled database: managed Postgres with `pgvector`.
- Later queue/cache: Redis-compatible managed service or cloud-native queue
  adapter before more than one server instance handles live traffic.
- Objects: Cloudflare R2 or another S3-compatible bucket for release artifacts,
  backups, support zips, synced raw artifacts, exports, and optional retained
  audio/screen blobs.
- Secrets: managed secrets store, mounted only into API and worker services.
- Observability: OpenTelemetry traces, metrics, structured JSON logs, provider cost events.

Environment contract:

- `BLUEY_DATABASE_URL`
- `BLUEY_REDIS_URL`
- `BLUEY_OBJECT_BUCKET`
- `BLUEY_OBJECT_REGION`
- `BLUEY_OBJECT_ENDPOINT_URL` (for R2/S3-compatible endpoints)
- `BLUEY_KMS_KEY_ID`
- `BLUEY_JWT_ISSUER`
- `BLUEY_JWT_AUDIENCE`
- `BLUEY_PROVIDER_SECRET_PREFIX`
- `BLUEY_WEB_APP_URL`
- `BLUEY_API_PUBLIC_URL`

Backup/off-host object storage contract:

- `OFFSITE_DESTINATION=s3://<bucket>/bluey-api-backups/`
- `BLUEY_BACKUP_S3_ENDPOINT_URL=https://<cloudflare-account-id>.r2.cloudflarestorage.com`
- `AWS_ACCESS_KEY_ID`
- `AWS_SECRET_ACCESS_KEY`
- `AWS_DEFAULT_REGION=auto`

These backup values belong only in root:root mode-0600
`/etc/bluey-api/bluey-storage.env` with a prefix-scoped list/head/get/put key.
They must not be loaded by `bluey-api.service`; application object storage uses
separate `BLUEY_OBJECT_*` credentials. The host key has no delete, lifecycle,
bucket-lock, or administrative permission.

The production storage profile also fixes
`BLUEY_PREFLIGHT_REQUIRE_RESTORE_DEADMAN_PROVIDER=1`. Phase 622 intentionally
fails that release gate until a separate reviewed external control-plane
provider can create, monitor, expire, clean up, and close restore-target leases
without depending on the Bluey host. A local marker is never sufficient.

Do not put Redis, Postgres, pgvector, object-store credentials, or provider keys
on customer desktops. Those are server-side concerns only.
