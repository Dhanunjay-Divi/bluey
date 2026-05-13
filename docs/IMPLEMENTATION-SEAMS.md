# Implementation Seams

This is the map for the next backend hardening work. The current build exposes the product surfaces and now has native macOS and Windows audio/STT paths, while partial streaming, provider failover, Windows hardware QA, and cloud services still need production work.

## Audio

Current code:

- `crates/cue-core/src/audio.rs`
- daemon requests: `audio_status`, `audio_start`, `audio_stop`
- CLI commands: `bluey audio status`, `bluey audio start`, `bluey audio stop`
- runtime: `crates/cue-daemon/src/app.rs` prefers bundled native audio helpers plus an OpenAI-compatible transcription endpoint when `OPENAI_API_KEY` or `BLUEY_STT_API_KEY` is configured, and otherwise uses the development simulator. FFmpeg remains a fallback/dev path.

Next hardening:

1. VAD and silence suppression before STT to reduce cost/noise.
2. Partial transcript streaming, reconnects, and provider failover.
3. Windows system/mic audio: QA the native WASAPI helper on hardware.
4. Device setup UI for microphone/system source selection and permission repair.

The model keeps system audio and microphone as separate sources so answers can distinguish what was said by the meeting and what was said by the Bluey user.

## AI Answers

Current code:

- `crates/cue-core/src/ai.rs`
- daemon request: `ai_status`
- CLI command: `bluey ai status`
- `AnswerRequest` now carries a real `ProviderRoute`; the daemon resolves each route step against environment-backed provider config.
- `ProviderRequestPayload` is the offline adapter contract for future HTTP clients. It includes request id, provider/model, endpoint, context, streaming flag, safety/privacy flags, and budget metadata without exposing secret values.
- Remote providers return unavailable errors when credentials, endpoints, or linked HTTP adapters are missing. The deterministic local answer is only used when the requested route includes `local`.

Next implementation:

1. Link provider-specific HTTP clients to the `ProviderRequestPayload` contract while keeping tests offline.
2. Add streaming chunk adapters that map provider deltas into `AnswerStreamEvent::Delta`.
3. Retrieve RAG hits before answer generation and attach citations to response metadata.
4. Store answer metadata, citations, safety notices, latency, and cost estimates.

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

1. Replace the temporary ask popup with an inline overlay composer.
2. Add an attachment drawer.
3. Add active audio/cloud health indicators.
4. Add streaming answer rendering.
5. Add global hotkeys for show/hide, ask, capture, and attach.

## Product Boundary

Bluey should keep consent and user control visible:

- no hidden capture
- no deceptive process disguise
- no proctoring or monitoring bypass
- no silent provider uploads

The commercial direction is still managed cloud, secure storage, workspace-scoped RAG, billing, and enterprise controls.
