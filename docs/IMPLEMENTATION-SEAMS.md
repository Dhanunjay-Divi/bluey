# Implementation Seams

> Historical implementation baseline. Its `0.1.0` platform statements are not
> current support claims. Use the signed live manifest and `INSTALL.md` for
> downloadable-platform facts.

This is the map for the next backend hardening work. The current build exposes
the product surfaces and now has native macOS audio, two-stage VAD, streaming
STT provider routing, streaming LLM cards, local RAG primitives, and terminal
packaging for macOS arm64. Windows helper source exists, but Windows is not a
supported v0.1.0 release path until whisper.cpp and hardware QA are complete.

## Audio

Current code:

- `crates/cue-core/src/audio.rs`
- daemon requests: `audio_status`, `audio_start`, `audio_stop`
- CLI commands: `bluey audio status`, `bluey audio start`, `bluey audio stop`
- runtime: `crates/cue-daemon/src/app.rs` prefers bundled native audio helpers,
  applies VAD/framing, and routes audio through Deepgram Nova-3, OpenAI
  Realtime transcription, LocalWhisper, mock, or echo providers depending on
  configuration and platform. FFmpeg remains a fallback/dev path.

Next hardening:

1. Long-session stress, sleep/wake, and device hot-swap validation.
2. Permission repair UI and richer runtime health diagnostics.
3. Windows system/mic audio: QA the native WASAPI helper on hardware.
4. Windows real whisper.cpp integration.

The model keeps system audio and microphone as separate sources so answers can distinguish what was said by the meeting and what was said by the Bluey user.

## AI Answers

Current code:

- `crates/cue-core/src/ai.rs`
- daemon request: `ai_status`
- CLI command: `bluey ai status`
- `AnswerRequest` now carries a real `ProviderRoute`; the daemon resolves each route step against environment-backed provider config.
- `ProviderRequestPayload` is the offline adapter contract for future HTTP clients. It includes request id, provider/model, endpoint, context, streaming flag, safety/privacy flags, and budget metadata without exposing secret values.
- Remote and local providers stream chunks into the overlay response-card path;
  deterministic local fallback is only used when the requested route includes a
  local/dev fallback.

Next implementation:

1. Add provider-level cancellation for superseded requests.
2. Attach citations and ranked RAG hits to streamed response metadata.
3. Store answer metadata, citations, safety notices, latency, and cost estimates.
4. Add plan/cost/budget enforcement behind managed Bluey routing.

Managed production mode should use Bluey cloud credentials. Local provider keys are only useful for development.

## Cloud And RAG

Current code:

- `crates/cue-core/src/cloud.rs`
- daemon requests: `cloud_status`, `cloud_sync_now`
- CLI commands: `bluey cloud status`, `bluey cloud sync`

Next implementation:

1. Add authenticated login/device session handling.
2. Upload meeting events and artifacts through a secure sync client.
3. Chunk transcripts, recaps, decisions, action items, instructions, documents, screenshots, and OCR/vision summaries.
4. Embed chunks server-side and index them by workspace.
5. Retrieve tenant-scoped `RagResult` items before answer generation.
6. Enforce retention, export, and deletion policies across metadata, objects, and vectors.

## Overlay UX

Current code:

- user-visible controls for ask, recap, active-page capture, attach/show-attached, answer instructions, opacity, hide, clear, and close.

Next implementation:

1. Add an attachment drawer and richer context management.
2. Add active audio/cloud health indicators.
3. Add provider/source badges on answer cards.
4. Add more clean-machine QA around click-through scrolling, resizing, and hide/show.
5. Expand Windows overlay parity once Windows becomes a supported target.

## Product Boundary

Bluey should keep consent and user control visible:

- no hidden capture
- no deceptive process disguise
- no proctoring or monitoring bypass
- no silent provider uploads

The commercial direction is still managed cloud, secure storage, workspace-scoped RAG, billing, and enterprise controls.
