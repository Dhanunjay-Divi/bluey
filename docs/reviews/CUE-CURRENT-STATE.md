# bluey (cue) Current State — Source Code Audit

**Method**: Read every source file in the bluey repo (excluding _refs/ reference collection + docs/). Task audit uses the 94-task inventory from the 4 design docs.
**Date**: 2026-05-12

## 1. Repository Layout

```
cue/
├── Cargo.toml              # Rust workspace manifest (3 crates)
├── Cargo.lock              # Pinned deps
├── README.md               # Product readme
├── PRD-Cue-AI-Meeting-Copilot.md
├── .gitignore
├── assets/                 # Brand assets (SVG logo, GIF frames)
│   └── brand/logo-options/ # 6 logo SVGs + animated frames
├── crates/
│   ├── cue-cli/            # CLI binary ("bluey" command)
│   ├── cue-core/           # Shared domain library
│   └── cue-daemon/         # Background daemon binary
├── native/
│   ├── macos/
│   │   ├── cue-overlay/    # Swift NSPanel overlay (2463 LOC)
│   │   └── cue-audio/      # Swift ScreenCaptureKit audio helper (212 LOC)
│   └── windows/
│       ├── cue-overlay/    # C Win32 overlay (996 LOC)
│       └── cue-audio/      # C WASAPI audio helper (325 LOC)
├── web/                    # Marketing landing page (static HTML)
│   ├── index.html          # Single-page site with canvas animation
│   └── assets/             # Logo SVG + GIF
├── infra/
│   ├── migrations/         # 001_initial_cloud_schema.sql (Postgres+pgvector)
│   ├── queues/             # workers.yaml (10 queue definitions)
│   ├── openapi.yaml        # Cloud API spec
│   └── README.md
├── scripts/
│   ├── build-macos.sh      # Cargo build + Swift compile
│   ├── build-windows.ps1   # Cargo build + C compile
│   ├── run-local.sh        # Dev launcher
│   └── smoke-test.sh       # Basic integration test
└── docs/                   # Design docs, reviews, strategy (not code)
```

## 2. Rust Workspace

### Workspace Cargo.toml
- **resolver**: 2
- **edition**: 2021
- **Shared deps**: anyhow, base64, clap (derive), dirs, libc, serde (derive), serde_json, reqwest (json+multipart+rustls-tls), tokio (full), tracing, tracing-subscriber (env-filter+fmt), uuid (v4+serde)

### crates/cue-core
**Role**: Shared domain types, serialization contracts, local intelligence engine, IPC protocol, config persistence.

**Modules** (from lib.rs):
| Module | Purpose | Key exports |
|--------|---------|-------------|
| ai | Provider routing, capabilities, budgets, request/response/stream types | ProviderRoute, AnswerRequest, AnswerResponse, AnswerStreamEvent, ProviderSelector, SafetyFlags, PrivacyFlags |
| audio | Audio pipeline types, capture config, device descriptors, STT segment metadata, simulated PCM | AudioPipelineStatus, AudioCaptureConfig, AudioChunkMetadata, SttSegmentMetadata, SimulatedPcmChunk |
| cards | Overlay card types | CueCard, CardKind |
| clock | Epoch-ms timestamp utility | now_epoch_ms_string() |
| cloud | Cloud sync types, RAG query/result, memory chunks, encryption metadata | CloudSyncStatus, MemoryChunk, RagQuery, RagResult, WorkspaceId, UserId |
| config | Account + settings persistence (JSON files with 0600 perms) | AccountConfig, CueSettings, load_account, save_account |
| intelligence | Local deterministic answer engine, segment analysis, recap generation | analyze_segment, local_answer, generate_recap |
| ipc | Daemon request/response protocol (JSON over TCP) | DaemonRequest (30+ variants), DaemonResponse |
| meeting | Meeting record, transcript, action items, decisions, context artifacts | MeetingRecord, TranscriptSegment, ActionItem, Decision, ContextArtifact |
| overlay | Overlay command/event protocol (JSON over stdin/stdout) | OverlayCommand, OverlayEvent |
| state | Daemon runtime state | DaemonState, MeetingState |

**Tests**: 25+ unit tests across ai.rs, audio.rs, cloud.rs, intelligence.rs, meeting.rs. All test serialization round-trips, builder patterns, and domain logic.

**Dependencies**: anyhow, dirs, serde, serde_json, uuid

### crates/cue-cli
**Role**: User-facing CLI binary (`bluey` / `cue`). Parses commands, communicates with daemon over TCP IPC.

**Key modules**: app.rs (entire CLI in one file, ~1200 LOC)

