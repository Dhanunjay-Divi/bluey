# Production Readiness

This is the current source of truth for what Bluey can honestly ship, what is
implemented locally, and what remains before a paid cloud product launch.

## Release Scope

**v0.1.0 target:** macOS arm64, terminal-distributed, local-first Bluey with a
native overlay and bundled helper binaries. **Tagged on `c34592a` 2026-05-19.**

Shipped artifacts:

- `dist/bluey-0.1.0-darwin-arm64.tar.gz` (6.3 MB; arm64-only).
- `dist/bluey-0.1.0-darwin-universal.tar.gz` (12 MB; arm64+x86_64 via lipo;
  the Intel slice is link-tested only on uno and needs clean-Intel-Mac
  validation before being recommended as the primary download).

Not shipped in v0.1.0:

- Linux and Windows release artifacts.
- Signed/notarized installer.
- Public Bluey distribution endpoint (R14.8 architecture chosen, server
  not yet stood up).
- Public managed Bluey cloud account system / managed Auto Router endpoint
  (R14.9, parked until monetization is greenlit).
- Production billing/plans.

## Implemented And Verified
### Bluey Auto Router

- `crates/cue-router/`: task classifier + routing policy + speculative router
  ship as a standalone crate. **26 tests passing.**
- HeuristicClassifier covers general / code / system_design / meeting /
  writing / vision task types with confidence scoring; vision keywords
  narrowed so text-only "diagram" / "chart" do not misroute design
  questions to Vision.
- StaticPolicy maps developer lanes to providers (Instant -> OpenAI
  gpt-4o-mini, Balanced -> Anthropic claude-3-5-sonnet, Deep ->
  claude-3-7-sonnet, Vision -> gpt-4o). Direct providers and Ollama require
  explicit dev flags in debug/dev builds and are not customer modes. Release
  binaries ignore those flags. Vision overrides latency.
- ManagedPolicy maps paid customer traffic to `bluey-managed-*` lanes. It never
  emits a Local lane; local/Ollama fallback is daemon-only and is not exposed as
  a paid customer model.
- AutoRouter coordinator takes ClassifierInput + RouteOptions { local_only }
  and returns a RoutedRequest. `local_only` is for developer/offline daemon
  fallback, not managed cloud dispatch.
- SpeculativeRouter honors ProviderRoute.stream and survives Instant-lane
  unavailability by emitting a non-fatal Error chunk and continuing Deep.
- **Daemon wiring shipped**: `request_cue` classifies every prompt and
  emits RouterMeta on the first cue_response_chunk. Speculative dispatch
  fires through SpeculativeRouter when `BLUEY_SPECULATIVE_ROUTING` is
  unset (default ON) or set to a truthy value; explicitly disable with
  `=0/false/off`. **`auto_recap` is NOT yet routed through the
  classifier** — it emits `router_meta: None` and uses the default
  RecapLlm provider directly. Routing recap is queued as a v0.2-class
  follow-up because recap input/output shapes (whole transcripts in,
  structured markdown out) need a separate classifier branch.

Speculative-routing comments in `crates/cue-dashboard/src/commands.rs`
and `crates/cue-router/src/speculative.rs` accurately describe the
current default-ON behaviour as of round 14
were updated alongside this change to match the default-ON behaviour.
- ProviderRegistry builds Arc<dyn LlmProvider> for every configured
  provider (OpenAI, Anthropic, Ollama) and impls SpeculativeProvider with
  per-route lookup + any-other-available fallback.
- Dashboard UI shipped: LaneBadge component renders above each in-flight
  card showing latency lane (color), task type, provider/model, confidence,
  and a REFINED tag when the deep lane has replaced the draft. Daemon
  emits replace_body=true on the deep Final chunk for clean draft -> final
  swap (no [refined] tag prefix hack).
- Tiny-model managed classifier slot is wired as a trait but no production
  endpoint exists yet (R14.9 Bluey product server).



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
- LocalWhisper is hidden reliability/dev fallback only. Paid cloud sessions
  should prefer Deepgram/OpenAI STT failover; do not advertise LocalWhisper as a
  customer-facing model choice until quality and support posture are validated.
- Windows whisper remains a stub and is not a supported release path.

### Answers And Context

- LLM streaming is implemented end-to-end for overlay cards.
- OpenAI, Anthropic, OpenAI-compatible routing, developer-gated Ollama, and
  managed-route metadata are implemented in code.
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

## Architecture and Roadmap Cross-References

- `docs/BLUEY-ARCHITECTURE.md` is the source of truth for the 3-layer
  architecture (local client / distribution server / product server) and
  the staged Stage 0-5 monetization rollout.
- `docs/AUTO-ROUTING-USP.md` covers the Auto Router product framing.
- `docs/BLUEY-DISTRIBUTION-ARCHITECTURE.md` covers the v0.1 distribution
  server choice (paths A / B / C, pending user pick).
- `docs/rounds/PHASE-3-ROUND-14-PLAN.md` tracks current-round work items
  including R14.8 (distribution server) and R14.9 (product server scaffold).

## Remaining Before Public Paid Launch

### P0 Product / Trust

- Clean-machine validation of the macOS arm64 install path.
- Privacy policy, data processing terms, deletion/export promises, and support
  process.
- Crash diagnostics/support bundle with explicit user consent.
- Runtime health view for audio/STT/provider/cloud status.

### P0 Cloud / Commercial (REQUIRED for v0.2 — no-BYOK decision 2026-05-19)

The 2026-05-19 no-BYOK decision (see `DECISIONS.md`) makes every item
in this section a hard blocker for v0.2 launch. v0.1 BYOK is dev-mode
only.

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
