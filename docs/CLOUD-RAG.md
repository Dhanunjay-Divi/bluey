# Cloud RAG Memory

Bluey's production memory should be cloud-backed and account-scoped. The current local search command is a development slice that proves the app-facing memory surface before the cloud service exists.

## Current Slice

Implemented:

```bash
cue memory search "native overlay"
cue cloud status
cue cloud sync
```

This searches active and archived local meeting records across:

- meeting title and summary
- transcript segments
- attached context metadata
- action items
- decisions
- answer instructions

The cloud commands expose the product control surface and sync/RAG status model. They do not upload data until authenticated cloud sync is implemented.

## Production Design

Cloud ingestion pipeline:

```text
meeting event -> encrypted upload -> chunking -> embeddings -> vector index -> retrieval -> answer context
```

Live commercial path:

```text
desktop audio/screen/context -> Bluey cloud stream -> STT/OCR/vision -> RAG query -> managed provider router -> streaming answer with citations -> overlay
```

Recommended storage boundaries:

- Object storage for raw artifacts.
- Relational database for metadata, permissions, sessions, billing, and audit trails.
- Vector index for retrieval chunks.
- KMS-managed encryption keys.
- Tenant/workspace scoped access checks on every read.

## RAG Objects

Index these as separate chunk types:

- transcript chunk
- recap chunk
- decision
- action item
- answer instruction/persona
- document chunk
- screenshot OCR/vision summary
- code snippet

## Retrieval Requirements

- Low-latency top-k retrieval for live answers.
- Workspace/tenant filtering before ranking.
- Recency and meeting-title boosts.
- Source citations back to meeting/context item.
- Retention-aware deletion from metadata, object storage, and vector index.
- Separate active-session retrieval from broader workspace memory retrieval so live answers stay fast.

## Security Requirements

- TLS in transit.
- Encryption at rest.
- No provider API keys stored on client devices for production managed mode.
- No raw secrets in logs.
- User export and deletion flows.
- Admin-configurable retention.

## Required Services

- Auth and device registration.
- Meeting/event sync.
- Signed artifact upload.
- STT, OCR/vision, embedding, recap, retention, export, and deletion workers.
- RAG query service with citation metadata.
- Managed provider router for STT, vision, embedding, and LLM calls.

Concrete implementation scaffolding:

- `BACKEND-CONTRACTS.md`: auth, sync, RAG, storage, queue, and observability contracts.
- `../infra/openapi.yaml`: API schema for auth, sync, answers, RAG, settings, export, and deletion.
- `../infra/migrations/001_initial_cloud_schema.sql`: cloud metadata and vector storage outline.
- `../infra/queues/workers.yaml`: worker queue definitions and retry budgets.
