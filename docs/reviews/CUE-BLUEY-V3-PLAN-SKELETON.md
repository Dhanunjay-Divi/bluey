# bluey Master Plan V3 — Complete

**Architecture**: Native Swift/C overlay (keep 3500 LOC) + cue-daemon (Rust) + Tauri 2 dashboard (React 19)
**Total tasks**: 123 (97 existing + 26 new)
**Date**: 2026-05-12
**Status**: SKELETON — detail fills in Steps 2-3

---

## Part 1: Executive Summary

bluey is a stealth AI copilot for live conversations. V3 locks a hybrid architecture: the existing native overlay (Swift macOS / C Windows) handles real-time display via Unix domain socket IPC to a Rust daemon, while a Tauri 2 + React 19 dashboard provides settings, session management, and prompt editing via Tauri invoke/events. Intelligence routes through three lanes — Snap (Cerebras DeepSeek V3, 80-300ms), Solve (Claude Sonnet 4.5, 500ms-4s streaming), and Think (o3/Claude Opus extended-thinking, 15-60s) — with an intent classifier dispatching in <5ms. Patch-mode follow-ups use PATCH/KEEP/MODIFY/ADD/REMOVE format so the overlay renders diffs, not replacements. STT is Deepgram Nova-3 streaming WebSocket (persistent per session). No local inference; pure cloud, optimized for latency + correctness. Cost target: $1-4 per 3-hour session.

---

## Part 2: Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│  USER'S MACHINE                                                             │
│                                                                             │
│  ┌─────────────────────┐     Unix Domain Socket      ┌──────────────────┐  │
│  │ [1] Native Overlay  │◄════════════════════════════►│ [2] cue-daemon   │  │
│  │  Swift (macOS)      │     HOT PATH (<10ms)         │  (Rust binary)   │  │
│  │  C (Windows)        │                              │                  │  │
│  │  • Content-protected│                              │  Owns:           │  │
│  │  • Always-on-top    │                              │  • Audio capture  │  │
│  │  • Diff rendering   │                              │  • STT (Deepgram)│  │
│  │  • Mode indicator   │                              │  • Three-lane LLM│  │
│  │  • Patch display    │                              │  • RAG (sqlite)  │  │
│  └─────────────────────┘                              │  • Session state │  │
│                                                       │  • Intent router │  │
│  ┌─────────────────────┐     Tauri invoke/events      │  • Context mgmt  │  │
│  │ [3] Tauri Dashboard │◄════════════════════════════►│                  │  │
│  │  React 19 + Radix   │     COLD PATH (10-30ms)      └───────┬──────────┘  │
│  │  • Sessions         │                                      │              │
│  │  • Settings         │                                      │              │
│  │  • Prompts/Skills   │                              ┌───────▼──────────┐  │
│  │  • Shortcuts        │                              │ [4] Cloud APIs   │  │
│  │  • Dev tools        │                              │  • Cerebras (Snap)│  │
│  └─────────────────────┘                              │  • Claude (Solve)│  │
│                                                       │  • o3/Opus(Think)│  │
│  ┌─────────────────────┐                              │  • Deepgram STT  │  │
│  │ [5] cue-cli         │──── TCP IPC (57321) ────────►│  HTTP/2 keep-alive│  │
│  │  Power user shell   │                              └──────────────────┘  │
│  └─────────────────────┘                                                    │
└─────────────────────────────────────────────────────────────────────────────┘

IPC Summary:
  • Overlay ↔ Daemon: Unix domain socket (hot path, <10ms, streaming tokens)
  • Dashboard ↔ Daemon: Tauri invoke/events (cold path, 10-30ms)
  • CLI ↔ Daemon: TCP localhost:57321 (existing, preserved)
  • Daemon → Cloud: HTTP/2 persistent connections (3 prewarmed pools)
  • Daemon → Deepgram: WebSocket (persistent per session, never closed mid-session)
```

---

## Part 3: Three-Lane Routing Spec

| Property | Snap Lane | Solve Lane | Think Lane |
|----------|-----------|------------|------------|
| **Model** | Cerebras DeepSeek V3 | Claude Sonnet 4.5 | o3 / Claude Opus |
| **Latency** | 80-300ms | 500ms-4s (streaming) | 15-60s (progress bar) |
| **Use cases** | Conversational, behavioral, follow-ups, simple Q&A | Coding, system design, technical analysis | Hardest problems, multi-step reasoning |
| **Token budget** | 2K input / 1K output | 16K input / 4K output | 32K input / 8K output |
| **Context strategy** | Fresh (last 3 turns only) | Refinement (relevant turns via RAG) | Full history + epoch summaries |
| **Prompt set** | TINY (<500 tokens) | Full composition + skill template | Full + extended-thinking preamble |
| **Caching** | None (too fast to matter) | Anthropic prompt caching (50-70% savings) | Anthropic prompt caching |
| **Upgrade path** | → Solve if classifier confidence <0.7 | → Think if user says "think harder" / Alt+D | N/A (terminal) |
| **Cancel semantics** | Instant cancel on new input | Drop answer on lane upgrade | Drop answer on lane upgrade |
| **Override** | Alt+F or /snap | Default (no override needed) | Alt+D or /think or /deep |
| **Progress UI** | None (too fast) | Streaming tokens in overlay | Progress bar + visible-thinking |

**Router decision flow**:
```
User input → Intent classifier (rule-based + embedding, <5ms)
  ├─ confidence ≥ 0.8 for Snap → dispatch Snap
  ├─ confidence ≥ 0.7 for Solve → dispatch Solve
  ├─ confidence ≥ 0.6 for Think → dispatch Think
  └─ ambiguous → parallel Snap+Solve, progressive enhancement
      (show Snap result immediately, replace with Solve when ready)
