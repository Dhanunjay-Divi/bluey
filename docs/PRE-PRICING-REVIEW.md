# Pre-Pricing Product Review

> Historical pre-pricing snapshot. Platform statements below predate the
> current `0.1.99` manifest, which lists macOS Apple silicon and Windows x86-64.
> This document is not a current readiness or pricing approval.

This is the working checklist before we discuss pricing plans. The goal is to know what Bluey can honestly sell, what is still a prototype seam, and what reference-app capabilities remain missing.

## Current Working Model

- `bluey on` is the intended customer entrypoint.
- macOS native overlay is movable, resizable, opacity-adjustable, capture-excluded, and frame-persistent.
- Overlay command bar has ask input, model route picker, answer mode picker, attach/show-attached, answer rules, clear, hide, and quit, with Answer, Recap, and Analyse Screen grouped in the bottom composer.
- Overlay ask events carry requested provider, model, and answer mode into the daemon.
- Bluey Auto routes through managed/provider options when configured and falls back locally when offline.
- OpenAI-compatible HTTP chat calls are wired for OpenAI, Groq, Cerebras, and optional Bluey-managed-compatible endpoints when keys/endpoints exist.
- Attached text/code/Markdown files get bounded local previews so answers can use actual file content.
- PDF and Word/RTF attachments attempt real text extraction and are rejected if Bluey cannot read them yet, keeping session context honest.
- Permissioned screenshot capture attaches screenshots as context through terminal/support flows, while the primary overlay Analyse Screen action attaches readable active-page text and generates an answer. If page text is unavailable, Analyse Screen can fall back to a single screenshot routed through a configured OpenAI-compatible vision provider.
- Real chunked audio/STT is wired through bundled native helpers, VAD, Deepgram/OpenAI Realtime/LocalWhisper-capable routing, and the same source-labeled meeting path. macOS arm64 is the v0.1.0 release target; Windows helper source exists but needs real Windows whisper.cpp and hardware QA before support is claimed. FFmpeg remains a fallback/dev path.
- Meeting engine detects questions, action items, decisions, recaps, and memory hits.
- Cloud/RAG, backend APIs, workers, storage schema, settings contract, and installer checklist are documented under `docs/` and `infra/`.

## Reference-App Gaps Still Missing

- Long-session audio/STT hardening: device hot-swap, sleep/wake, reconnect stress, and production health UI.
- Windows audio hardware QA for the native WASAPI helper.
- Windows real whisper.cpp.
- Rich OCR/vision for screenshots and diagrams with citations, thumbnails, multi-screenshot queueing, and cloud status. The first provider-backed screenshot fallback is wired for Analyse Screen.
- Production-grade cloud document parsing for PDFs and office files where local extraction is unavailable or too weak.
- Rich attachment drawer showing selected files, captured screenshots, page captures, upload status, and processing status.
- Meeting/chat history dashboard with search, recaps, decisions, action items, and artifacts.
- Settings/onboarding UI for account, permissions, audio devices, model policy, hotkeys, retention, export, and deletion.
- Global hotkeys for show/hide, ask, attach, capture, recap, and quick actions.
- Windows overlay parity for command bar, model/mode selection, file picker, answer rules, capture controls, recap, and close confirmation.
- Signed macOS and Windows installers, auto-update, crash diagnostics, and support bundle export.
- Production cloud auth, device registration, sync queue, artifact upload, RAG indexing, deletion/export propagation, billing, and admin controls.

## Product Differentiators To Keep

- One-command start with `bluey on`.
- Native lightweight overlay, not Electron-first.
- Consent-based capture and visible user controls.
- Commercial managed provider routing so customers do not need model keys.
- Secure cloud memory and RAG as the paid moat.
- Fast answer modes for Code, System Design, Meeting, Writing, and General workflows.
- Offline local fallback for reliability during demos and poor connectivity.

## Things We Should Not Build

- Proctoring or monitoring bypass.
- Process disguise meant to deceive admins, proctors, or security tooling.
- Hidden capture without visible consent and user control.
- Product copy aimed at cheating in exams, interviews, or monitored assessments.

## Before Pricing

Do not finalize pricing until these are at least partially real:

- Authenticated account/device flow.
- Cloud sync for meetings and artifacts.
- At least one real managed LLM route behind Bluey-controlled credentials.
- Real STT route for microphone plus system audio with production-grade reliability, billing meters, VAD, and device setup.
- Basic dashboard or settings UI.
- Plan enforcement points for answer requests, audio minutes, artifact storage, RAG queries, and retention.

For the current implementation/missing matrix, see `docs/PRODUCTION-READINESS.md`.

Pricing can then map cleanly to:

- Audio/STT minutes.
- Answer requests or included fast-token budget.
- Screenshot/document processing volume.
- Cloud storage and retention.
- RAG memory size.
- Team/workspace controls.
- Premium low-latency model routes.
