# Backend Contracts

Bluey's production backend is a managed cloud control plane for auth, sync, RAG, provider routing, billing state, retention, export, and deletion. The desktop remains responsible for visible capture consent, local resilience, and offline queueing.

## Service Boundaries

- API gateway: validates auth, rate limits, routes REST and streaming requests.
- Realtime gateway: WebSocket or SSE answer sessions, partial STT, answer deltas, citations, and final metadata.
- Sync API: idempotent meeting/event/artifact metadata ingestion.
- Artifact API: short-lived direct-to-object-storage upload/download URLs.
- RAG service: tenant-scoped retrieval, citation shaping, active-meeting cache.
- Provider router: server-side STT, OCR, vision, embedding, and LLM calls with fallback policy.
- Worker fleet: durable background jobs for chunking, embeddings, extraction, retention, export, and deletion.
- Admin/billing API: workspace membership, plan limits, invoices, audit log, retention policy.

## Auth And Device Flow

1. Desktop opens `POST /auth/device-code` or browser login.
2. User authenticates in web flow.
3. Desktop exchanges device code through `POST /auth/token`.
4. API returns short-lived access token and refresh token bound to `device_id`.
5. Desktop stores refresh token in Keychain or Credential Manager.
6. Desktop registers capabilities with `POST /devices/register`.
7. Every sync/answer request includes workspace id, device id, app version, and request id.

Token rules:

- Access tokens should expire in 15 minutes.
- Refresh tokens should rotate on every use.
- Device revoke invalidates all refresh tokens for that device.
- Provider API keys never leave the server in managed mode.
- Local development can keep environment-key mode behind a non-production build flag.

## API Contract

The first concrete OpenAPI skeleton lives at `infra/openapi.yaml`.

Required public routes:

- `POST /auth/device-code`
- `POST /auth/token`
- `POST /auth/refresh`
- `POST /auth/logout`
- `POST /devices/register`
- `GET /settings`
- `PUT /settings`
- `POST /meetings`
- `POST /meetings/{meeting_id}/events`
- `POST /artifacts/upload-url`
- `POST /sync`
- `POST /answers/stream`
- `POST /rag/query`
- `GET /billing/portal`
- `POST /export-requests`
- `POST /deletion-requests`

Idempotency:

- Mutating routes accept `Idempotency-Key`.
- Desktop event ids are ULIDs generated before enqueueing locally.
- Server writes are unique on `(workspace_id, source_event_id)` for meeting events.
- Artifact uploads are completed by hash and size, not just filename.

## Storage Model

The migration outline lives at `infra/migrations/001_initial_cloud_schema.sql`.

Core tables:

- `users`
- `workspaces`
- `workspace_members`
- `devices`
- `sessions`
- `meetings`
- `meeting_events`
- `artifacts`
- `memory_chunks`
- `rag_citations`
- `answer_runs`
- `settings_profiles`
- `audit_log`
- `export_requests`
- `deletion_requests`

Data placement:

- Postgres stores identity, metadata, permissions, settings, billing state, and audit records.
- Object storage stores raw screenshots, documents, optional retained audio chunks, exports, and derived OCR payloads.
- `pgvector` stores early retrieval embeddings in `memory_chunks.embedding`.
- Redis or managed queues store transient jobs and live-session caches.

Retention:

- `settings_profiles.retention_days` drives meeting, artifact, and vector expiry.
- Deletion requests enqueue metadata tombstoning, object deletion, vector deletion, export purge, and audit write jobs.
- RAG queries must filter out tombstoned rows and expired rows before ranking.

## Worker Queues

Queue definitions live at `infra/queues/workers.yaml`.

Required queues:

- `stt.realtime`
- `vision.ocr`
- `transcript.chunk`
- `embedding.write`
- `meeting.extract`
- `rag.compact`
- `retention.sweep`
- `export.build`
- `delete.cascade`
- `billing.meter`

Worker contract:

- Every job includes `job_id`, `workspace_id`, `trace_id`, `created_at`, `attempt`, and `payload_version`.
- Jobs are idempotent by `job_id`.
- Workers write progress to `audit_log` for export, deletion, and retention jobs.
- Provider calls record latency, model, token/audio/image usage, and estimated cost.
- Failed jobs use bounded exponential backoff and land in a dead-letter queue after the configured attempt count.

## Sync Flow

Desktop local state:

```text
meeting cache -> pending sync queue -> signed upload -> sync commit -> local ack
```

Cloud flow:

```text
POST /sync
  -> validate workspace/device/settings
  -> upsert meeting metadata
  -> store event metadata
  -> enqueue chunk/extract/embed jobs
  -> return ack cursor and per-event status
```

Rules:

- Overlay rendering never blocks on cloud sync.
- Audio/STT and answer streaming can run while background sync drains.
- If the cloud is degraded, desktop keeps a bounded local queue and surfaces health in settings and CLI.
- Sync responses include retryable vs permanent failure reason per event.

## RAG Flow

Ingestion:

```text
meeting event/artifact
  -> chunk worker
  -> embedding worker
  -> memory_chunks insert
  -> active-session cache refresh
```

Retrieval:

```text
answer request
  -> workspace and retention filters
  -> active meeting top-k
  -> workspace memory top-k
  -> rerank and cite
  -> provider router
  -> answer stream
```

Citation requirements:

- Every retrieved chunk keeps `source_type`, `source_id`, `meeting_id`, optional `artifact_id`, and text span or page coordinates.
- Answer deltas may stream before all citations are known, but final event must include complete citations.
- Redacted or deleted sources must never be returned as citations.

## Observability

Minimum production signals:

- API latency by route and workspace plan.
- Active realtime sessions.
- Queue depth and age by queue.
- Provider latency, error rate, fallback rate, and cost.
- Sync backlog per device.
- RAG query latency and result count.
- Retention/export/delete job completion.
- Installer version adoption and crash-free sessions.