```

---

## Part 4: Competitive Positioning

| Feature | bluey V3 | Cluely | Final Round AI | natively-cluely | pluely |
|---------|----------|--------|----------------|-----------------|--------|
| Three-lane intelligent routing | ✅ | ❌ single model | ❌ single model | ❌ | ❌ |
| Sub-300ms conversational response | ✅ (Snap lane) | ❌ ~2s | ❌ ~3s | ❌ | ❌ |
| Patch-mode diff follow-ups | ✅ | ❌ full replace | ❌ | ❌ | ❌ |
| Content protection (both platforms) | ✅ | ✅ | ❌ | 🟡 macOS only | ❌ |
| Persistent STT WebSocket | ✅ | ❌ REST per utterance | ❌ | ❌ | ❌ |
| Local RAG (sqlite-vec) | ✅ | ❌ cloud only | ❌ | ❌ | ❌ |
| Speaker identification | ✅ | ❌ | ❌ | ❌ | ❌ |
| Process masquerading | ✅ | ✅ | ❌ | ❌ | ❌ |
| Multi-provider fallback | ✅ (5+ providers) | ❌ OpenAI only | ❌ | ❌ | ❌ |
| Open source / self-host | ✅ | ❌ | ❌ | ✅ | ✅ |
| Extended thinking with progress | ✅ (Think lane) | ❌ | ❌ | ❌ | ❌ |
| Cost per 3hr session | $1-4 | $15-25 | $10-15 | $5-10 | $3-8 |
| Latency (mic→first token) | <400ms | ~2500ms | ~3000ms | ~2000ms | ~2500ms |


---

## Part 5: Dependency Graph

```
                         ┌──────────────┐
                         │  PHASE 0     │
                         │  Foundation  │
                         │  (2 weeks)   │
                         └──────┬───────┘
                                │
            ┌───────────────────┼───────────────────┐
            │                   │                   │
            ▼                   ▼                   ▼
   ┌────────────────┐  ┌────────────────┐  ┌────────────────┐
   │  PHASE 1       │  │  PHASE 3       │  │  PHASE 9       │
   │  Dashboard     │  │  Listening     │  │  Latency Eng   │
   │  Shell (1wk)   │  │  Upgrade (3wk) │  │  (3 weeks)     │
   └───────┬────────┘  └───────┬────────┘  └───────┬────────┘
           │                    │                    │
           ▼                    │                    │
   ┌────────────────┐           │            ┌──────▼─────────┐
   │  PHASE 2       │           │            │  L1 Deepgram   │
   │  Session UX    │           │            │  L2 HTTP/2 pool│
   │  (2 weeks)     │           │            │  L5 Unix socket│
   └───────┬────────┘           │            │  L6 Instrument │
           │                    │            └────────────────┘
           │                    ▼
           │           ┌────────────────┐
           │           │  PHASE 4       │
           │           │  Reasoning +   │
           │           │  Three-Lane +  │◄── R1-R5 (new)
           │           │  Patch-Mode    │◄── F1-F8 (new)
           │           │  (5 weeks)     │
           │           └───────┬────────┘
           │                   │
           ▼                   ▼
   ┌────────────────┐  ┌────────────────┐
   │  PHASE 6       │  │  PHASE 5       │
   │  Dashboard     │  │  Memory + RAG  │◄── CM1-CM4 (new)
   │  Polish (2wk)  │  │  (2 weeks)     │
   └───────┬────────┘  └───────┬────────┘
           │                   │
           └─────────┬─────────┘
                     ▼
            ┌────────────────┐
            │  PHASE 7       │
            │  Ops+Security  │
            │  (2 weeks)     │
            └───────┬────────┘
                    ▼
            ┌────────────────┐
            │  PHASE 8       │
            │  Dev Discipline│
            │  (3 days)      │
            └───────┬────────┘
                    ▼
            ┌────────────────┐
            │  PHASE 10      │
            │  Cost Optimize │◄── CO1-CO3 (new)
            │  (1 week)      │
            └────────────────┘

  PARALLEL TRACKS (anytime after Phase 0):
  ┌──────────────────────────────────────────────────┐
  │ Phase 8 Dev Discipline (B10.1-B10.7) — no deps   │
  │ Phase 9 Latency Eng (L1-L6) — after Phase 0     │
  │ Observability (B8.1-B8.7) — after Phase 0       │
  └──────────────────────────────────────────────────┘

  KEY BLOCKING CHAINS:
  Phase 0 → Phase 4 (R1-R5 need daemon IPC)
  Phase 3 (B2.5 STT) → Phase 4 (F1-F3 need transcripts)
  Phase 4 (B3.1 LLM trait) → Phase 5 (embeddings)
  Phase 4 (R1 router) → Phase 10 (CO3 router split)
  Phase 5 (CM1 session model) → Phase 4 (F4-F8 need state)
  L5 (Unix socket) → Phase 4 streaming (hot path)
