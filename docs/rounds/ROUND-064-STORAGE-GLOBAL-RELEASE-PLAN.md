# Round 064 - Storage + Global Release Plan — 2026-06-19

## Decision

Bluey stays local-first on the customer machine and cloud-managed for paid
provider work.

The customer install must remain simple:

- No Redis.
- No Postgres.
- No pgvector.
- No Docker.
- No local provider keys for normal paid accounts.

The desktop can use local SQLite databases and local Markdown files because
those ship inside the normal app data model and do not require user-managed
infrastructure.

## Current Local Shape

On macOS now, and on Windows when shipped:

- Sessions, transcripts, settings, and pending sync live in local SQLite.
- Attached readable documents are converted to Markdown and stored in Bluey's
  local app data directory.
- Local RAG uses a SQLite-backed vector store with embeddings saved as BLOBs and
  Rust cosine search.
- This is fast enough for alpha and keeps session recall usable even when the
  network is degraded.

The local vector store should be upgraded to `sqlite-vec` or `usearch` only when
real chunk counts prove linear scan is a problem. The current code already notes
that migration point.

## Cloud Shape For Alpha

For the first paid alpha:

- Keep DigitalOcean droplet + Caddy + `bluey-server` + SQLite.
- Keep Square, Resend, provider keys, auth, billing, sync, and managed AI on the
  server.
- Keep backups hourly and off-host.
- Keep all provider keys server-side.

This is intentionally boring and maintainable. It is better than overbuilding
Kubernetes or multi-region databases before we have real usage data.

## Cloudflare R2 Role

Use Cloudflare R2, or another S3-compatible bucket, for object storage:

- signed release artifacts
- `latest.json`
- `latest.json.sig`
- installer scripts
- database backups
- support zip/log exports
- synced raw documents when cloud restore needs the original file
- screenshots / screen artifacts if the user opts into cloud sync
- optional audio chunks if we retain audio later

R2 must not be used as the source of truth for:

- account balances
- billing ledgers
- Square webhook events
- auth tokens
- idempotency rows
- provider capacity state
- vector similarity search

Those need database semantics, transactions, or realtime coordination.

## Redis / Valkey Trigger

Do not add Redis to the desktop.

Add Redis/Valkey to the server before either of these becomes true:

- more than one `bluey-server` process handles live user traffic
- provider capacity/cooldown state must be shared across machines

Until then, in-process buckets are acceptable for one server process and simpler
to operate.

## Postgres + pgvector Trigger

Do not move to Postgres just because it sounds production-shaped.

Move server storage from SQLite to Postgres + pgvector when at least one is true:

- cloud-synced session/history volume makes SQLite backup/restore or query time
  uncomfortable
- we need multiple API servers writing to one database
- tenant-scoped cloud RAG becomes a core product path
- analytics/admin queries begin competing with live request latency

When that happens, Postgres owns accounts, billing, usage events, session
metadata, cloud memory chunks, and pgvector indexes. R2 still owns large blobs.

## Worldwide Release Plan

Downloads and updates should be global before runtime is global:

1. Serve release artifacts and manifests from R2/Cloudflare edge.
2. Keep the app server on DigitalOcean for alpha.
3. Use logs and live smoke data to measure non-US latency.
4. Add regional stateless gateways only when live STT/streaming answer latency
   is visibly painful outside the primary region.

For worldwide runtime, the first split should be:

- regional ingress for live STT websockets and answer SSE/websocket streaming
- central account/billing database
- shared Redis/Valkey provider capacity ledger
- R2 for blobs and artifacts

Avoid multi-region writable account/billing databases until customer traffic
forces that complexity.

## Windows Plan

Windows should share the same backend and storage contract:

- same Bluey account APIs
- same provider routing
- same Square billing
- same R2 release/object storage
- same local SQLite session/RAG model

Windows-specific work is packaging and native behavior:

- installer
- code signing
- WASAPI/system audio
- overlay window invisibility
- click-through/interactive mode
- local app data paths
- Windows Defender false-positive smoke

Do not fork the product architecture for Windows.

## Remaining Required Follow-Up

The local RAG embedding path should use Bluey cloud `/router/embed` for normal
paid accounts instead of requiring an OpenAI key on the laptop. Dev/BYOK keys can
remain as a local fallback, but the production path should be:

```text
desktop doc/transcript chunk
  -> bluey-server /router/embed
  -> server-held provider key
  -> embedding returned to desktop
  -> local SQLite RAG index
```

That gives local-speed retrieval while keeping provider credentials cloud-only.

## Verification

- `docs/DEPLOYMENT-SCALING.md` now reflects the local/DO/R2/Redis/Postgres
  phases.
- `ops/backup-bluey-db.sh` supports Cloudflare R2 by accepting
  `BLUEY_BACKUP_S3_ENDPOINT_URL` for S3-compatible backup upload.

