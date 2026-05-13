# Bluey Cloud Infrastructure Skeleton

This directory contains implementation contracts for the managed backend. It is intentionally provider-neutral so the first production pass can target a managed Postgres, object storage, Redis-compatible queue/cache, and container workers without binding the desktop code to one cloud vendor.

Files:

- `openapi.yaml`: public API and streaming contract.
- `migrations/001_initial_cloud_schema.sql`: initial Postgres schema outline with `pgvector`.
- `queues/workers.yaml`: queue names, retry budgets, payload shapes, and worker ownership.

Suggested first deployment:

- API and realtime gateway: container service behind TLS load balancer.
- Database: managed Postgres with `pgvector`.
- Queue/cache: Redis-compatible managed service or cloud-native queue adapter.
- Objects: S3-compatible bucket with KMS encryption and short-lived signed URLs.
- Secrets: managed secrets store, mounted only into API and worker services.
- Observability: OpenTelemetry traces, metrics, structured JSON logs, provider cost events.

Environment contract:

- `BLUEY_DATABASE_URL`
- `BLUEY_REDIS_URL`
- `BLUEY_OBJECT_BUCKET`
- `BLUEY_OBJECT_REGION`
- `BLUEY_KMS_KEY_ID`
- `BLUEY_JWT_ISSUER`
- `BLUEY_JWT_AUDIENCE`
- `BLUEY_PROVIDER_SECRET_PREFIX`
- `BLUEY_WEB_APP_URL`
- `BLUEY_API_PUBLIC_URL`