```


---

## Part 6: Full Task Index (123 tasks)

### Existing Tasks (97 from V2) — Status from CUE-CURRENT-STATE.md

| ID | Title | Phase | Sev | Size | Status | Deps | V3 Notes |
|---|---|---|---|---|---|---|---|
| D0.1 | Tauri 2 scaffold | 0 | 🔴 | M | ❌ | — | |
| D0.2 | Tauri↔daemon IPC | 0 | 🔴 | M | ❌ | D0.1 | |
| D0.3 | Single-binary confirmation | 0 | 🔴 | S | ❌ | D0.1 | |
| C0.1 | Session-ID model in SQLite | 0 | 🔴 | M | ❌ | D0.1 | **SUPERSEDED by CM1** — CM1 extends this |
| D0.4 | Overlay protocol upgrade | 0 | 🟡 | S | ❌ | C0.1 | Now includes mode-indicator (R5) |
| N0.1 | Audit overlay for Tauri-overlap | 0 | 🟢 | S | ❌ | D0.2 | |
| B6.3 | Dashboard sidebar nav | 1 | 🔴 | M | ❌ | D0.1,D0.2 | |
| B6.1 | Hotkey system (Cmd+Shift+D) | 1 | 🟡 | S | 🟡 | D0.1 | Extend with Alt+D/Alt+F (R3) |
| B6.4 | Theme management | 1 | 🟡 | S | ✅→ext | D0.2 | |
| B6.5 | rAF streaming buffer | 1 | 🟡 | M | ❌ | D0.2 | |
| B6.6 | React.memo MessageRow | 1 | 🟢 | S | ❌ | B6.3 | |
| B7.8 | CSP configuration | 1 | 🔴 | S | ❌ | D0.1 | |
| B7.9 | Error boundaries | 1 | 🟢 | S | ❌ | B6.3 | |
| B6.3.1 | Session list page | 2 | 🔴 | M | ❌ | C0.1,B6.3 | |
| B6.3.2 | Session detail page | 2 | 🔴 | M | ❌ | B6.3.1 | Now shows lane badge per message |
| B6.3.3 | Session switching logic | 2 | 🟡 | S | ❌ | C0.1,D0.4 | |
| B5.5 | InterviewTranscriptBuffer | 2 | 🟡 | M | 🟡 | C0.1 | **SUPERSEDED by CM3** epoch summarization |
| B10.8 | Graceful shutdown | 2 | 🟡 | S | 🟡 | C0.1 | |
| B2.4 | Two-stage VAD | 3 | 🔴 | M | ❌ | B2.3 | |
| B2.1 | SystemAudioStream trait | 3 | 🔴 | L | 🟡 | — | |
| B2.2 | CPAL microphone capture | 3 | 🟡 | M | 🟡 | — | |
| B2.3 | Zero-copy DSP loop | 3 | 🟡 | S | 🟡 | B2.1,B2.2 | |
| B2.5 | SttProvider trait + 3 providers | 3 | 🔴 | L | 🟡 | B2.3 | **REWORKED**: Deepgram Nova-3 WS is now primary (L1) |
| B2.6 | Google gRPC + Soniox + ElevenLabs | 3 | 🟢 | M | ❌ | B2.5 | |
| B2.7 | Local Whisper (whisper-rs) | 3 | 🟡 | S | ❌ | B2.5 | |
| B2.8 | STT state machine + backoff | 3 | 🟡 | S | 🟡 | B2.5 | |
| B2.9 | LocalAgreement-2 decoder | 3 | 🟢 | L | ❌ | B2.7 | |
| B2.10 | Speaker ID (ECAPA-TDNN) | 3 | 🟡 | L | ❌ | B2.5,B2.13 | |
| B2.11 | Question extractor + noise filter | 3 | 🟡 | S | 🟡 | — | Now feeds intent classifier (R1) |
| B2.12 | Dual-channel hot-swap | 3 | 🟡 | M | ✅→ext | B2.5 | |
| B2.13 | Sample rate + rubato | 3 | 🟡 | S | 🟡 | B2.1,B2.2 | |
| B2.14 | Audio supervisor + recovery | 3 | 🟡 | M | ❌ | B2.1,B2.2 | |
| B3.1 | Multi-provider LLM trait | 4 | 🔴 | L | 🟡 | — | **REWORKED**: Now three-lane aware |
| B3.2 | ModelVersionManager | 4 | 🟡 | M | ❌ | B3.1 | |
| B3.3 | Fallback chains + backoff | 4 | 🟡 | M | 🟡 | B3.1 | Per-lane fallback chains |
| B3.4 | Rate limiters (governor) | 4 | 🔴 | S | ❌ | B3.1 | Per-lane rate limits |
| B3.5 | Streaming 60Hz + cancellation | 4 | 🔴 | M | 🟡 | B3.1,D0.2 | **REWORKED**: Cancel-on-upgrade semantics |
| B3.6 | Structured JSON generation | 6 | 🟢 | M | ❌ | B3.1 | |
| B3.7 | Custom cURL provider | 6 | 🟢 | S | ❌ | B3.1 | |
| B3.8 | Codex CLI integration | 6 | 🟢 | S | ❌ | B3.1 | |
| B3.9 | Key scrubbing (zeroize) | 4 | 🟡 | S | ❌ | — | |
| B3.10 | testConnection command | 6 | 🟢 | S | ❌ | B3.1 | |
| B3.11 | Triple-layer language injection | 4 | 🟢 | S | ❌ | B4.1 | |
| B3.12 | Vision fallback + parallel race | 4 | 🟡 | M | 🟡 | B3.1 | |
| B4.1 | Prompt composition system | 4 | 🔴 | M | 🟡 | — | **REWORKED**: Per-lane prompt templates |
| B4.2 | Answer modes (3 primary) | 4 | 🟡 | M | 🟡 | B4.1 | **SUPERSEDED by R4** skill-template library |
| B4.3 | Per-provider prompt variants | 6 | 🟢 | S | ❌ | B4.1,B3.1 | |
| B4.4 | TINY prompt for fast mode | 6 | 🟢 | S | ❌ | B4.1 | Now = Snap lane prompt |
| B4.5 | Skill library (9 prompts) | 6 | 🟡 | L | ❌ | B4.1,C0.1 | **SUPERSEDED by R4** (9→extended) |
| B4.6 | Anti-chatbot constraints | 4 | 🟡 | S | 🟡 | B4.1 | |
| B4.7 | System-prompt protection | 6 | 🟢 | S | ❌ | B4.1 | |
| B4.8 | Context prioritization matrix | 4 | 🟡 | S | 🟡 | B4.1 | **REWORKED**: Per-lane token budgets (CM2) |
| B4.9 | First-person enforcement | 6 | 🟢 | S | ❌ | B4.1,B4.2 | |
| B5.1 | sqlite-vec vector store | 5 | 🔴 | M | ❌ | C0.1 | |
| B5.2 | SemanticChunker | 5 | 🔴 | M | ❌ | — | |
| B5.3 | Embedding trait + providers | 5 | 🔴 | M | ❌ | B3.1 | |
| B5.4 | Live RAG indexer | 5 | 🔴 | M | ❌ | B5.1,B5.2,B5.3 | |
| B5.6 | Epoch summarization | 5 | 🟡 | M | ❌ | B5.1,B3.1 | **SUPERSEDED by CM3** |
| B5.7 | Async vector search | 5 | 🟡 | S | ❌ | B5.1 | |
| B5.8 | Hybrid retrieval (vec+BM25) | 5 | 🟡 | M | 🟡 | B5.1,B5.7 | |
| B6.2 | Rebindable keybinds | 6 | 🟡 | M | ❌ | B6.1,C0.1 | |
| B6.7 | Inertial scroll engine | 6 | 🟢 | M | ❌ | B6.3.2 | |
| B6.8 | Code expansion animation | 6 | 🟢 | S | ❌ | B6.3.2 | |
| B6.9 | Command palette (Cmd+K) | 6 | 🟡 | L | ❌ | B6.3 | |
| B6.10 | Onboarding flow | 6 | 🟢 | S | ❌ | B6.3 | |
| B7.1 | Keychain integration | 7 | 🔴 | S | ❌ | D0.1 | |
| B7.2 | Key scrubbing on drop | 7 | 🟡 | S | ❌ | B3.9 | |
| B7.3 | Log masking utility | 7 | 🟡 | S | ❌ | — | |
| B7.4 | Log rotation + NDJSON | 7 | 🔴 | S | ❌ | — | |
| B7.5 | SQLite migration system | 7 | 🔴 | M | ❌ | C0.1 | |
| B7.6 | Hot-reload config | 7 | 🟢 | S | ❌ | — | |
| B7.7 | Single-instance lock | 7 | 🟡 | S | 🟡 | D0.1 | |
| B7.10 | Panic handler | 7 | 🟡 | S | ❌ | B7.4 | |
| B8.1 | OpenTelemetry init | 7 | 🟡 | M | ❌ | — | |
| B8.2 | Metric definitions | 7 | 🟡 | M | ❌ | B8.1 | Now includes per-lane metrics |
| B8.3 | Host identity labels | 7 | 🟢 | S | ❌ | B8.1 | |
| B8.4 | AI pricing table | 7 | 🟡 | S | 🟡 | B3.1 | **REWORKED**: Per-lane cost tracking |
| B8.5 | In-memory ring buffer | 7 | 🟢 | S | ❌ | B7.4 | |
| B8.6 | Grafana dashboard JSON | 7 | 🟢 | L | ❌ | B8.1,B8.2 | |
| B9.1 | Auto-updater | 7 | 🟡 | M | ❌ | D0.1 | |
| B9.2 | Release notes fetcher | 7 | 🟢 | S | ❌ | B9.1 | |
| B9.3 | Autostart on login | 7 | 🟢 | S | ❌ | D0.1 | |
| B9.4 | PostHog analytics | 7 | 🟢 | S | ❌ | — | |
| B9.5 | Anonymous install ping | 7 | 🟢 | S | ❌ | B8.3 | |
| B9.6 | Machine UID | 7 | 🟢 | S | ❌ | — | |
| B9.7 | Build targets (.dmg,.msi,.AppImage) | 7 | 🟡 | M | ❌ | D0.1 | |
| B10.1 | CLAUDE.md | 8 | 🟡 | S | ❌ | — | |
| B10.2 | CHANGELOG.md | 8 | 🟡 | S | ❌ | — | |
| B10.3 | PR template | 8 | 🟡 | S | ❌ | — | |
| B10.4 | FIXES.md | 8 | 🟡 | S | ❌ | — | |
| B10.5 | AUDIT.md | 8 | 🟡 | S | ❌ | — | |
| B10.6 | .codex/agents | 8 | 🟡 | M | ❌ | — | |
| B10.7 | .codex/skills | 8 | 🟢 | M | ❌ | — | |
| B1.5 | Process masquerading | 8 | 🟡 | M | ❌ | D0.1 | |
| B1.6 | Dock/taskbar visibility | 8 | 🟡 | S | 🟡 | D0.1 | |
| B1.7 | Click-through toggle | 8 | 🟢 | S | ❌ | B6.1 | |
| B1.8 | Full-screen capture (xcap) | 8 | 🟡 | S | 🟡 | — | |
| B1.9 | Multi-monitor screenshot | 8 | 🟢 | L | ❌ | B1.8,D0.1 | |
| B1.12 | Window binding | 8 | 🟢 | M | ❌ | — | |
| B1.13 | Screen-share detection | 8 | 🟢 | M | ❌ | — | |
| B1.14 | Cursor hiding | 8 | 🟢 | S | ❌ | D0.1 | |
| B1.15 | Always-on-top re-assertion | 8 | 🟢 | S | ✅→ext | — | |


### NEW Tasks (26 — V3 additions)

| ID | Title | Phase | Sev | Size | Status | Deps | Notes |
|---|---|---|---|---|---|---|---|
| **Latency Engineering (L-series)** | | | | | | | |
| L1 | Persistent Deepgram WebSocket lifecycle | 9 | 🔴 | M | ❌ | B2.5 | Never close mid-session, reconnect on drop |
| L2 | HTTP/2 keep-alive pool (3 prewarmed) | 9 | 🔴 | M | ❌ | B3.1 | Cerebras + Claude + o3 pools |
| L3 | Stable-partial detector + trigger-on-stable | 9 | 🔴 | M | ❌ | L1,R1 | Dispatch LLM only when partial stabilizes |
| L4 | Speculative LLM with cancel-on-partial-change | 9 | 🟡 | L | ❌ | L3,R1 | Start LLM early, cancel if STT changes |
| L5 | Unix domain socket daemon↔overlay (hot path) | 9 | 🔴 | M | ❌ | D0.1 | Replace stdin/stdout, <10ms streaming |
| L6 | End-to-end latency instrumentation | 9 | 🟡 | S | ❌ | L1,L2,L5 | Timestamp every hop, dashboard flamegraph |
| **Three-Lane Routing (R-series)** | | | | | | | |
| R1 | Intent classifier (rule+embedding, <5ms) | 4 | 🔴 | L | ❌ | B2.11,B3.1 | Snap/Solve/Think routing decision |
| R2 | Parallel fast+deep with progressive UI | 4 | 🔴 | M | ❌ | R1,B3.5 | Show Snap, replace with Solve if better |
| R3 | Manual override system (Alt+D/F, /think /snap) | 4 | 🟡 | S | ❌ | R1,B6.1 | Hotkeys + prefix commands |
| R4 | Skill-template library (9+ templates) | 4 | 🟡 | M | ❌ | B4.1,R1 | Vysper templates + bluey extensions |
| R5 | Overlay mode-indicator badge | 4 | 🟡 | S | ❌ | R1,D0.4 | Tiny badge: Snap/Solve/Think + model |
| **Patch-Mode Follow-ups (F-series)** | | | | | | | |
| F1 | Three-lane router integration point | 4 | 🔴 | M | ❌ | R1,B3.1 | Dispatch to correct lane |
| F2 | Solve-lane streaming (Claude Sonnet 4.5) | 4 | 🔴 | M | ❌ | F1,B3.5 | Skill template dispatch |
| F3 | Think-lane integration (o3 + visible-thinking) | 4 | 🔴 | L | ❌ | F1 | Progress bar + reasoning display |
| F4 | Follow-up intent classifier | 4 | 🟡 | M | ❌ | R1,F8 | Refinement vs new problem |
| F5 | Edit-mode PATCH/KEEP/MODIFY/ADD/REMOVE parser | 4 | 🔴 | M | ❌ | F2 | Prompt + response parser |
| F6 | Response-block state tracker | 4 | 🟡 | S | ❌ | F5 | Track displayed_response sections |
| F7 | Overlay diff rendering | 4 | 🔴 | M | ❌ | F5,F6,L5 | Highlight modified/added/removed |
| F8 | Topic fingerprint embedding | 4 | 🟡 | S | ❌ | B5.3 | Similarity continuity check |
| **Context Management (CM-series)** | | | | | | | |
| CM1 | SessionState + Turn model (SQLite) | 5 | 🔴 | M | ❌ | C0.1 | Extends C0.1 with lane metadata |
| CM2 | Context-assembly (lane-aware strategies) | 5 | 🔴 | M | ❌ | CM1,R1 | Fresh/Refinement/LaneUpgrade/LongHistory |
| CM3 | Epoch summarization background job | 5 | 🟡 | M | ❌ | CM1,B3.1 | Compress >50K tokens to summary |
| CM4 | Token counter + compaction trigger | 5 | 🟡 | S | ❌ | CM1 | Trigger CM3 at threshold |
| **Cost Optimization (CO-series)** | | | | | | | |
| CO1 | Anthropic prompt caching (Solve lane) | 10 | 🟡 | M | ❌ | F2,B3.1 | 50-70% input token savings |
| CO2 | RAG-filtered context for Solve lane | 10 | 🟡 | M | ❌ | B5.4,CM2 | Only relevant turns, not full history |
| CO3 | Router split: easy→Cerebras, hard→Claude | 10 | 🟡 | S | ❌ | R1,CO1 | Within Solve lane, cost-route |

**Task count**: 97 existing + 26 new = **123 total**

### Superseded/Reworked Tasks Summary

| V2 Task | Disposition | V3 Replacement |
|---------|-------------|----------------|
| C0.1 Session-ID model | EXTENDED | CM1 adds lane metadata, turn model |
| B5.5 TranscriptBuffer summarization | SUPERSEDED | CM3 epoch summarization (more general) |
| B5.6 Epoch summarization | SUPERSEDED | CM3 (same concept, now in CM-series) |
| B4.2 Answer modes (3 primary) | REWORKED | R1+R4 (three-lane routing replaces modes) |
| B4.5 Skill library | REWORKED | R4 (expanded from 9 to extensible) |
| B4.8 Context prioritization | REWORKED | CM2 (lane-aware strategies replace matrix) |
| B3.5 Streaming + cancellation | REWORKED | Now includes lane-upgrade cancel semantics |
| B3.1 LLM trait | REWORKED | Now three-lane aware with per-lane dispatch |


---

## Part 7: Phase-by-Phase Skeleton

### Phase 0: Foundation (~2 weeks solo)

**Entry**: Repo cloned, `cargo build` passes, Node 20+ available
**Exit**: Tauri binary builds, SQLite with sessions table, Unix domain socket IPC proven, overlay still works

**Tasks** (7):
- D0.1 [M] Tauri 2 scaffold on existing workspace
- D0.2 [M] Tauri↔daemon shared-process IPC (invoke/events)
- D0.3 [S] Single-binary architecture confirmation + ARCHITECTURE.md
- C0.1 [M] Session-ID model in daemon SQLite (sessions + messages tables)
- D0.4 [S] Native overlay protocol upgrade (SessionChanged + mode indicator prep)
- N0.1 [S] Audit native overlay for Tauri-overlap (scope doc)
- L5 [M] **NEW** Unix domain socket daemon↔overlay (replace stdin/stdout)

**Critical path**: D0.1 → D0.2 → C0.1 → D0.4; L5 parallel with D0.2

---

### Phase 1: Dashboard Shell (~1 week)

**Entry**: Phase 0 complete
**Exit**: Cmd+Shift+D opens dashboard, sidebar nav, daemon status display, theme sync

**Tasks** (7):
- B6.3 [M] Dashboard window with sidebar navigation
- B6.1 [S] Hotkey system — register Cmd+Shift+D
- B6.4 [S] Theme management — dark/light sync
- B6.5 [M] rAF streaming buffer (useStreamBuffer hook)
- B6.6 [S] React.memo MessageRow with custom comparator
- B7.8 [S] CSP configuration for Tauri webview
- B7.9 [S] Error boundaries on route components

---

### Phase 2: Session UX (~2 weeks)

**Entry**: Phase 1 complete
**Exit**: Create/switch/archive sessions, messages persist, overlay reflects session

**Tasks** (5):
- B6.3.1 [M] Session list page
- B6.3.2 [M] Session detail page (with lane badge per message)
- B6.3.3 [S] Session switching logic (overlay + dashboard sync)
- B5.5 [M] InterviewTranscriptBuffer with persistence
- B10.8 [S] Graceful shutdown with session save

---

### Phase 3: Listening Upgrade (~3 weeks)

**Entry**: Phase 0 complete (can run parallel with Phase 1-2)
**Exit**: VAD working, Deepgram Nova-3 streaming, speaker ID, audio recovery

**Tasks** (14):
- B2.1 [L] SystemAudioStream trait (wrap native helpers)
- B2.2 [M] CPAL microphone capture with stream recreation
- B2.3 [S] Zero-copy DSP loop (bytemuck + channels)
- B2.4 [M] Two-stage VAD (RMS + WebRTC ML)
- B2.5 [L] SttProvider trait + Deepgram Nova-3 WS + OpenAI REST + Groq
- B2.6 [M] Google gRPC + Soniox + ElevenLabs (stretch)
- B2.7 [S] Local Whisper via whisper-rs (offline fallback)
- B2.8 [S] STT state machine with error classification + backoff
- B2.9 [L] LocalAgreement-2 streaming decoder (stretch)
- B2.10 [L] Speaker ID via ECAPA-TDNN ONNX
- B2.11 [S] Question extractor + noise filter (feeds R1)
- B2.12 [M] Dual-channel hot-swap
- B2.13 [S] Sample rate detection + rubato resampler
- B2.14 [M] Audio supervisor with recovery

---

### Phase 4: Reasoning Upgrade — Three-Lane Router + Patch-Mode (~5 weeks)

**Entry**: Phase 0 complete, B2.5 done (STT produces transcripts)
**Exit**: Three-lane routing working, patch-mode follow-ups, progressive enhancement UI

**Batch 4A — LLM Foundation (week 1-2)**:
- B3.1 [L] Multi-provider LLM trait (three-lane aware)
- B3.2 [M] ModelVersionManager (background polling)
- B3.3 [M] Fallback chains with per-lane backoff
- B3.4 [S] Rate limiters via governor (per-lane)
- B3.5 [M] Streaming 60Hz + CancellationToken + lane-upgrade cancel
- B3.9 [S] Key scrubbing (zeroize)
- B3.12 [M] Vision fallback + parallel race

**Batch 4B — Prompt + Routing (week 2-3)**:
- B4.1 [M] Prompt composition system (per-lane templates)
- B4.2 [M] Answer modes → lane mapping
- B4.6 [S] Anti-chatbot constraints
- B4.8 [S] Context prioritization (per-lane budgets)
- R1 [L] **NEW** Intent classifier (rule+embedding, <5ms)
- R2 [M] **NEW** Parallel fast+deep with progressive-enhancement UI
- R3 [S] **NEW** Manual override system (Alt+D/F, /think /snap /deep)
- R4 [M] **NEW** Skill-template library (9+ templates)
- R5 [S] **NEW** Overlay mode-indicator badge

**Batch 4C — Patch-Mode (week 3-5)**:
- F1 [M] **NEW** Three-lane router integration point
- F2 [M] **NEW** Solve-lane streaming (Claude Sonnet 4.5 + skill dispatch)
- F3 [L] **NEW** Think-lane integration (o3 + visible-thinking progress)
- F4 [M] **NEW** Follow-up intent classifier (refinement vs new problem)
- F5 [M] **NEW** Edit-mode PATCH/KEEP/MODIFY/ADD/REMOVE prompt + parser
- F6 [S] **NEW** Response-block state tracker
- F7 [M] **NEW** Overlay diff rendering (highlight changes, fade after 1s)
- F8 [S] **NEW** Topic fingerprint embedding (similarity continuity)

**Also in Phase 4** (stretch):
- B3.11 [S] Triple-layer language injection

---

### Phase 5: Memory + RAG + Context Management (~2 weeks)

**Entry**: B3.1 done (LLM trait for embeddings), C0.1 done (SQLite)
**Exit**: Local RAG working, lane-aware context assembly, epoch summarization

**Tasks** (11):
- B5.1 [M] sqlite-vec vector store
- B5.2 [M] SemanticChunker with sliding-window overlap
- B5.3 [M] Embedding trait + providers (OpenAI + local ONNX)
- B5.4 [M] Live RAG indexer (JIT during session)
- B5.7 [S] Async vector search via spawn_blocking
- B5.8 [M] Hybrid retrieval (vector + BM25)
- CM1 [M] **NEW** SessionState + Turn model in SQLite (extends C0.1)
- CM2 [M] **NEW** Context-assembly with lane-aware strategies
- CM3 [M] **NEW** Epoch summarization background job (>50K → summary)
- CM4 [S] **NEW** Token counter + compaction trigger

*Note: B5.6 (old epoch summarization) superseded by CM3*

---

### Phase 6: Dashboard Polish (~2 weeks)

**Entry**: Phase 1+2 complete, Phase 4 partial (providers configured)
**Exit**: Full settings, prompts, shortcuts, dev page, command palette

**Tasks** (14):
- B6.2 [M] Rebindable keybinds with settings UI
- B6.7 [M] Inertial scroll engine
- B6.8 [S] Code expansion animation
- B6.9 [L] Command palette (Cmd+K)
- B6.10 [S] Onboarding flow
- B4.3 [S] Per-provider prompt variants
- B4.4 [S] TINY prompt for Snap lane
- B4.5 [L] Skill library UI (dashboard page for R4 templates)
- B4.7 [S] System-prompt protection
- B4.9 [S] First-person enforcement
- B3.6 [M] Structured JSON generation
- B3.7 [S] Custom cURL provider (dev page)
- B3.8 [S] Codex CLI integration
- B3.10 [S] testConnection command

---

### Phase 7: Ops + Security (~2 weeks)

**Entry**: Phase 0 complete, Phase 4 partial
**Exit**: Keychain, logs, telemetry, auto-updater, single-instance

**Tasks** (18):
- B7.1 [S] Keychain integration
- B7.2 [S] Key scrubbing on drop
- B7.3 [S] Log masking utility
- B7.4 [S] Log rotation + NDJSON
- B7.5 [M] SQLite migration system
- B7.6 [S] Hot-reload config
- B7.7 [S] Single-instance lock
- B7.10 [S] Panic handler + crash log
- B8.1 [M] OpenTelemetry init
- B8.2 [M] Metric definitions (per-lane TTFT, latency)
- B8.3 [S] Host identity labels
- B8.4 [S] AI pricing table (per-lane cost tracking)
- B8.5 [S] In-memory ring buffer
- B8.6 [L] Grafana dashboard JSON (stretch)
- B9.1 [M] Auto-updater
- B9.2 [S] Release notes fetcher
- B9.3 [S] Autostart on login
- B9.4 [S] PostHog analytics (opt-in)
- B9.5 [S] Anonymous install ping
- B9.6 [S] Machine UID
- B9.7 [M] Build targets

---

### Phase 8: Dev Discipline (~3 days)

**Entry**: Phase 0 complete (can run anytime in parallel)
**Exit**: All dev docs, stealth polish, codex agent configs

**Tasks** (13):
- B10.1 [S] CLAUDE.md
- B10.2 [S] CHANGELOG.md
- B10.3 [S] PR template
- B10.4 [S] FIXES.md
- B10.5 [S] AUDIT.md
- B10.6 [M] .codex/agents (7 configs)
- B10.7 [M] .codex/skills (10 cards)
- B1.5 [M] Process masquerading (3 presets)
- B1.6 [S] Dock/taskbar visibility toggle
- B1.7 [S] Click-through toggle
- B1.8 [S] Full-screen capture (xcap)
- B1.9 [L] Multi-monitor selective screenshot
- B1.12 [M] Window binding system
- B1.13 [M] Screen-share detection (Windows)
- B1.14 [S] Cursor hiding
- B1.15 [S] Always-on-top re-assertion (extend)

---

### Phase 9: Latency Engineering + Profiling (~3 weeks)

**Entry**: Phase 0 complete (can run parallel with Phases 1-4)
**Exit**: Sub-400ms mic→first-token, persistent connections, instrumented pipeline

**Tasks** (6 — all NEW):
- L1 [M] Persistent Deepgram WebSocket lifecycle (never close mid-session)
- L2 [M] HTTP/2 keep-alive pool to Cerebras + Claude (3 prewarmed connections)
- L3 [M] Stable-partial detector + trigger-on-stable dispatcher
- L4 [L] Speculative LLM with cancel-on-partial-change
- L5 [M] Unix domain socket daemon↔overlay (hot path) — *also in Phase 0*
- L6 [S] End-to-end latency instrumentation (timestamp every hop)

---

### Phase 10: Cost Optimization (~1 week)

**Entry**: Phase 4 complete (three-lane routing working), Phase 5 complete (RAG available)
**Exit**: 50-70% cost reduction on Solve lane, smart routing within Solve

**Tasks** (3 — all NEW):
- CO1 [M] Anthropic prompt caching integration (Solve lane)
- CO2 [M] RAG-filtered context for Solve lane (only relevant turns)
- CO3 [S] Router split within Solve: easy→Cerebras, hard→Claude


---

## Part 8: Timeline Summary

### Solo Engineer (Sequential Critical Path)

```
Phase 0 (2wk) → Phase 1 (1wk) → Phase 2 (2wk) → Phase 3 (3wk) →
Phase 4 (5wk) → Phase 5 (2wk) → Phase 6 (2wk) → Phase 7 (2wk) →
Phase 8 (0.5wk) → Phase 9 (3wk) → Phase 10 (1wk)