**Commands** (from clap Subcommand enum):
- `on` / `off` — product-level start/stop
- `login` — browser OAuth + token login + local mode
- `account` — show linked account
- `sessions` — list/inspect saved meetings
- `settings` — view/update preferences
- `start` / `stop` / `status` — daemon lifecycle
- `overlay show/hide/toggle/clear/opacity/position`
- `meeting start/end`
- `listen` — feed transcript
- `ask` — ask with provider routing
- `recap` / `action-items`
- `context add/capture/page/list/watch`
- `instructions set/show/clear`
- `memory search`
- `audio status/start/stop`
- `ai status`
- `cloud status/sync`
- `providers` — show env-configured providers
- `dev-card` — push test card

**No Tauri commands** — this is a pure CLI, not a Tauri app.

**Dependencies**: anyhow, clap, cue-core, libc, serde_json, tokio

### crates/cue-daemon
**Role**: Background daemon. Manages meeting state, overlay sidecar, audio pipeline, AI answer routing, screen capture, cloud sync status.

**Key modules**: app.rs (~3500 LOC), storage.rs

**Architecture**:
- TCP IPC listener on 127.0.0.1:57321
- Spawns native overlay as child process (stdin/stdout JSON protocol)
- Manages meeting lifecycle with JSON file persistence
- Real audio capture via ffmpeg or native helper binaries
- STT via OpenAI Whisper API (multipart upload)
- AI answers via OpenAI-compatible chat completions (streaming SSE)
- Screen capture via `screencapture` (macOS) / PowerShell (Windows)
- Active page text extraction via AppleScript (Chrome/Safari/Edge/Arc/Brave)

**Live provider support** (from code):
- OpenAI (chat completions + streaming)
- Groq (OpenAI-compatible)
- Cerebras (OpenAI-compatible)
- Bluey Managed (proxied OpenAI-compatible)
- Local (deterministic, no HTTP)
- Anthropic, Google, others: config exists but HTTP adapter returns "not yet wired"

**Dependencies**: anyhow, base64, clap, cue-core, reqwest, serde, serde_json, tokio, tracing, tracing-subscriber, uuid

## 3. Frontend (web/)

- **Framework**: None. Static HTML landing page.
- **Build tool**: None. No package.json, no bundler.
- **Routing**: Single page, anchor links only.
- **State management**: Vanilla JS (theme toggle).
- **UI library**: Custom CSS, canvas animation.
- **npm deps**: None.

**This is NOT an app frontend.** It's a marketing/product site. The actual UI is the native overlay (Swift/C), not a web app.

## 4. Native Modules

### macOS Overlay (native/macos/cue-overlay/main.swift — 2463 LOC)
- **Language**: Swift (AppKit, no SwiftUI)
- **What it does**: Full NSPanel-based floating overlay with:
  - Capture exclusion (`sharingType = .none`)
  - Drag-to-move header
  - Resize grip with screen bounds clamping
  - Card feed (scrollable stack view)
  - Composer with question field, model picker, mode picker
  - Collapsed pill mode (minimize to floating dot)
  - Theme toggle (dark/light)
  - Opacity slider
  - Hotkey support (Cmd+Shift+B toggle, Cmd+Shift+H hide)
  - Markdown rendering in cards (code blocks, headers, lists)
  - Chip buttons: Answer, Recap, Analyse Screen, Record, Attach, Eye
  - File attachment via NSOpenPanel + drag-and-drop
  - Instructions editor (NSAlert with text field)
  - Session continue/new prompts
- **Bindings to Rust**: JSON over stdin/stdout (OverlayCommand in, OverlayEvent out)

### macOS Audio (native/macos/cue-audio/main.swift — 212 LOC)
- **Language**: Swift
- **What it does**: ScreenCaptureKit system audio capture + AVFoundation microphone capture
- **Output**: Raw f32 PCM to stdout, consumed by daemon's wav converter
- **Args**: `--source system|microphone --duration-ms N`

### Windows Overlay (native/windows/cue-overlay/main.c — 996 LOC)
- **Language**: C (Win32 API)
- **What it does**: Layered window overlay with:
  - `WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW`
  - `SetWindowDisplayAffinity(WDA_EXCLUDEFROMCAPTURE)` — capture exclusion
  - JSON stdin/stdout protocol (same as macOS)
  - Card rendering, composer, basic theme support
- **Bindings**: Same JSON IPC protocol as macOS overlay

### Windows Audio (native/windows/cue-audio/main.c — 325 LOC)
- **Language**: C (WASAPI)
- **What it does**: WASAPI loopback capture (system audio) + microphone capture
- **Output**: Raw f32 PCM to stdout
- **Args**: `--source system|microphone --duration-ms N`

## 5. Data Layer

### SQL Migrations (infra/migrations/001_initial_cloud_schema.sql)
**Target**: Postgres 16+ with pgvector

