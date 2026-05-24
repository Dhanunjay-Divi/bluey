# Deployment And Scaling Plan

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
- Temporary capture directory with cleanup.
- Encrypted local token storage through Keychain on macOS and Credential Manager on Windows.

Cloud:

- Postgres for users, workspaces, meetings, transcript/event metadata, permissions, billing state, device sessions, audit logs.
- Object storage for screenshots, documents, exports, and optional retained audio chunks.
- Vector index for RAG chunks. `pgvector` is enough for early scale; dedicated vector infra can come later.
- Redis or queue service for background jobs.
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
- STT workers: autoscale by active audio sessions.
- Vision/OCR workers: queue-based, burstable.
- Embedding workers: queue-based, retryable, idempotent.
- RAG query service: low-latency cache for active meetings and workspace memory.
- Artifact service: direct-to-object-storage uploads with short-lived signed URLs.

Reliability:

- Keep the desktop usable if cloud is degraded.
- Queue sync locally and retry with backoff.
- Never block overlay rendering on cloud sync, embeddings, recap, or billing checks.
- Stream partial answer cards, then finalize with citations.
- Track health for audio, STT, provider routing, cloud sync, and capture permissions.
- Enforce three layers of capacity before provider dispatch:
  - Per-IP edge buckets for abuse protection.
  - High-ceiling per-account guardrails only for runaway loops or stolen tokens;
    paid usage itself is controlled by wallet balance and provider availability.
  - Per-provider/model buckets for OpenAI, Anthropic, Deepgram, and embeddings.
- Return typed `429` responses with `retry_after_secs` when Bluey is busy, and
  route LLM lanes through fallback candidates before surfacing an outage.
- For multi-instance deployment, move the capacity buckets from in-process
  governor state to Redis/shared counters so 1000+ active users respect one
  global provider budget.

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

- `BACKEND-CONTRACTS.md`
- `SETTINGS-UI-CONTRACT.md`
- `INSTALLER-CHECKLIST.md`
- `PRODUCTION-READINESS.md`