Total: ~23.5 weeks (~6 months)
```

### 2-Engineer Parallel

```
Engineer A (Backend/Daemon):           Engineer B (Frontend/Dashboard+Stealth):
Phase 0 shared (2wk)                   Phase 0 shared (2wk)
Phase 3 Listening (3wk)                Phase 1 Dashboard Shell (1wk)
Phase 4 Reasoning+Router (5wk)         Phase 2 Session UX (2wk)
Phase 5 Memory+Context (2wk)           Phase 6 Dashboard Polish (2wk)
Phase 9 Latency Eng (3wk)             Phase 8 Dev+Stealth (1wk)
Phase 10 Cost Opt (1wk)               Phase 7 Ops (2wk)

Engineer A: 16 weeks                   Engineer B: 10 weeks
Critical path: 16 weeks (~4 months)
```

### Aggressive (3 engineers, max parallelism)

```
Engineer A: Phase 0 → Phase 3 → Phase 4 (Batches 4B+4C)
Engineer B: Phase 0 → Phase 1 → Phase 2 → Phase 6 → Phase 7
Engineer C: Phase 9 (L-series) → Phase 4 (Batch 4A) → Phase 5 → Phase 10

Critical path: ~12 weeks (~3 months)
```

---

## Part 9: Cost Model

### Per-Session Cost (3-hour session, typical usage)

| Component | Usage Pattern | Cost/Session |
|-----------|--------------|--------------|
| **Deepgram Nova-3 STT** | ~90 min active speech (VAD filters silence) | $0.32 |
| **Snap lane (Cerebras)** | ~40 queries × 2K tokens avg | $0.08 |
| **Solve lane (Claude Sonnet 4.5)** | ~15 queries × 16K in / 4K out | $1.44 |
| **Solve lane w/ prompt caching** | Same, 60% cache hit | $0.72 |
| **Think lane (o3)** | ~3 queries × 32K in / 8K out | $1.20 |
| **Embeddings (OpenAI)** | ~200 chunks × 200 tokens | $0.004 |
| **Total (no caching)** | | **$3.04** |
| **Total (with CO1 caching)** | | **$2.32** |

### Pricing Tiers (if productized)

| Tier | Monthly | Includes | Cost to serve |
|------|---------|----------|---------------|
| Free | $0 | 5 sessions, Snap only | ~$0.40 |
| Pro | $29 | Unlimited, all lanes | ~$60-120 |
| Team | $49/seat | + shared prompts, analytics | ~$60-120 |

### Cost Optimization Levers (Phase 10)

1. **CO1 Prompt caching**: 50-70% reduction on Solve lane input tokens
2. **CO2 RAG-filtered context**: Send only relevant turns (reduces 16K → ~6K avg)
3. **CO3 Router split**: Route easy Solve queries to Cerebras ($0.001/query vs $0.10)
4. **Combined effect**: $3.04 → $1.20 per session (60% reduction)

---

## Part 10: Success Metrics

### Latency (measured by L6 instrumentation)

- [ ] Mic → STT partial: <200ms (Deepgram Nova-3 streaming)
- [ ] STT stable → Snap response complete: <300ms
- [ ] STT stable → Solve first token: <800ms
- [ ] STT stable → Think progress indicator: <2s
- [ ] Overlay render latency (daemon→pixels): <10ms (Unix socket)
- [ ] Intent classifier decision: <5ms
- [ ] End-to-end mic→first-visible-token (Snap): <500ms

### Correctness

- [ ] Intent classifier accuracy: >85% on held-out test set
- [ ] Follow-up classifier accuracy: >90% (refinement vs new)
- [ ] Lane upgrade improves answer quality in >80% of cases
- [ ] Patch-mode diffs are semantically correct >95% of time

### Reliability

- [ ] Audio recovers from device disconnect: <2s
- [ ] Deepgram WebSocket reconnects: <3s (L1)
- [ ] No message loss on crash (WAL mode)
- [ ] Graceful degradation: Snap-only mode if Solve/Think unavailable

### Cost

- [ ] Average session cost: <$2 (with CO1-CO3)
- [ ] Snap query cost: <$0.002 per query
- [ ] No runaway costs (rate limiters + token budgets)

### UX

- [ ] Three-lane badge visible and accurate in overlay (R5)
- [ ] Patch diffs render correctly with highlight+fade (F7)
- [ ] Progressive enhancement: Snap answer visible before Solve replaces it (R2)
- [ ] Manual overrides work within 100ms (R3)

---

## Part 11: Open Questions (decisions needed before implementation)

1. **L5 Unix socket vs stdin/stdout** (before Phase 0): The V2 plan uses stdin/stdout for overlay IPC. V3 specifies Unix domain socket for <10ms hot path. Decision: do we keep stdin/stdout as fallback for Windows (which lacks Unix sockets), or use named pipes on Windows?

2. **R1 classifier model** (before Phase 4): Rule-based only (zero latency, lower accuracy) vs rule-based + small embedding model (5ms, higher accuracy)? The embedding model adds a ~50MB ONNX file to the bundle.

3. **F5 patch format** (before Phase 4 Batch 4C): Should PATCH/KEEP/MODIFY/ADD/REMOVE be a structured JSON response (reliable parsing) or inline markdown annotations (more natural for LLM)? JSON requires structured output mode; markdown requires regex parsing.

4. **CM3 summarization model** (before Phase 5): Use cheapest model (Cerebras) for epoch summarization, or use Solve-lane model (Claude) for higher quality? Cost difference: $0.001 vs $0.05 per summarization.

5. **CO3 difficulty classifier** (before Phase 10): How to determine "easy" vs "hard" within Solve lane? Options: (a) token count heuristic, (b) keyword rules, (c) same embedding classifier as R1 with finer granularity.

---

## Part 12: Index of Appendix Docs (on uno)

All at `/Users/uno/Downloads/cue/docs/reviews/`:

| File | Lines | Content |
|------|-------|---------|
| `CUE-MASTER-PORT-PLAN-V2.md` | 1895 | Previous V2 plan with 97 tasks (source of truth for task IDs) |
| `CUE-CURRENT-STATE.md` | 486 | Code audit: 9 DONE / 22 PARTIAL / 63 NOT STARTED |
| `CUE-DESIGN-01-STEALTH-WINDOWS.md` | 1300 | 15 stealth tasks (B1.x series) |
| `CUE-DESIGN-02-AUDIO-STT.md` | 1648 | 14 audio/STT tasks (B2.x series) |
| `CUE-DESIGN-03-LLM-PROMPTS-RAG.md` | 1791 | 28 LLM/prompts/RAG tasks (B3-B5 series) |
| `CUE-DESIGN-04-UX-OPS-DEV.md` | 1486 | 37 UX/ops tasks (B6-B10 series) |
| `CUE-BLUEY-V3-PLAN-SKELETON.md` | THIS | V3 skeleton (you are here) |

---

*Skeleton only. Detail will be filled in Step 2 (agents fill phases 0-4) and Step 3 (agents fill phases 5-10 + appendices).*
