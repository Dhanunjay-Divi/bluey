# Deployment And Scaling Plan

Bluey should ship as a desktop product backed by a managed cloud service. The CLI remains a developer/debug surface.

## User-Facing Surfaces

- Desktop overlay: daily use, launched by `bluey on`.
- Desktop settings window: login, permissions, audio devices, hotkeys, privacy, workspace, plan.
- Web dashboard: meeting history, search, recaps, action items, attached files, admin controls.
- CLI: internal diagnostics, smoke tests, support workflows.

The commercial promise should be that normal users only need `bluey on` or the packaged app launcher. CLI commands are still valuable for development and support, but they should not appear in onboarding as required customer steps.

## Desktop Packaging

macOS:

- Bundle `bluey`, `bluey-daemon`, and `bluey-overlay-macos` into a signed `.app`.
- Notarize the app.
- Request microphone and screen-recording permissions through visible onboarding.
- Store local config in the per-user app config directory.
- Use launch agent or login item only after explicit user opt-in.

Windows:

- Package `bluey.exe`, `bluey-daemon.exe`, and `bluey-overlay.exe` with an installer.
- Code sign binaries and installer.
- Use WASAPI loopback for system audio and standard microphone permissions.
- Add Windows notification/tray/settings entry.
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

## Product Gaps To Prioritize

1. Inline overlay composer instead of native ask popup.
2. Global hotkeys for ask, show/hide, capture, attach.
3. Real macOS and Windows audio capture.
4. Streaming STT with system/mic source labels.
5. Streaming provider answer cards with markdown and citations.
6. Vision/OCR for screen captures and attached images/documents.
7. Auth, secure sync, and cloud RAG.
8. Settings/onboarding UI.
9. Signed installers and auto-update.
10. Web dashboard for meeting history and admin.

Related production readiness docs:

- `BACKEND-CONTRACTS.md`
- `SETTINGS-UI-CONTRACT.md`
- `INSTALLER-CHECKLIST.md`
