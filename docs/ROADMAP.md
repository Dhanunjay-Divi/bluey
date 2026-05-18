# Roadmap

## Version 0.1: macOS Arm64 Local-First Overlay

- Native overlay sidecar.
- Capture-excluded overlay on macOS.
- Compact pill-first launch, movable/resizable panel, click-through readable feed,
  and opacity-adjustable background glass.
- CLI/daemon process model.
- Durable active session state.
- Manual transcript ingestion.
- User-selected screenshot/diagram/code/document context attachments.
- macOS user-triggered screenshot capture with preview and confirmation.
- Explicit support-only periodic screen context capture, separate from the primary user-confirmed Analyse Screen action.
- Deterministic question/action/decision cards.
- Native macOS audio helper path, two-stage VAD, Deepgram/OpenAI Realtime/LocalWhisper-capable STT routing, and source-labeled transcript storage.
- Streaming LLM answer cards with compacted transcript/context/attachment memory.
- Local SQLite session storage, FTS search, export, local RAG primitives, and OS-keyring-backed settings where wired.
- Terminal tarball package and installer script for macOS arm64.
- Recap and action-item commands.

## Version 0.2: Reliability And Local RAG

- Clean-machine macOS arm64 install validation.
- Long-session stress tests for overlay, audio, STT reconnects, and answer streaming.
- Device hot-swap, sleep/wake, permission repair, and health diagnostics.
- sqlite-vec or another ANN index for local RAG.
- Attachment drawer and visible context management.
- Provider cancellation/abort semantics for superseded answer requests.

## Version 0.3: Platform Expansion

- macOS x86_64 artifact if Intel support is required.
- Windows whisper.cpp integration and Windows 10/11 QA for overlay, audio,
  page capture, installer, and update paths.
- Linux build decision after audio/capture feasibility review.
- Signed installers and auto-update only after the supported platform matrix is real.

## Version 0.4: Commercial Cloud Memory

- Authenticated Bluey cloud account.
- Secure cloud sync for meetings, artifacts, and recaps.
- Cloud RAG index with tenant/workspace scoped retrieval.
- Background recap and memory extraction.
- Meeting history dashboard.
- Export and deletion jobs backed by the cloud policy model.
- Retention, export, and deletion controls.
- Billing and plan enforcement.

## Reliability Principles

- The live meeting path must never block on recap, storage compaction, embeddings, or network retries.
- Overlay failure should not crash the daemon.
- Audio capture should be restartable independently from the meeting engine.
- Provider errors should degrade to local cards/status, not silence.
- State should be recoverable after daemon restart.
