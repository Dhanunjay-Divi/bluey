# Competitive Gaps

This tracks what Bluey still needs to feel like a paid product rather than a strong prototype.

## Core Product Gaps

- Cloud auth: account creation, login, session refresh, device management.
- Secure cloud sync: encrypted meeting/event/artifact upload and download.
- Cloud RAG: vector search, chunking, citations, workspace filtering, retention-aware deletion.
- Meeting history dashboard: searchable meetings, recaps, attachments, action items, decisions.
- Managed AI routing: model/provider router, fallback, latency budget, retry policy, cost tracking.
- Streaming answer cards: partial answer updates instead of one-shot local cards.
- Real vision/OCR: screenshot and document understanding with source citations.
- Real audio/STT: system audio plus microphone, VAD, speaker/source labeling, restartable capture.
- Personas/modes: sales call, engineering design review, standup, customer support, interview prep for ethical personal practice, etc.
- Billing and plans: subscription tiers, limits, trial, invoices.
- Settings app: account, capture permissions, providers, workspace, retention, shortcuts, privacy.
- Auto-update and signing: packaged macOS/Windows installers, codesigning, notarization.
- Observability: crash reporting, local diagnostics, user-facing health status.
- Enterprise controls: audit log, SSO/SAML later, retention policy, data export/delete, workspace admin.

Implemented or scaffolded but not fully production yet:

- Audio/STT has core data models, status commands, native macOS ScreenCaptureKit/CoreAudio chunks, native Windows WASAPI chunks, and a simulated dev runtime that emits labeled transcript segments.
- Managed AI routing has core data models, status commands, requested route metadata, offline fallback, and OpenAI-compatible HTTP calls for configured OpenAI/Groq/Cerebras routes.
- Secure cloud/RAG has core data models and `bluey cloud ...` status commands.
- Backend/deployment has OpenAPI, migration, worker queue, settings, and installer skeletons.

## UX Gaps

- Inline overlay input box is implemented on macOS; Windows parity, markdown streaming, citations, and answer history still need product polish.
- Global hotkeys for show/hide, capture, ask, and attach.
- Onboarding checklist for permissions and first session.
- Clear active capture indicator across overlay and status output.
- Attachment drawer in overlay to show selected session files.
- Answer style presets in overlay.

## Platform Gaps

- Compile and QA Windows overlay on Windows.
- Windows file picker/instructions dialog parity.
- Windows screenshot capture/watch parity.
- Windows native audio hardware QA.
- Audio VAD, partial transcripts, reconnects, and STT provider fallback.

## Things We Should Not Build

- Proctoring or monitoring bypass.
- Process disguise intended to deceive admin/proctor/security tooling.
- Hidden capture without visible consent and user control.
