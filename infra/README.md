# Bluey Cloud Infrastructure Skeleton

This directory contains implementation contracts for the managed backend. It is intentionally provider-neutral so the first production pass can target a managed Postgres, object storage, Redis-compatible queue/cache, and container workers without binding the desktop code to one cloud vendor.

Files:

- `openapi.yaml`: public API and streaming contract.
- `migrations/001_initial_cloud_schema.sql`: initial Postgres schema outline with `pgvector`.
- `queues/workers.yaml`: queue names, retry budgets, payload shapes, and worker ownership.

Suggested first deployment:

- Alpha API: one DigitalOcean droplet running `bluey-server` behind Caddy.
- Alpha database: SQLite on the droplet with hourly online backups.
- Later database: managed Postgres with `pgvector` when multi-server or
  cloud-memory scale requires it.
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

Do not put Redis, Postgres, pgvector, or object-store credentials on customer
desktops. Those are server-side concerns only.
