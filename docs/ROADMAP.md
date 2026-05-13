# Roadmap

## Version 0.1: Private Meeting Loop

- Native overlay sidecar.
- Capture-excluded overlay on macOS and Windows.
- Movable, resizable, opacity-adjustable overlay controls.
- CLI/daemon process model.
- Durable active meeting state.
- Manual transcript ingestion.
- User-selected screenshot/diagram/code/document context attachments.
- macOS user-triggered screenshot capture with preview and confirmation.
- Explicit support-only periodic screen context capture, separate from the primary user-confirmed Analyse Screen action.
- Deterministic question/action/decision cards.
- Audio, AI routing, and cloud/RAG runtime status scaffolds.
- Recap and action-item commands.

## Version 0.2: Audio and STT

- macOS system audio via ScreenCaptureKit.
- macOS microphone capture via CoreAudio/AVFoundation.
- Windows system audio via native WASAPI loopback.
- Windows microphone capture via native WASAPI.
- VAD before STT.
- Streaming STT provider trait.
- OpenAI/Deepgram/local adapters behind the same interface.
- Feed final STT segments into the existing transcript engine.

## Version 0.3: Intelligence

- LLM provider abstraction.
- Vision provider abstraction for user-selected screenshots, diagrams, and code context.
- Provider routing, fallback, and key rotation.
- Streaming answer cards.
- Managed Bluey cloud routing as the production default.
- Rolling context compaction.
- Meeting-mode detection.
- Confidence and cooldown logic so Bluey does not spam cards.

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
