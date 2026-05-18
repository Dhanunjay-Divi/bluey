# Commercial Path Review

This review consolidates the remaining product path for Bluey as a commercial desktop-plus-cloud product.

## Product Boundary

`bluey on` is the only intended user-facing flow.

The user should launch Bluey, see the native overlay, attach context, set answer style, ask questions, control capture, and close the session from the overlay. Terminal commands still matter, but they are development, support, diagnostics, smoke-test, and automation surfaces. They should not become required steps in the commercial workflow.

Commercial product surfaces:

- Native desktop overlay launched by `bluey on`.
- Settings/onboarding UI for account, permissions, audio devices, hotkeys, workspace, privacy, retention, and plan.
- Web dashboard for meeting history, search, recaps, artifacts, exports, deletion, billing, and workspace administration.
- Support/diagnostic CLI for internal use.

Current CLI surfaces that should stay internal:

- `bluey run`, `bluey listen`, `bluey ask`, `bluey recap`, and meeting lifecycle commands.
- `bluey context ...`, `bluey instructions ...`, and `bluey memory search`.
- `bluey audio ...`, `bluey ai status`, `bluey cloud ...`, and `bluey providers`.

The overlay or settings UI should absorb these user-visible capabilities before paid release.

## APIs Needed

Bluey cloud needs a small set of production APIs before the desktop can move
from a local-first macOS product into a paid SaaS:

- Auth, refresh, logout, and device registration.
- Workspace membership, role, and policy lookup.
- Meeting/session creation and event append.
- Low-latency answer streaming over WebSocket or SSE.
- Artifact upload via signed URLs.
- Cloud sync for queued local events and server-side deltas.
- RAG query with citations and workspace filtering.
- Settings and permission state.
- Billing portal and plan enforcement.
- Export request, deletion request, and audit-log access.

The desktop should authenticate to Bluey cloud and call these APIs with Bluey-issued credentials. It should not ask normal customers to configure model provider keys.

## Storage Needed

Local storage should be a resilient cache, not the system of record:

- SQLite for recent sessions, settings, pending sync queue, and offline reads.
- Temporary artifact cache with cleanup policy.
- OS credential store for Bluey auth tokens.

Cloud storage should own commercial durability and governance:

- Postgres for users, workspaces, devices, meetings, events, permissions, billing state, retention policy, and audit records.
- Object storage for screenshots, documents, exports, and optional retained audio chunks.
- Vector storage for retrieval chunks and citations.
- Queue/worker state for STT, OCR, embeddings, recap extraction, retention, export, and deletion jobs.
- KMS or secrets manager for encryption material and provider credentials.

All storage reads must be workspace-scoped. Deletion and retention jobs need to remove metadata, objects, and vectors together.

## Provider Keys

Provider keys belong server-side in production managed mode.

The desktop should hold only Bluey auth tokens in the OS credential store. Bluey cloud should route requests to STT, vision, embedding, and LLM providers through a managed provider router with budgets, timeouts, fallbacks, and logging that excludes raw secrets.

Developer/local provider readiness commands can remain available, but they are not part of the paid customer setup path.

## Cloud And RAG Architecture

The commercial live path should look like this:

```text
desktop capture/context -> authenticated Bluey cloud stream -> STT/OCR/vision -> chunking/embeddings/RAG -> provider router -> streaming answer with citations -> overlay
```

The RAG service needs two latency profiles:

- Active-session retrieval for live answers, biased toward the current meeting, attached context, recent screen captures, and answer instructions.
- Workspace memory retrieval across prior meetings, recaps, decisions, action items, documents, and screenshots.

Every result should carry citation metadata that can point back to a meeting, transcript segment, artifact, or generated summary. Retention and deletion policy must apply before retrieval ranking, not after.

## UI Surfaces To Finish

Overlay:

- Inline composer for asking Bluey from inside the overlay. Implemented on macOS first.
- Model and answer-mode selection in the command bar. Implemented on macOS first as requested-route metadata.
- Attachment drawer showing selected files and captured context.
- Active audio, STT, provider, cloud sync, and capture health indicators.
- Streaming answer cards with markdown and citations.
- Global hotkeys for ask, show/hide, capture, attach, and stop. Ask/show composer hotkey is implemented on macOS first.

Settings/onboarding:

- Account login and workspace switcher.
- Microphone, system audio, and screen permission setup.
- Audio device selection and test.
- Privacy, retention, export, deletion, and capture defaults.
- Plan/billing status.

Web dashboard:

- Meeting history, search, recaps, action items, decisions, and artifacts.
- Workspace/admin controls.
- Export and deletion request status.
- Audit and billing surfaces for team plans.

## Remaining Gaps

The biggest commercial blockers are:

- Clean-machine macOS arm64 install validation and support diagnostics.
- Windows parity: real whisper.cpp, overlay/audio/page capture QA, and clean Windows packaging.
- Managed cloud auth/device registration and tenant-scoped sync.
- Managed provider answer streaming with Bluey-owned credentials, metering, budgets, and citations.
- Production vision/OCR extraction for screenshots and documents with artifact status and citations.
- Attachment drawer and richer visible session context management.
- Authenticated cloud sync and tenant-scoped RAG.
- Customer settings/onboarding UI and bundled web/dashboard surface.
- Signed installers, auto-update, crash diagnostics, telemetry opt-in, and support bundle collection.

Until these are implemented, Bluey should be described as a strong macOS arm64 local-first product candidate with commercial architecture scaffolding, not as a production SaaS.

For the current implementation matrix, use `docs/PRODUCTION-READINESS.md` as the source of truth.
