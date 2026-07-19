# Deployment And Scaling Plan

> Historical planning baseline. Statements below about the "current v0.1.0"
> release are preserved as architecture history. Use the signed live manifest
> and `INSTALL.md` for the current customer support matrix.

Bluey should become a desktop product backed by a managed cloud service. The
current v0.1.0 release scope is narrower: macOS arm64, terminal-distributed,
local-first, with the native overlay and helper binaries bundled in a tarball.
The CLI remains the install/launch surface for v0.1.0 and a developer/support
surface long term.

## User-Facing Surfaces

- Desktop overlay: daily use, launched by `bluey on`.
- CLI: v0.1.0 install/launch plus internal diagnostics, smoke tests, and support workflows.
- Desktop settings window: future customer UI for login, permissions, audio devices, hotkeys, privacy, workspace, and plan.
- Web dashboard: future meeting history, search, recaps, action items, attached files, and admin controls.

The commercial promise should be that normal users only need `bluey on` or a
packaged app launcher. Until the app launcher exists, v0.1.0 should be described
as a terminal-distributed build, not a polished GUI installer.

## Desktop Packaging

macOS:

- Current v0.1.0: bundle `bluey`, `bluey-daemon`, `bluey-overlay-macos`,
  `bluey-audio-macos`, and `bluey-whisper-macos` into the macOS arm64 tarball.
- Current v0.1.0: install with `scripts/install.sh` or a downloaded release
  archive plus checksum verification.
- Future: signed `.app`, notarization, and GUI onboarding.
- Future: request microphone and screen-recording permissions through visible onboarding.
- Store local config in the per-user app config directory.
- Use launch agent or login item only after explicit user opt-in.

Windows:

- Source exists for Win32 overlay rendering and WASAPI audio helpers, but Windows
  is not shipped in v0.1.0.
- Before claiming support, package `bluey.exe`, `bluey-daemon.exe`, overlay,
  audio, and whisper helpers with a tested installer.
- Replace the Windows whisper stub with real whisper.cpp integration.
- Code sign binaries/installer when Windows distribution becomes public.
- QA `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` behavior on Windows 10/11.

## Cloud Services

Core APIs:

- `POST /auth/login`
- `POST /devices/register`
- `POST /meetings`
- `POST /meetings/:id/events`
- `POST /artifacts/upload-url`
- `POST /answers/stream`
- `POST /rag/query`
- `POST /sync`
- `GET /settings`
- `POST /settings`
- `GET /billing/portal`
- `POST /export-request`
- `POST /deletion-request`

The concrete route, payload, and streaming skeleton is tracked in `../infra/openapi.yaml`, with backend behavior in `BACKEND-CONTRACTS.md`.

Internal workers:

- STT processing
- screenshot/document OCR and vision summaries
- transcript chunking
- embeddings
- recap/action/decision extraction
- RAG index writes
- retention/deletion/export jobs

Queue names, retry policy, and worker ownership are tracked in `../infra/queues/workers.yaml`.

## Storage

Local desktop:

- SQLite for meeting cache, settings, pending sync queue, and recent memory.
- Local Markdown files for converted attachments and readable document context.
- Local SQLite-backed RAG/vector store for fast per-device retrieval. The current
  implementation stores embeddings as SQLite BLOBs and searches with Rust cosine
  similarity; migrate to `sqlite-vec`/`usearch` only when per-device chunk counts
  make linear scan too slow.
- Temporary capture directory with cleanup.
- Local account/profile storage for Bluey tokens. OS keychain support is
  retained as a legacy/fallback mode, but provider API keys must remain
  server-side for the product path.

Normal users on macOS or Windows should never install Redis, Postgres,
pgvector, Docker, or any database server. The desktop bundle owns its local
SQLite files and helper binaries.

Cloud:

- **Alpha:** one DigitalOcean droplet running `bluey-server` + SQLite behind
  Caddy. This is enough for internal testers and a small paid alpha as long as
  the live smoke, backups, Square webhooks, and provider probes are green.
- **Object storage:** Cloudflare R2, or another S3-compatible bucket, for large
  blobs and global static artifacts: signed release tarballs, `latest.json` /
  `latest.json.sig`, backups, support zips, synced raw documents, screenshots,
  optional audio chunks, and exports. R2 is not the source of truth for billing,
  accounts, ledgers, or vector search.
- **Before multi-server:** Redis/Valkey for shared provider-capacity buckets,
  cooldowns, and rate-limit state. In-process buckets are acceptable for a
  single server only.
- **When cloud memory/search grows:** Postgres + `pgvector` for users,
  workspaces, session metadata, billing ledger, cloud-synced memory chunks, and
  tenant-scoped vector search. Dedicated vector infrastructure can come later.
- Queue/cache: Redis-compatible managed service or cloud-native queue adapter.
- KMS/secrets manager for provider keys and envelope encryption.