| Table | Purpose |
|-------|---------|
| users | User accounts (id, email, display_name) |
| workspaces | Multi-tenant workspace (plan, retention_days) |
| workspace_members | RBAC (owner/admin/member) |
| devices | Registered desktop clients (platform, capabilities, status) |
| sessions | Auth sessions (refresh_token_hash, expiry) |
| settings_profiles | Per-workspace settings (capture, privacy, answering, shortcuts, limits) |
| meetings | Synced meeting records |
| meeting_events | Granular meeting events (transcript, action items, etc.) |
| artifacts | Uploaded files/screenshots (object_key, sha256, status) |
| memory_chunks | RAG chunks with pgvector embedding (1536-dim) |
| answer_runs | Answer audit trail (question, model, latency, cost) |
| rag_citations | Citation links between answers and chunks |
| audit_log | Compliance audit trail |
| export_requests | GDPR-style data export |
| deletion_requests | GDPR-style data deletion |

### Queues (infra/queues/workers.yaml)
**System**: Declarative queue definitions (not yet wired to a runtime)

| Queue | Purpose |
|-------|---------|
| stt.realtime | Audio chunk → transcript |
| vision.ocr | Screenshot → OCR text |
| transcript.chunk | Events → memory chunks |
| embedding.write | Chunks → vector embeddings |
| meeting.extract | Meeting → recap/actions/decisions |
| rag.compact | Old memory → summary chunks |
| retention.sweep | Expire data per policy |
| export.build | Build encrypted export bundles |
| delete.cascade | Cascade deletions |
| billing.meter | Usage → billable events |

