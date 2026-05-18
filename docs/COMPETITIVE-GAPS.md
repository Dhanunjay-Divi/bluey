# Competitive Gaps

This tracks what Bluey still needs to feel like a paid product beyond the
current macOS arm64 local-first release.

## Core Product Gaps

- Cloud auth: account creation, login, session refresh, device management.
- Secure cloud sync: encrypted meeting/event/artifact upload and download.
- Cloud RAG: vector search, chunking, citations, workspace filtering, retention-aware deletion.
- Customer-facing meeting history dashboard: searchable meetings, recaps, attachments, action items, decisions. Local session search/export exists; the dashboard is still a developer tool in v0.1.0.
- Managed AI routing: server-side provider router, fallback, latency budget, retry policy, and cost tracking. Local/provider-side streaming is implemented, but customers should not need provider keys in the paid product.
- Production vision/OCR: screenshot and document understanding with source citations, thumbnails, processing status, and cloud parser fallback. Initial page/screenshot vision path exists.
- Production audio/STT reliability: native capture, VAD, streaming STT, and fallback routing exist on macOS, but long-session stress, device hot-swap, clean-machine QA, and Windows parity remain.
- Personas/modes: sales call, engineering design review, standup, customer support, interview prep for ethical personal practice, etc.
- Billing and plans: subscription tiers, limits, trial, invoices.
- Customer settings app: account, capture permissions, providers, workspace, retention, shortcuts, privacy. Terminal settings and developer dashboard exist.
- Auto-update and signing: packaged macOS/Windows installers, codesigning, notarization. v0.1.0 is terminal-only macOS arm64 tarball + installer script.
- Observability: crash reporting, local diagnostics, user-facing health status.
- Enterprise controls: audit log, SSO/SAML later, retention policy, data export/delete, workspace admin.

Implemented or scaffolded but not fully production yet:

- Audio/STT has native macOS capture, two-stage VAD, Deepgram/OpenAI Realtime/LocalWhisper providers, STT fallback routing, native Windows WASAPI source, and dev-only mock/echo paths for no-key testing.
- Managed AI routing has core data models, requested route metadata, local fallback, streaming provider adapters, and OpenAI/Groq/Cerebras/Anthropic/Ollama-compatible paths where configured.
- Secure cloud/RAG has core data models, local RAG, OpenAPI, Postgres schema outline, worker queues, and `bluey cloud ...` status commands.
- Backend/deployment has OpenAPI, migration, worker queue, settings, and installer skeletons.

## UX Gaps

- Inline overlay input box is implemented on macOS; Windows parity, markdown polish, citations, and richer answer history still need product polish.
- Global hotkeys for show/hide, capture, ask, and attach. Some hotkey wiring exists; full cross-platform UX/QA remains.
- Onboarding checklist for permissions and first session.
- Clear active capture indicator across overlay and status output.
- Attachment drawer in overlay to show selected session files.
- Rich answer style presets in overlay.

## Platform Gaps

- Compile and QA Windows overlay on Windows.
- Windows file picker/instructions dialog parity.
- Windows screenshot capture/watch parity.
- Windows native audio hardware QA.
- Windows real whisper.cpp.

See `docs/PRODUCTION-READINESS.md` for the current source-of-truth matrix.

## Things We Should Not Build

- Proctoring or monitoring bypass.
- Process disguise intended to deceive admin/proctor/security tooling.
- Hidden capture without visible consent and user control.