The first Postgres schema outline is tracked in `../infra/migrations/001_initial_cloud_schema.sql`.

## AI Providers

Production provider keys should live server-side:

- OpenAI: reasoning/chat, vision, embeddings.
- Groq: low-latency realtime text path.
- Cerebras: fast answer path where available.
- Deepgram or equivalent: streaming STT.
- Anthropic/Google/Azure OpenAI: optional enterprise/provider fallback.

The desktop should authenticate to Bluey cloud, not store provider keys directly.

Local or environment-provided provider keys can remain useful for development builds, but managed Bluey cloud routing is the product path. Provider health should surface to users as simple answer/audio/cloud status, not as raw key configuration.

## Scaling Shape

Live path:

```text
desktop audio/screen/context -> Bluey cloud stream -> STT/vision/RAG -> provider router -> streaming answer -> overlay
```

Scale independently:

- Realtime answer gateway: horizontally scaled WebSocket/SSE service.
- STT relay workers: autoscale by active audio sessions. A single US-East
  droplet is fine for alpha, but worldwide realtime captions eventually need
  regional relay nodes so audio does not hairpin through one region before
  reaching the STT provider.
- Vision/OCR workers: queue-based, burstable.
- Embedding workers: queue-based, retryable, idempotent.
- RAG query service: low-latency cache for active meetings and workspace memory.
- Artifact service: direct-to-object-storage uploads with short-lived signed URLs.
- Release artifact delivery: Cloudflare/R2 edge cache for fast global installs
  on macOS, Windows, and Linux.

Reliability:

- Keep the desktop usable if cloud is degraded.
- Queue sync locally and retry with backoff.
- Never block overlay rendering on cloud sync, embeddings, recap, or billing checks.
- Stream partial answer cards, then finalize with citations.
- Track health for audio, STT, provider routing, cloud sync, and capture permissions.
- Enforce three layers of capacity before provider dispatch:
  - Per-IP edge buckets for unauthenticated auth abuse protection; authenticated
    router edge buckets are disabled by default so shared NAT/VPN users are not
    punished for being active.
  - Per-provider/model buckets for OpenAI, Anthropic, Deepgram, OpenAI STT
    fallback, and embeddings.
  - Optional per-account guardrails, disabled by default and used only for
    runaway loops, stolen tokens, or abuse response; paid usage itself is
    controlled by account-credit balance and provider availability.
- Scale provider capacity with approved provider allocations: single keys for
  early alpha, comma-separated key pools for approved multi-project or
  enterprise capacity, then a shared global capacity ledger when Bluey runs more
  than one server process.
- Set `BLUEY_REDIS_URL` before running more than one `bluey-server`; Redis makes
  provider capacity global across every instance. The local in-process fallback
  is for dev/single-node resilience only.
- Return typed `429` responses with `retry_after_secs` when Bluey is busy, and
  route LLM lanes through fallback candidates before surfacing an outage.
- For multi-instance deployment, move the capacity buckets from in-process
  governor state to Redis/shared counters so 1000+ active users respect one
  global provider budget.

Worldwide release posture:

- **Downloads/updates:** serve from Cloudflare/R2-backed static files. This keeps
  installer and update latency low globally and avoids making the DigitalOcean
  app server a binary-download bottleneck.
- **Runtime alpha:** one DigitalOcean region. This is operationally simple and
  acceptable for early testers; LLM/STT provider time dominates most requests.
- **Runtime wider release:** add regional stateless gateways for live STT,
  screen/vision ingress, and answer streaming. Keep account/billing data in one
  primary database until we have enough traffic to justify multi-region data
  complexity.
- **Windows parity:** use the same server APIs, R2 artifacts, account model, and
  local SQLite/RAG design. Only packaging, capture helpers, and overlay
  invisibility/click-through behavior are platform-specific.

## Product Gaps To Prioritize

1. Clean-machine validation of the macOS arm64 tarball and installer script.
2. Runtime health view for overlay/audio/STT/provider/cloud status.
3. Long-session stress tests, sleep/wake, device hot-swap, and permission repair.
4. sqlite-vec or another ANN index for local RAG.
5. Production OCR/vision pipeline with citations and artifact status.
6. Auth, secure sync, managed provider routing, billing, and cloud RAG.
7. Customer settings/onboarding UI.
8. Windows whisper.cpp and Windows 10/11 hardware QA.
9. Signed installers and auto-update after the platform matrix is real.
10. Web dashboard for meeting history and admin.

Related production readiness docs:

- `deploy/PRODUCTION-CLOUD-ARCHITECTURE.md`
- `BACKEND-CONTRACTS.md`
- `SETTINGS-UI-CONTRACT.md`
- `INSTALLER-CHECKLIST.md`
- `PRODUCTION-READINESS.md`
