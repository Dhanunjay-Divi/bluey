# Production Readiness

This is the current source of truth for what Bluey can honestly ship, what is
implemented locally, and what remains before a paid cloud product launch.

## Release Scope

**v0.1.0 target:** macOS arm64, terminal-distributed, local-first Bluey with a
native overlay and bundled helper binaries.

Not shipped in v0.1.0:

- macOS x86_64, Linux, and Windows release artifacts.
- Signed/notarized GUI installer.
- Public managed Bluey cloud account system.
- Production billing/plans.

## Implemented And Verified
### Bluey Auto Router

- `crates/cue-router/`: task classifier + routing policy + speculative router
  ship as a standalone crate.
- Heuristic classifier covers general / code / system_design / meeting /
  writing / vision task types with confidence scoring.
- StaticPolicy maps lanes to providers (Instant -> OpenAI gpt-4o-mini,
  Balanced -> Anthropic claude-3-5-sonnet, Deep -> claude-3-7-sonnet,
  Vision -> gpt-4o, Local -> Ollama llama3.1).
- SpeculativeRouter optionally fires Instant + Deep in parallel; UI replaces
  draft with final via OverlayCommand::UpdateCard.
- Local-only mode forces all routing to the Local lane.
- 17 tests (heuristic + policy + speculative + follow-up) all passing.
- Tiny-model managed classifier slot is wired as a trait but no production
  endpoint exists yet; ships as heuristic-only.
- Daemon integration (replace direct LlmProvider calls with SpeculativeRouter)
  is the next round.



### Product Flow

- `bluey on` starts the daemon and native overlay.
- `bluey off` stops the daemon and overlay.
- Startup is pill-first: a compact Bluey pill opens first, and the larger feed
  appears on click/show/toggle.
- The overlay has a chronological feed, composer, answer/recap/analyse actions,
  attach and instructions entry points, opacity/state handling, and capture
  exclusion on macOS.
- The overlay IPC path has a per-session token, field length caps, and
  state-machine validation.

### Audio And STT

- Native macOS helper path exists for system audio and microphone capture.
- Native Windows helper source exists for WASAPI capture, but it is not shipped
  in v0.1.0 until Windows QA is complete.
- Two-stage VAD and audio framing are implemented.
- STT provider chain supports Deepgram Nova-3 WebSocket, OpenAI Realtime
  transcription protocol, LocalWhisper, mock, and echo providers.
- LocalWhisper is real on macOS through the SwiftWhisper/whisper.cpp helper.
- Windows whisper remains a stub and is not a supported release path.

### Answers And Context

- LLM streaming is implemented end-to-end for overlay cards.
- OpenAI, Anthropic, Ollama, OpenAI-compatible routing, and managed-route
  metadata are implemented in code.
- Recap/action-item/decision extraction exists for sessions.
- Provider context compacts transcript, recent Q&A, attachments, screenshots,
  notes, and local memory before model calls.
- Active page analysis attaches readable browser page text when available and
  can fall back to a single configured vision request.
- Attachments support local text/code/Markdown previews plus best-effort local
  PDF/Office extraction.

### Storage And Local Tools

- Local SQLite session storage, settings, FTS search, export, and session
  history are implemented.
- API keys/settings use OS keyring-backed storage where wired.
- Local RAG exists with chunking, embeddings, SQLite persistence, and Rust
  cosine search.
- The dashboard exists as a developer tool; it is not shipped in v0.1.0.

### Distribution

- `make package-darwin-arm64` builds the macOS arm64 terminal tarball with
  daemon, CLI, overlay helper, audio helper, and whisper helper.
- `scripts/install.sh` supports macOS arm64 archive installs, release download,
  checksum verification, and a versioned install layout under
  `~/.local/bluey/<version>`.
- Installed-path smoke passes locally: `bluey` can be launched through a
  symlink and still discover its helper binaries from the canonical install
  directory.

## Remaining Before Public Paid Launch

### P0 Product / Trust

- Clean-machine validation of the macOS arm64 install path.
- Privacy policy, data processing terms, deletion/export promises, and support
  process.
- Crash diagnostics/support bundle with explicit user consent.
- Runtime health view for audio/STT/provider/cloud status.

### P0 Cloud / Commercial

- Bluey account auth, refresh, logout, and device registration.
- Cloud sync queue with encrypted event/artifact upload and server ack.
- Managed provider router with Bluey-owned provider keys, budgets, rate limits,
  fallbacks, and cost metering.
- Cloud RAG with tenant-scoped retrieval, citations, retention, and deletion.
- Billing, plan limits, invoice/portal integration, and admin controls.

### P0 Platform

- Decide whether v0.1.0 GA stays macOS arm64-only.
- If not, build and smoke-test macOS x86_64, Linux, and Windows artifacts before
  claiming support.
- Windows whisper.cpp integration and Windows hardware QA.
- Windows overlay/audio/page-capture QA on clean Windows 10/11 machines.

### P1 Quality

- Replace local RAG's linear cosine scan with sqlite-vec or another ANN index.
- Add production-grade OCR/vision queueing, citations, thumbnails, and artifact
  processing status.
- Add richer attachment drawer and visible context management.
- Bundle/support the dashboard or keep it explicitly developer-only.
- Add long-session stress tests for audio, STT reconnects, LLM streaming, and
  overlay updates.

## Current Verdict

Bluey is a strong macOS arm64 local-first GA candidate for internal/early user
testing. It is not yet a production SaaS. The next release decision should be:

1. Clean-machine validate `scripts/install.sh` + `bluey on/off`.
2. Decide whether macOS arm64-only is acceptable for `v0.1.0`.
3. If yes, tag `v0.1.0` and start cloud/RAG/platform expansion.
4. If no, complete the platform matrix first and update every public claim.