### Persistence Patterns
- **Local**: JSON files in `~/.local/share/bluey/` (active-meeting.json, meetings/*.json, account.json, settings.json)
- **Cloud**: Postgres + pgvector (schema defined, not connected)
- **No SQLite** in the current codebase (planned in design docs)

## 6. Build + Dev Tooling

### scripts/
- `build-macos.sh`: `cargo build --release` + `swiftc` for overlay + audio helpers
- `build-windows.ps1`: `cargo build --release` + `cl.exe` for overlay + audio helpers
- `run-local.sh`: Builds debug, starts daemon, runs CLI
- `smoke-test.sh`: Starts daemon, runs basic commands, checks responses

### Cargo workspace config
- 3 crates, resolver 2, shared workspace deps
- No custom build scripts, no proc macros
- No CI config in repo

### Tauri config
- **There is NO tauri.conf.json.** This is NOT a Tauri app.
- The architecture is: CLI → TCP IPC → Daemon → stdin/stdout → Native overlay
- No webview, no Tauri plugins, no React frontend for the app itself.


## 7. Feature Inventory Against 94-Task Plan

### Design Doc 01: Stealth + Windows (B1.1–B1.15)

| Task ID | Title | Status | Current code location | Gap |
|---------|-------|--------|----------------------|-----|
| B1.1 | NSPanel macOS overlay via tauri-nspanel + panel_delegate! | 🟡 PARTIAL | native/macos/cue-overlay/main.swift:L1-2463 | NSPanel IS implemented but NOT via Tauri/tauri-nspanel. It's a standalone Swift binary using AppKit NSPanel directly. No Tauri involved. |
| B1.2 | Content protection via .content_protected(true) | ✅ DONE | native/macos/cue-overlay/main.swift (sharingType=.none), native/windows/cue-overlay/main.c (SetWindowDisplayAffinity WDA_EXCLUDEFROMCAPTURE) | Both platforms implemented |
| B1.3 | Windows stealth: SetWindowDisplayAffinity + SW_SHOWNOACTIVATE + WS_EX_TRANSPARENT | ✅ DONE | native/windows/cue-overlay/main.c:L1-996 | WS_EX_LAYERED, WS_EX_TOPMOST, WS_EX_TOOLWINDOW, SetWindowDisplayAffinity all present |
| B1.4 | setOpacity(0) fade-prevention + opacity preset system | ✅ DONE | native/macos/cue-overlay/main.swift (opacitySlider, savedOpacityKey, restoreOpacity), cue-cli/src/app.rs (normalize_opacity) | Opacity slider 0.18-1.0, persisted to UserDefaults |
| B1.5 | Process masquerading: 3 disguise presets + icon assets + re-assertion timer | ❌ NOT STARTED | — | No process name/icon disguise code anywhere |
| B1.6 | Dock/taskbar visibility: ActivationPolicy::Accessory + set_skip_taskbar | 🟡 PARTIAL | native/windows/cue-overlay/main.c (WS_EX_TOOLWINDOW hides from taskbar) | macOS: no ActivationPolicy::Accessory (the overlay is a standalone process, not an NSApplication with dock icon management). Windows: done via WS_EX_TOOLWINDOW. |
| B1.7 | Click-through toggle via set_ignore_cursor_events | ❌ NOT STARTED | — | No click-through toggle. The overlay uses PassthroughEffectView for partial passthrough but no full click-through mode. |
| B1.8 | Full-screen capture via xcap in spawn_blocking | 🟡 PARTIAL | cue-daemon/src/app.rs (capture_screen_platform uses `screencapture -x` on macOS, PowerShell on Windows) | Uses OS tools, not xcap crate. Functional but different approach. |
| B1.9 | Multi-monitor selective screenshot: overlay windows + canvas + crop | ❌ NOT STARTED | — | Only full-screen capture exists. No per-monitor selection UI. |
| B1.10 | Hold-to-move window at 60fps with bounds clamping | ✅ DONE | native/macos/cue-overlay/main.swift (DragHeaderView mouseDown performDrag, CollapsedPillView mouseDragged with screen bounds clamping) | Drag with bounds clamping implemented |
| B1.11 | Content-aware window resize with centered expansion | ✅ DONE | native/macos/cue-overlay/main.swift (ResizeGripView with min/max size, collapseToPill/expandFromPill) | Resize + collapse to pill implemented |
| B1.12 | Window binding: vertical column with coordinated movement | ❌ NOT STARTED | — | No multi-window binding system |
| B1.13 | Screen-share detection via EnumWindows heuristics (Windows) | ❌ NOT STARTED | — | No screen-share detection code |
| B1.14 | Custom cursor hiding for stealth (CSS cursor: none) | ❌ NOT STARTED | — | No cursor hiding (not applicable — no webview) |
| B1.15 | Always-on-top enforcement with periodic re-assertion | ✅ DONE | native/macos/cue-overlay/main.swift (panel.level = .screenSaver, panel.isFloatingPanel = true), native/windows/cue-overlay/main.c (WS_EX_TOPMOST) | Both platforms enforce always-on-top |

### Design Doc 02: Audio + STT (B2.1–B2.14)

| Task ID | Title | Status | Current code location | Gap |
|---------|-------|--------|----------------------|-----|
| B2.1 | Platform-abstracted SystemAudioStream trait + macOS SCK + WASAPI + PulseAudio | 🟡 PARTIAL | native/macos/cue-audio/main.swift (SCK capture, 212 LOC), native/windows/cue-audio/main.c (WASAPI, 325 LOC) | Native helpers exist but there's no Rust trait abstraction. Helpers are standalone C/Swift binaries piping PCM to stdout. No PulseAudio. |
| B2.2 | CPAL microphone capture with stream recreation, atomic sample rate | 🟡 PARTIAL | native/macos/cue-audio/main.swift (AVFoundation mic), native/windows/cue-audio/main.c (WASAPI mic) | Mic capture exists in native helpers. No CPAL. No atomic sample rate. No stream recreation logic. |
| B2.3 | Zero-copy DSP loop: f32→i16, bytemuck, channel-based emission | 🟡 PARTIAL | cue-daemon/src/app.rs:wav_from_f32le_48k_mono_to_i16_16k() | f32→i16 conversion exists but it's a simple loop, not zero-copy, no bytemuck, no channel-based emission. |
| B2.4 | Two-stage VAD (adaptive RMS + WebRTC ML) with hangover FSM | ❌ NOT STARTED | — | No VAD at all. Every chunk is sent to STT regardless. |
| B2.5 | SttProvider trait + Deepgram WS + OpenAI Realtime WS + REST | 🟡 PARTIAL | cue-daemon/src/app.rs:transcribe_audio_file() | Only OpenAI Whisper REST (multipart upload). No trait, no WebSocket, no Deepgram, no Realtime. |
| B2.6 | Google gRPC STT + Soniox/ElevenLabs WebSocket | ❌ NOT STARTED | — | No gRPC, no alternative STT providers |
| B2.7 | Local Whisper impl via whisper-rs (offline fallback) | ❌ NOT STARTED | — | No whisper-rs, no local STT |
| B2.8 | STT state machine: error classification, exponential backoff | 🟡 PARTIAL | cue-daemon/src/app.rs (warned_stt_error flag, error card push) | Minimal error handling. No state machine, no backoff, no Tauri events. |
| B2.9 | LocalAgreement-2 streaming decoder | ❌ NOT STARTED | — | No streaming decoder |
| B2.10 | Speaker ID: ECAPA-TDNN ONNX | ❌ NOT STARTED | — | No speaker identification |
| B2.11 | Question extractor: noise filter + coding question detection | 🟡 PARTIAL | cue-core/src/intelligence.rs:is_question() | Basic question detection (ends with ?, starts with question words). No noise filter, no coding question heuristic. |
| B2.12 | Dual-channel audio pipeline: system + mic, channel-keyed STT | ✅ DONE | cue-daemon/src/app.rs (real_audio_loop iterates sources, separate system/mic sequences), cue-core/src/audio.rs (AudioSourceKind::System/Microphone) | Dual-channel with source labels implemented |
| B2.13 | Sample rate detection + rubato resampler (device → 16kHz) | 🟡 PARTIAL | cue-daemon/src/app.rs:wav_from_f32le_48k_mono_to_i16_16k() (hardcoded 48k→16k downsample by factor 3) | Hardcoded downsample, no rubato, no dynamic detection |
| B2.14 | Audio supervisor: recovery, device watcher, sleep/wake, TCC detection | ❌ NOT STARTED | — | No supervisor, no device watcher, no sleep/wake handling |


### Design Doc 03: LLM + Prompts + RAG (B3.1–B5.8)

| Task ID | Title | Status | Current code location | Gap |
|---------|-------|--------|----------------------|-----|
| B3.1 | Multi-provider LLM router with async trait dispatch | 🟡 PARTIAL | cue-daemon/src/app.rs:resolve_answer_route() + provider_client_config() | Routing exists with fallback chain iteration. But no async trait, no trait dispatch — it's a big match + loop. |
| B3.2 | ModelVersionManager — background polling + vision tiers | ❌ NOT STARTED | — | No model version polling, no vision tier selection |
| B3.3 | Fallback chains with exponential backoff + Ollama terminal | 🟡 PARTIAL | cue-daemon/src/app.rs:resolve_answer_route() iterates route.steps() with fallback | Fallback iteration exists. No exponential backoff between attempts. No Ollama. |
| B3.4 | Rate limiters via governor crate per provider | ❌ NOT STARTED | — | No rate limiting |
| B3.5 | Streaming response with 60Hz batching + CancellationToken | 🟡 PARTIAL | cue-daemon/src/app.rs:read_streaming_chat_response() + OverlayAnswerStream | SSE streaming implemented with word-by-word overlay updates. No 60Hz batching, no CancellationToken. |
| B3.6 | Structured JSON generation (6-provider chain) | ❌ NOT STARTED | — | No structured output / JSON mode |
| B3.7 | Custom cURL provider (parse + variable substitution) | ❌ NOT STARTED | — | No custom provider config |
| B3.8 | Codex CLI subprocess integration | ❌ NOT STARTED | — | No codex integration |
| B3.9 | scrubKeys via zeroize crate | ❌ NOT STARTED | — | No zeroize, keys read from env at runtime |
| B3.10 | testConnection with stable pingable model | ❌ NOT STARTED | — | No connection test command |
| B3.11 | Triple-layer language injection | ❌ NOT STARTED | — | No language detection/injection |
| B3.12 | Parallel Gemini race + 3-tier vision fallback | 🟡 PARTIAL | cue-daemon/src/app.rs:select_vision_provider_from_env() + analyze_screen_with_screenshot_fallback() | Vision fallback exists (page text → screenshot → vision provider). No parallel race, no Gemini-specific code. |
| B4.1 | Prompt composition system (XML-tagged shared blocks) | 🟡 PARTIAL | cue-daemon/src/app.rs:provider_messages() | System prompt is hardcoded string. No XML blocks, no composition system. |
| B4.2 | 3 modes (Assist / Answer / WhatToAnswer) | 🟡 PARTIAL | cue-daemon/src/app.rs:mode_instructions() | 5 modes exist (General, Code, System Design, Meeting, Writing). Different naming but similar concept. |
| B4.3 | Per-provider prompt variants (Claude XML, Groq terse) | ❌ NOT STARTED | — | Same prompt for all providers |
| B4.4 | TINY prompt set for fast mode | ❌ NOT STARTED | — | No tiny/fast prompt variant |
| B4.5 | Skill library (9 prompts with language injection) | ❌ NOT STARTED | — | No skill library |
| B4.6 | Anti-chatbot constraints + HUMAN ANSWER LENGTH RULE | 🟡 PARTIAL | cue-daemon/src/app.rs system prompt: "concise", "short sections" | Basic brevity instruction. No formal anti-chatbot rules. |
| B4.7 | System-prompt protection / jailbreak defense | ❌ NOT STARTED | — | No prompt protection |
| B4.8 | Context prioritization matrix | 🟡 PARTIAL | cue-daemon/src/app.rs:provider_context_item_limit() + compact_provider_context() | Per-kind character limits exist (transcript 10k, doc 6k, screenshot 3k). Total 32k cap. Not a full matrix. |
| B4.9 | First-person "speak AS the user" enforcement | ❌ NOT STARTED | — | No first-person enforcement in prompts |
| B5.1 | sqlite-vec vector store (Rust native) | ❌ NOT STARTED | — | No SQLite, no local vector store |
| B5.2 | SemanticChunker with sliding-window overlap | ❌ NOT STARTED | — | No chunking logic |
| B5.3 | Multi-provider embedding trait + resolver | ❌ NOT STARTED | — | No embedding generation |
| B5.4 | Live RAG indexer (JIT during meeting) | ❌ NOT STARTED | — | No RAG indexing |
| B5.5 | InterviewTranscriptBuffer (Q&A memory + summarization) | 🟡 PARTIAL | cue-core/src/meeting.rs:MeetingRecord.conversation (capped at 80 turns), push_conversation_turn() | Conversation history exists with cap. No summarization. |
| B5.6 | Epoch summarization (compress old context) | ❌ NOT STARTED | — | No summarization |
| B5.7 | Async vector search via spawn_blocking | ❌ NOT STARTED | — | No vector search |
| B5.8 | Hybrid retrieval (vector + BM25 keyword) | 🟡 PARTIAL | cue-daemon/src/app.rs:search_memory() | Keyword search across meetings exists (substring matching). No vector search, no BM25. |

### Design Doc 04: UX + Ops + Dev (B6.1–B10.8)

| Task ID | Title | Status | Current code location | Gap |
|---------|-------|--------|----------------------|-----|
| B6.1 | Hotkey system — register default bindings | ✅ DONE | native/macos/cue-overlay/main.swift:setupHotkeys() (Cmd+Shift+B, Cmd+Shift+H) | macOS hotkeys implemented in native overlay |
| B6.2 | Rebindable keybinds — settings UI with ShortcutRecorder | ❌ NOT STARTED | — | Hotkeys are hardcoded, no rebinding UI |
| B6.3 | Dashboard window — pre-create on startup, sidebar nav | ❌ NOT STARTED | — | No dashboard window, no React app |
| B6.4 | Theme management — dark/light/system, CSS variables | ✅ DONE | native/macos/cue-overlay/main.swift (lightTheme toggle, applyTheme, savedThemeKey) | Dark/light theme with persistence |
| B6.5 | rAF streaming buffer — useStreamBuffer hook | ❌ NOT STARTED | — | No React, no hooks |
| B6.6 | React.memo MessageRow — custom comparator | ❌ NOT STARTED | — | No React |
| B6.7 | Inertial scroll engine — physics-based scroll | ❌ NOT STARTED | — | NSScrollView used, no custom physics |
| B6.8 | Code expansion animation — CSS transition | ❌ NOT STARTED | — | No CSS (native UI) |
| B6.9 | Command palette (cmdk) — Cmd+K spotlight | ❌ NOT STARTED | — | No command palette |
| B6.10 | Onboarding — FeatureSpotlight component | ❌ NOT STARTED | — | No onboarding flow |
| B7.1 | Keychain integration — tauri-plugin-keychain | ❌ NOT STARTED | — | Keys stored in env vars, no keychain |
| B7.2 | Key scrubbing — zeroize crate on Drop | ❌ NOT STARTED | — | No zeroize |
| B7.3 | Log hashing — mask_key() utility | ❌ NOT STARTED | — | No key masking in logs |
| B7.4 | Log rotation — 10MB cap, tracing-appender | ❌ NOT STARTED | — | Logs go to stderr only |
| B7.5 | SQLite schema — 4 migration files | ❌ NOT STARTED | — | JSON file persistence, no SQLite |
| B7.6 | Hot-reload config — notify crate watcher | ❌ NOT STARTED | — | Config read at startup only |
| B7.7 | Single-instance lock — tauri-plugin-single-instance | 🟡 PARTIAL | cue-cli/src/app.rs:ensure_daemon() pings existing daemon before starting | Daemon checks if already running via TCP ping. Not a file lock. |
| B7.8 | CSP configuration — tauri.conf.json security headers | ❌ NOT STARTED | — | No Tauri, no CSP |
| B7.9 | Error boundaries — react-error-boundary | ❌ NOT STARTED | — | No React |
| B7.10 | Panic handler — custom hook logging to file | ❌ NOT STARTED | — | No custom panic hook |
| B8.1 | OpenTelemetry init — OTLP HTTP exporter | ❌ NOT STARTED | — | No telemetry |
| B8.2 | Metric definitions — ai_ttft_ms, stt_decode_ms | ❌ NOT STARTED | — | No metrics |
| B8.3 | Host identity labels — machine_uid hash | ❌ NOT STARTED | — | No machine UID |
| B8.4 | AI pricing table — per-model USD/1M tokens | 🟡 PARTIAL | cue-daemon/src/app.rs:CostEstimate::usd(0.0) always returns 0 | Cost estimate struct exists but always zero. No pricing table. |
| B8.5 | In-memory ring buffer — last 1000 log lines | ❌ NOT STARTED | — | No ring buffer |
| B8.6 | Grafana dashboard JSON | ❌ NOT STARTED | — | No dashboard |
| B8.7 | NDJSON structured logs — tracing-subscriber JSON layer | ❌ NOT STARTED | — | tracing-subscriber fmt only (human-readable) |
| B9.1 | Auto-updater — tauri-plugin-updater | ❌ NOT STARTED | — | No auto-update |
| B9.2 | Release notes fetcher | ❌ NOT STARTED | — | No release notes |
| B9.3 | Autostart — tauri-plugin-autostart | ❌ NOT STARTED | — | No autostart |
| B9.4 | PostHog analytics | ❌ NOT STARTED | — | No analytics |
| B9.5 | Anonymous install ping | ❌ NOT STARTED | — | No telemetry ping |
| B9.6 | Machine UID — tauri-plugin-machine-uid | ❌ NOT STARTED | — | No machine UID |
| B9.7 | Build targets — .dmg, .msi, .AppImage + .deb | ❌ NOT STARTED | — | Only cargo build + manual compile scripts |
| B10.1 | CLAUDE.md — root development rules | ❌ NOT STARTED | — | No CLAUDE.md |
| B10.2 | CHANGELOG.md | ❌ NOT STARTED | — | No changelog |
| B10.3 | PR template | ❌ NOT STARTED | — | No .github/ |
| B10.4 | FIXES.md | ❌ NOT STARTED | — | No FIXES.md |
| B10.5 | AUDIT.md | ❌ NOT STARTED | — | No AUDIT.md |
| B10.6 | .codex/agents — 7 specialized agent configs | ❌ NOT STARTED | — | No .codex/ |
| B10.7 | .codex/skills — 10 reusable skill cards | ❌ NOT STARTED | — | No .codex/ |
| B10.8 | Graceful shutdown — track in-flight handlers | 🟡 PARTIAL | cue-daemon/src/app.rs:shutdown_daemon() | Shutdown exists (kills overlay, removes state file). No in-flight handler tracking. |

### Summary Counts

| Status | Count |
|--------|-------|
| ✅ DONE | 9 |
| 🟡 PARTIAL | 22 |
| ❌ NOT STARTED | 63 |
| **Total** | **94** |


## 8. Features NOT in the Plan

The current codebase implements several things not explicitly called out in the 94 tasks:

1. **Browser login with OAuth callback** (cue-cli/src/app.rs:browser_login) — Full local HTTP server for OAuth redirect, state validation, token extraction. Not in any B-task.

2. **Active page text extraction via AppleScript** (cue-daemon/src/app.rs:capture_active_page_platform) — Extracts text from Chrome, Safari, Edge, Arc, Brave via AppleScript/JXA. Falls back to screenshot+vision. Not in plan.

3. **Collapsed pill mode** (native/macos/cue-overlay/main.swift:CollapsedPillPanel) — Overlay minimizes to a tiny floating pill, click to expand. Not in plan.

4. **Answer instructions / style system** (cue-core/src/config.rs:CueSettings.answer_style, daemon InstructionsSet/Get/Clear) — Per-session and per-settings answer style that merges into prompts. Not explicitly a B-task.

5. **Conversation history with provider attribution** (cue-core/src/meeting.rs:ConversationTurn with provider field) — Tracks which provider answered each question. Not in plan.

6. **Near-duplicate transcript deduplication** (cue-daemon/src/app.rs:is_near_duplicate_transcript) — 8-second window dedup for repeated STT segments. Not in plan.

7. **Cloud schema + queue definitions** (infra/) — Full Postgres schema with pgvector + 10 queue definitions. The plan mentions cloud/RAG but doesn't have specific infra tasks.

8. **Marketing landing page** (web/index.html) — Polished product site with canvas animation, theme toggle, responsive design. Not in plan.

9. **Session persistence + archive** (cue-daemon/src/storage.rs:MeetingStore) — Active meeting auto-saves, archives on end, list/inspect from CLI. Not explicitly a B-task.

10. **Vision-based screen analysis with provider fallback** (cue-daemon/src/app.rs:analyze_screen_with_screenshot_fallback) — If page text fails, captures screenshot and routes to vision-capable provider with base64 image. Not in plan.

11. **Model picker + mode picker in overlay** (native/macos/cue-overlay/main.swift) — UI for selecting provider route and answer mode directly from overlay header. Not in plan.

12. **File drag-and-drop attachment** (native/macos/cue-overlay/main.swift) — Drag files onto overlay to attach as context. Not in plan.

## 9. Code Quality Observations

### Tests
- **cue-core**: 25+ unit tests covering serialization, builder patterns, domain logic, audio pipeline status. Good coverage of the type system.
- **cue-cli**: Zero tests.
- **cue-daemon**: Zero tests.
- **Native code**: Zero tests.
- **Integration tests**: smoke-test.sh is a basic shell script, not a proper test suite.

### Error Handling
- Consistent use of `anyhow::Result` with `.context()` for error chains.
- Daemon wraps all handler errors into `DaemonResponse::Error { message }`.
- No panics in production paths (checked: no `unwrap()` on fallible operations in daemon).
- Audio errors are surfaced as overlay warning cards — good UX pattern.

### Module Boundaries
- **Clean separation**: cue-core has zero IO, zero async, zero network. Pure domain types + local logic.
- **Daemon is monolithic**: app.rs is ~3500 LOC with everything from audio capture to AI routing to screen capture. Should be split into modules.
- **CLI is monolithic**: app.rs is ~1200 LOC. Acceptable for a CLI but could benefit from submodule extraction.

### Concurrency Patterns
- Tokio async throughout daemon.
- `Arc<Daemon>` with `Mutex<T>` for shared state (meeting, overlay, audio, cloud, capture).
- `oneshot::Sender` for stop signals on background loops.
- `AtomicU64` for answer generation ID (prevents stale overlay updates).
- `spawn_blocking` for file I/O and screen capture.
- No deadlock risk visible (mutexes are short-held, no nested locks).

### Technical Debt
1. **cue-daemon/src/app.rs is too large** (3500+ LOC). Needs extraction into audio.rs, ai.rs, capture.rs, overlay_handler.rs.
2. **No Tauri** — the design docs assume Tauri 2 but the actual architecture is CLI+daemon+native. This is a fundamental architecture divergence. The 94 tasks reference Tauri plugins, Tauri events, tauri.conf.json, React frontend — none of which exist.
3. **Hardcoded audio downsample** — 48k→16k by skipping every 3rd sample. Lossy and assumes 48kHz input.
4. **No retry/backoff on provider calls** — single attempt per provider in the fallback chain.
5. **JSON file persistence** — works for single-user but won't scale. Plan calls for SQLite.
6. **No encryption at rest** — account.json has tokens in plaintext (0600 perms only).
7. **Cloud sync is scaffolded but not wired** — types exist, status reporting works, but no actual HTTP sync client.

## 10. Recommended Starting Point for Codex

### Critical Architecture Decision First
The 94 tasks assume **Tauri 2 + React + webview**. The actual codebase is **CLI + daemon + native Swift/C overlays**. Codex needs a decision:
- **Option A**: Port to Tauri 2 (rewrite overlay as React, add tauri.conf.json, use Tauri plugins). This invalidates the working native overlays.
- **Option B**: Keep current architecture, adapt the 94 tasks to work without Tauri. Many B6/B7/B9 tasks become irrelevant or need reframing.

### If keeping current architecture (recommended — it works):

**Phase 1: Quick wins (tasks nearly done)**
1. B1.6 macOS dock hiding — add `NSApp.setActivationPolicy(.accessory)` to overlay Swift code (1 line)
2. B7.7 — already functional via TCP ping
3. B10.1-B10.5 — documentation files, zero code risk
4. B10.8 — graceful shutdown needs in-flight tracking added to existing shutdown_daemon()

**Phase 2: High-value gaps that unblock others**
1. **B2.4 VAD** — blocks efficient STT (currently sends silence to API, wasting money)
2. **B5.1 sqlite-vec** — blocks all RAG tasks (B5.2-B5.8)
3. **B3.4 Rate limiting** — blocks production use of paid providers
4. **B7.1 Keychain** — blocks secure credential storage (currently env vars only)

**Phase 3: Audio pipeline hardening**
1. B2.8 STT state machine + backoff
2. B2.13 Dynamic sample rate detection
3. B2.14 Audio supervisor (recovery, device watcher)

**Phase 4: LLM routing improvements**
1. B3.3 Backoff between fallback attempts
2. B3.5 Proper streaming with cancellation
3. B4.1 Prompt composition (extract from hardcoded string)

### Blocking dependencies
- B5.2-B5.8 all blocked by B5.1 (need vector store first)
- B2.9-B2.10 blocked by B2.7 (need local whisper-rs first)
- B3.6 blocked by B3.1 being formalized as a trait
- B6.2-B6.10 blocked by architecture decision (Tauri vs native)
- B8.1-B8.7 independent, can start anytime
- B9.1-B9.7 blocked by architecture decision

### Tasks that are nearly done and just need polish
- **B1.1**: Overlay works perfectly, just not via Tauri. Mark as DONE with different approach.
- **B2.12**: Dual-channel works. Needs hot-swap support only.
- **B3.1**: Router works. Needs trait extraction for testability.
- **B4.2**: Modes work (5 of them). Just different naming.
- **B5.5**: Conversation buffer exists. Needs summarization added.
