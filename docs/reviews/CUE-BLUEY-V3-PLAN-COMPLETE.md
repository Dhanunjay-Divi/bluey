# bluey Master Plan V3 — COMPLETE

**Version**: 3.0 final
**Date**: 2026-05-12
**Architecture**: Hybrid native overlay + cue-daemon + Tauri 2 + React 19 dashboard
**Status**: codex-ready

**Total**: 8180 lines across 4 integrated parts:
  1. SKELETON (structure + task index + cost model + timeline)
  2. PART A — Phases 0-4 (foundation → reasoning, 53 tasks)
  3. PART B — Phases 5-10 (memory → cost optimization)
  4. PART C — APPENDICES (architecture/competitive/cost/metrics/risks/providers/glossary)

---

## NAVIGATION

- [Skeleton & task index →](#part-1-skeleton)
- [Phase 0-4 execution plan →](#part-a-phases-0-4)
- [Phase 5-10 execution plan →](#part-b-phases-5-10)
- [Architecture + competitive + cost + metrics + risks + providers →](#part-c-appendices)

---

---

# Part 1 — SKELETON

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


---

# Part A — PHASES 0-4 (Execution plan with code sketches)

# bluey V3 — Phases 0-4 Detailed Implementation Plan

**Scope**: Foundation → Dashboard Shell → Session UX → Listening Upgrade → Reasoning Upgrade
**Total duration**: ~13 weeks solo
**Date**: 2026-05-12

---

# Phase 0 — Foundation (~2 weeks solo)

## Entry criteria
- Repo cloned, `cargo build` passes on all 3 crates
- Node 20+ and npm available for Tauri frontend
- Rust 1.78+ with `cargo-tauri` CLI installed
- Native overlay binaries compile (Swift on macOS, C on Windows)

## Exit criteria
- `cargo tauri dev` launches Tauri window alongside existing daemon
- SQLite DB created at `~/.local/share/bluey/sessions.db` with sessions + messages tables
- Unix domain socket IPC proven: daemon sends JSON frame, overlay receives and renders
- Existing overlay still works via new socket path
- `cargo test` passes with ≥5 new integration tests

## Batch structure
Ships as 2 PRs:
- **PR1**: D0.1 + D0.2 + D0.3 + N0.1 (Tauri scaffold + IPC)
- **PR2**: C0.1 + D0.4 + L5 (Session model + overlay protocol upgrade + Unix socket)

## Tasks (in execution order)

### Task D0.1 🔴 [M] Tauri 2 Scaffold
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: —
**Summary**: Add Tauri 2 to the existing Cargo workspace. Create `crates/cue-dashboard/` with `tauri.conf.json`, a minimal React 19 + Vite frontend in `crates/cue-dashboard/ui/`, and wire it as a workspace member. The daemon remains a separate binary; Tauri manages only the dashboard webview.
**Design source**: CUE-DESIGN-01-STEALTH-WINDOWS.md §B1.1, CUE-BLUEY-V3-PLAN-SKELETON.md Part 2 Architecture Diagram
**Code sketch**:
```rust
// crates/cue-dashboard/src/main.rs
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .invoke_handler(tauri::generate_handler![
            commands::get_daemon_status,
            commands::get_sessions,
        ])
        .run(tauri::generate_context!())
        .expect("error running tauri application");
}

mod commands {
    #[tauri::command]
    pub async fn get_daemon_status() -> Result<String, String> {
        // Connect to daemon TCP 57321, send Status request
        let resp = cue_core::ipc::send_request(
            cue_core::ipc::DaemonRequest::Status
        ).await.map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&resp).unwrap())
    }

    #[tauri::command]
    pub async fn get_sessions() -> Result<String, String> {
        let resp = cue_core::ipc::send_request(
            cue_core::ipc::DaemonRequest::SessionsList
        ).await.map_err(|e| e.to_string())?;
        Ok(serde_json::to_string(&resp).unwrap())
    }
}
```
```json
// crates/cue-dashboard/tauri.conf.json (key fields)
{
  "productName": "bluey",
  "identifier": "com.bluey.dashboard",
  "build": { "frontendDist": "../ui/dist" },
  "app": {
    "windows": [{
      "title": "bluey",
      "width": 420,
      "height": 700,
      "visible": false,
      "decorations": false
    }],
    "security": { "csp": "default-src 'self'; script-src 'self'" }
  }
}
```
**Acceptance criteria**:
- `cargo tauri build` produces a single binary containing the webview
- `cargo tauri dev` opens a window showing "bluey dashboard" placeholder
- Workspace `Cargo.toml` lists `cue-dashboard` as a member
- No regressions: `cargo build -p cue-daemon` and `cargo build -p cue-cli` still pass
**Verification commands**:
- `cargo tauri dev` — window appears
- `cargo build --workspace` — all crates compile
- `ls crates/cue-dashboard/tauri.conf.json` — exists
**Risks + mitigations**:
- Tauri 2 requires specific Rust toolchain features → pin `tauri = "2"` in Cargo.toml, test on CI

---

### Task D0.2 🔴 [M] Tauri↔Daemon IPC
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1
**Summary**: Implement bidirectional communication between the Tauri dashboard and the running cue-daemon. Uses existing TCP IPC on port 57321 for commands (invoke → daemon) and adds Tauri event emission for daemon→dashboard push (streaming tokens, status changes). Reuses `DaemonRequest`/`DaemonResponse` from cue-core.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 2 (Tauri invoke/events COLD PATH)
**Code sketch**:
```rust
// crates/cue-dashboard/src/ipc_bridge.rs
use tauri::{AppHandle, Emitter, Manager};
use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct DaemonBridge {
    app: AppHandle,
}

impl DaemonBridge {
    pub async fn connect_event_stream(&self) -> anyhow::Result<()> {
        let stream = TcpStream::connect("127.0.0.1:57321").await?;
        let (reader, mut writer) = stream.into_split();
        // Subscribe to daemon events
        let subscribe = serde_json::to_string(
            &cue_core::ipc::DaemonRequest::SubscribeEvents
        )?;
        writer.write_all(format!("{}\n", subscribe).as_bytes()).await?;

        let app = self.app.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                app.emit("daemon-event", &line).ok();
            }
        });
        Ok(())
    }
}
```
**Acceptance criteria**:
- `invoke('get_daemon_status')` from React returns valid JSON when daemon is running
- `invoke('get_daemon_status')` returns error string when daemon is not running
- Tauri event `daemon-event` fires when daemon pushes a status change
- Round-trip latency <30ms measured via timestamp in response
**Verification commands**:
- Start daemon: `cargo run -p cue-daemon &`
- Start dashboard: `cargo tauri dev`
- Browser console: `window.__TAURI__.invoke('get_daemon_status')` returns JSON
**Risks + mitigations**:
- Daemon not running when dashboard starts → show "daemon offline" banner, retry every 2s

---

### Task D0.3 🔴 [S] Single-Binary Architecture Confirmation
**Layer**: INFRA
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1
**Summary**: Write `ARCHITECTURE.md` documenting the hybrid architecture. Confirm that `cargo tauri build` produces a single distributable binary that bundles the webview. Verify the daemon remains a separate process launched by the CLI or auto-started.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 2
**Code sketch**:
```markdown
<!-- ARCHITECTURE.md (excerpt) -->
# bluey Architecture

## Process Model
- `cue-daemon`: Long-running background process. Owns audio, STT, LLM, sessions.
- `cue-dashboard`: Tauri 2 webview. Settings, sessions, prompts UI. Cold-path IPC.
- `cue-overlay`: Native Swift/C floating panel. Hot-path IPC via Unix socket.
- `cue-cli`: Thin client. Commands forwarded to daemon over TCP.

## IPC Summary
| Path | Transport | Latency | Use |
|------|-----------|---------|-----|
| Overlay↔Daemon | Unix domain socket | <10ms | Streaming tokens |
| Dashboard↔Daemon | Tauri invoke (TCP 57321) | 10-30ms | Commands, settings |
| CLI↔Daemon | TCP 57321 | 10-30ms | User commands |
```
**Acceptance criteria**:
- `ARCHITECTURE.md` exists at repo root with process model + IPC table
- `cargo tauri build` output is a single `.app` (macOS) or `.exe` (Windows)
- Dashboard binary size <30MB (Tauri webview + React bundle)
**Verification commands**:
- `cat ARCHITECTURE.md | grep "Unix domain socket"`
- `ls target/release/bundle/macos/*.app` — exists after `cargo tauri build`
**Risks + mitigations**:
- None significant — documentation task

---

### Task N0.1 🟢 [S] Audit Overlay for Tauri-Overlap
**Layer**: NATIVE
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.2
**Summary**: Audit the native overlay (2463 LOC Swift, 996 LOC C) to identify features that overlap with the new Tauri dashboard. Produce a scope document listing what stays in native overlay (real-time display, stealth, always-on-top) vs what moves to dashboard (settings, session management, prompt editing).
**Design source**: CUE-CURRENT-STATE.md §4 Native Modules
**Code sketch**:
```markdown
<!-- docs/OVERLAY-SCOPE-AUDIT.md -->
# Overlay Scope Audit

## STAYS in Native Overlay (hot path, stealth-critical)
- Card feed rendering (streaming tokens)
- Always-on-top + capture exclusion
- Collapsed pill mode
- Drag-to-move + resize
- Mode indicator badge (new: R5)
- Patch-mode diff rendering (new: F7)

## MOVES to Dashboard (cold path, non-stealth)
- Model picker dropdown → Dashboard settings page
- Instructions editor → Dashboard prompts page
- File attachment UI → Dashboard context page
- Theme toggle → Dashboard settings (sync to overlay via IPC)

## SHARED (both render, dashboard is source of truth)
- Session ID display
- Current mode indicator
```
**Acceptance criteria**:
- `docs/OVERLAY-SCOPE-AUDIT.md` exists with ≥10 items categorized
- No code changes to overlay in this task (audit only)
**Verification commands**:
- `test -f docs/OVERLAY-SCOPE-AUDIT.md && echo OK`
**Risks + mitigations**:
- None — read-only audit

---

### Task C0.1 🔴 [M] Session-ID Model in SQLite
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1
**Summary**: Replace JSON file persistence with SQLite (WAL mode) for sessions and messages. Each session has a UUID, title, created_at, updated_at, and state (active/archived). Messages store role, content, lane (snap/solve/think), provider, latency_ms, token counts. This is the foundation that CM1 (Phase 5) extends with turn model and lane metadata.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 6 (C0.1 → CM1 extension path)
**Code sketch**:
```rust
// crates/cue-daemon/src/storage/sqlite.rs
use rusqlite::{Connection, params};
use uuid::Uuid;

pub struct SessionStore {
    conn: Connection,
}

impl SessionStore {
    pub fn new(db_path: &std::path::Path) -> anyhow::Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(include_str!("../../migrations/001_sessions.sql"))?;
        Ok(Self { conn })
    }

    pub fn create_session(&self, title: &str) -> anyhow::Result<String> {
        let id = Uuid::new_v4().to_string();
        self.conn.execute(
            "INSERT INTO sessions (id, title, state, created_at, updated_at)
             VALUES (?1, ?2, 'active', datetime('now'), datetime('now'))",
            params![id, title],
        )?;
        Ok(id)
    }

    pub fn add_message(&self, session_id: &str, msg: &Message) -> anyhow::Result<()> {
        self.conn.execute(
            "INSERT INTO messages (id, session_id, role, content, lane, provider, latency_ms, input_tokens, output_tokens, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, datetime('now'))",
            params![
                Uuid::new_v4().to_string(), session_id,
                msg.role, msg.content, msg.lane, msg.provider,
                msg.latency_ms, msg.input_tokens, msg.output_tokens
            ],
        )?;
        Ok(())
    }

    pub fn list_sessions(&self) -> anyhow::Result<Vec<SessionSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, title, state, created_at, updated_at FROM sessions ORDER BY updated_at DESC"
        )?;
        let rows = stmt.query_map([], |row| Ok(SessionSummary {
            id: row.get(0)?, title: row.get(1)?, state: row.get(2)?,
            created_at: row.get(3)?, updated_at: row.get(4)?,
        }))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }
}

pub struct Message {
    pub role: String,
    pub content: String,
    pub lane: Option<String>,
    pub provider: Option<String>,
    pub latency_ms: Option<i64>,
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
}

pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub state: String,
    pub created_at: String,
    pub updated_at: String,
}
```
```sql
-- crates/cue-daemon/migrations/001_sessions.sql
CREATE TABLE IF NOT EXISTS sessions (
    id TEXT PRIMARY KEY,
    title TEXT NOT NULL,
    state TEXT NOT NULL DEFAULT 'active' CHECK(state IN ('active','archived')),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role TEXT NOT NULL CHECK(role IN ('user','assistant','system')),
    content TEXT NOT NULL,
    lane TEXT CHECK(lane IN ('snap','solve','think')),
    provider TEXT,
    latency_ms INTEGER,
    input_tokens INTEGER,
    output_tokens INTEGER,
    created_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_messages_session ON messages(session_id, created_at);
```
**Acceptance criteria**:
- `SessionStore::new()` creates DB file with WAL mode enabled
- `create_session` + `list_sessions` round-trips correctly
- `add_message` stores lane metadata (snap/solve/think)
- Existing JSON persistence still works (migration is additive, not replacing yet)
- 3 unit tests: create, list, add_message
**Verification commands**:
- `cargo test -p cue-daemon storage::sqlite`
- `sqlite3 ~/.local/share/bluey/sessions.db ".tables"` — shows sessions, messages
**Risks + mitigations**:
- SQLite locking under concurrent access → WAL mode handles this; single-writer pattern in daemon

---

### Task D0.4 🟡 [S] Overlay Protocol Upgrade
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: C0.1
**Summary**: Extend the `OverlayCommand` enum in cue-core with `SessionChanged { id, title }` and `ModeIndicator { lane, model }` variants. The overlay renders a small badge showing current lane. This prepares for R5 (Phase 4) without requiring the full routing system yet.
**Design source**: CUE-CURRENT-STATE.md §2 cue-core overlay module, Skeleton R5 description
**Code sketch**:
```rust
// crates/cue-core/src/overlay.rs — additions to existing enum
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum OverlayCommand {
    // ... existing variants ...
    SessionChanged { session_id: String, title: String },
    ModeIndicator { lane: Lane, model: String },
    PatchDiff { blocks: Vec<DiffBlock> }, // prep for F7
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Lane { Snap, Solve, Think }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffBlock {
    pub action: DiffAction,
    pub content: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum DiffAction { Keep, Modify, Add, Remove }
```
**Acceptance criteria**:
- `OverlayCommand::SessionChanged` serializes to `{"type":"SessionChanged","session_id":"...","title":"..."}`
- `OverlayCommand::ModeIndicator` serializes with lane as lowercase string
- Existing overlay commands still deserialize correctly (backward compat)
- 2 unit tests for new variant serialization
**Verification commands**:
- `cargo test -p cue-core overlay`
**Risks + mitigations**:
- Native overlay ignores unknown commands (already does via serde `deny_unknown_fields` off) → safe

---

### Task L5 🔴 [M] Unix Domain Socket Daemon↔Overlay
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1
**Summary**: Replace stdin/stdout IPC between daemon and native overlay with a Unix domain socket (macOS/Linux) or named pipe (Windows). This enables <10ms streaming latency for the hot path. The daemon listens on `/tmp/bluey-overlay.sock`, overlay connects on launch. Framing: newline-delimited JSON (same as current stdin/stdout protocol).
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 2 (Unix Domain Socket HOT PATH <10ms), L5 task description
**Code sketch**:
```rust
// crates/cue-daemon/src/overlay_socket.rs
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

const SOCKET_PATH: &str = "/tmp/bluey-overlay.sock";

pub struct OverlaySocket {
    writer: Option<tokio::io::WriteHalf<UnixStream>>,
}

impl OverlaySocket {
    pub async fn listen(tx: tokio::sync::mpsc::Sender<cue_core::overlay::OverlayEvent>) -> anyhow::Result<Self> {
        let _ = std::fs::remove_file(SOCKET_PATH);
        let listener = UnixListener::bind(SOCKET_PATH)?;
        let (stream, _) = listener.accept().await?;
        let (reader, writer) = tokio::io::split(stream);

        // Spawn reader for overlay→daemon events
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                if let Ok(event) = serde_json::from_str(&line) {
                    let _ = tx.send(event).await;
                }
            }
        });

        Ok(Self { writer: Some(writer) })
    }

    pub async fn send_command(&mut self, cmd: &cue_core::overlay::OverlayCommand) -> anyhow::Result<()> {
        if let Some(ref mut w) = self.writer {
            let json = serde_json::to_string(cmd)?;
            w.write_all(format!("{}\n", json).as_bytes()).await?;
        }
        Ok(())
    }
}
```
**Acceptance criteria**:
- Daemon creates `/tmp/bluey-overlay.sock` on startup
- Overlay connects and receives `OverlayCommand` JSON frames
- Measured latency: send→receive <5ms on localhost (use timestamp echo test)
- Fallback: if socket connection fails, daemon logs warning and overlay can still use stdin/stdout
- Windows: uses `\\.\pipe\bluey-overlay` named pipe (same framing)
**Verification commands**:
- `cargo test -p cue-daemon overlay_socket` — integration test with mock client
- `echo '{"type":"Status"}' | socat - UNIX-CONNECT:/tmp/bluey-overlay.sock` — gets response
**Risks + mitigations**:
- Socket file left behind on crash → `remove_file` on startup (already in code sketch)
- Windows lacks Unix sockets → use `tokio::net::windows::named_pipe` behind `#[cfg(windows)]`

---

## Phase deliverable
- A user can run `cargo tauri dev` and see a placeholder dashboard window
- The daemon stores sessions in SQLite with lane metadata
- The overlay communicates via Unix socket with <10ms latency
- All existing CLI + overlay functionality is preserved
- Codex review should check: no regressions in `cargo build --workspace`, SQLite WAL mode, socket cleanup on exit


---

# Phase 1 — Dashboard Shell (~1 week)

## Entry criteria
- Phase 0 complete: Tauri binary builds, IPC proven, SQLite sessions table exists
- React 19 + Vite scaffold in `crates/cue-dashboard/ui/` compiles

## Exit criteria
- Cmd+Shift+D toggles dashboard window visibility
- Sidebar navigation renders 4 placeholder pages (Sessions, Settings, Prompts, Dev)
- Daemon status displayed in dashboard header (online/offline badge)
- Theme syncs between dashboard and system preference
- CSP configured, error boundaries on all routes

## Batch structure
Ships as 1 PR:
- **PR3**: B6.3 + B6.1 + B6.4 + B6.5 + B6.6 + B7.8 + B7.9

## Tasks (in execution order)

### Task B6.3 🔴 [M] Dashboard Sidebar Navigation
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1, D0.2
**Summary**: Build the main dashboard layout with a collapsible sidebar using Radix UI primitives. Four nav items: Sessions, Settings, Prompts, Dev Tools. React Router for client-side routing. Header shows daemon connection status badge.
**Design source**: CUE-DESIGN-04-UX-OPS-DEV.md §B6.3
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/App.tsx
import { BrowserRouter, Routes, Route } from 'react-router-dom';
import { Sidebar } from './components/Sidebar';
import { DaemonStatus } from './components/DaemonStatus';
import { SessionsPage } from './pages/Sessions';
import { SettingsPage } from './pages/Settings';

export function App() {
  return (
    <BrowserRouter>
      <div className="flex h-screen bg-background text-foreground">
        <Sidebar />
        <main className="flex-1 flex flex-col overflow-hidden">
          <header className="h-12 flex items-center px-4 border-b">
            <DaemonStatus />
          </header>
          <Routes>
            <Route path="/" element={<SessionsPage />} />
            <Route path="/settings" element={<SettingsPage />} />
            <Route path="/prompts" element={<div>Prompts</div>} />
            <Route path="/dev" element={<div>Dev Tools</div>} />
          </Routes>
        </main>
      </div>
    </BrowserRouter>
  );
}
```
```typescript
// crates/cue-dashboard/ui/src/components/Sidebar.tsx
import { NavLink } from 'react-router-dom';
import { MessageSquare, Settings, Sparkles, Terminal } from 'lucide-react';

const NAV_ITEMS = [
  { to: '/', icon: MessageSquare, label: 'Sessions' },
  { to: '/settings', icon: Settings, label: 'Settings' },
  { to: '/prompts', icon: Sparkles, label: 'Prompts' },
  { to: '/dev', icon: Terminal, label: 'Dev Tools' },
] as const;

export function Sidebar() {
  return (
    <nav className="w-48 border-r flex flex-col py-2" role="navigation">
      {NAV_ITEMS.map(({ to, icon: Icon, label }) => (
        <NavLink key={to} to={to}
          className={({ isActive }) =>
            `flex items-center gap-2 px-3 py-2 text-sm rounded-md mx-2 ${
              isActive ? 'bg-accent text-accent-foreground' : 'hover:bg-muted'
            }`
          }>
          <Icon size={16} />
          {label}
        </NavLink>
      ))}
    </nav>
  );
}
```
**Acceptance criteria**:
- Sidebar renders 4 navigation items with icons
- Clicking nav item changes route and highlights active item
- Layout is responsive: sidebar collapses to icons at <600px width
- Keyboard navigation works (Tab through items, Enter to select)
**Verification commands**:
- `cd crates/cue-dashboard/ui && npm run build` — no errors
- `cargo tauri dev` — sidebar visible with 4 items
**Risks + mitigations**:
- React Router in Tauri webview → use `MemoryRouter` if `BrowserRouter` has issues with file:// protocol

---

### Task B6.1 🟡 [S] Hotkey System (Cmd+Shift+D)
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (macOS overlay has Cmd+Shift+B)
**Depends on**: D0.1
**Summary**: Register global hotkey Cmd+Shift+D (Ctrl+Shift+D on Windows/Linux) to toggle dashboard window visibility. Uses `tauri-plugin-global-shortcut`. Also registers Alt+D and Alt+F as prep for R3 (manual lane override) — these emit events but don't act yet.
**Design source**: CUE-CURRENT-STATE.md §B6.1 (existing hotkeys in overlay), Skeleton R3
**Code sketch**:
```rust
// crates/cue-dashboard/src/main.rs — in setup
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .setup(|app| {
            let toggle_shortcut = "CommandOrControl+Shift+D".parse::<Shortcut>()?;
            app.global_shortcut().on_shortcut(toggle_shortcut, |app, _, event| {
                if event.state == ShortcutState::Pressed {
                    if let Some(window) = app.get_webview_window("main") {
                        if window.is_visible().unwrap_or(false) {
                            window.hide().ok();
                        } else {
                            window.show().ok();
                            window.set_focus().ok();
                        }
                    }
                }
            })?;
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error running tauri");
}
```
**Acceptance criteria**:
- Cmd+Shift+D toggles dashboard window from any app
- Window appears focused when shown, releases focus when hidden
- Alt+D emits `lane-override-think` event (no handler yet)
- Alt+F emits `lane-override-snap` event (no handler yet)
- No conflict with existing Cmd+Shift+B (overlay toggle)
**Verification commands**:
- Press Cmd+Shift+D with dashboard running → window toggles
- `cargo test -p cue-dashboard` — shortcut registration doesn't panic
**Risks + mitigations**:
- Hotkey conflict with other apps → make configurable in Phase 6 (B6.2)

---

### Task B6.4 🟡 [S] Theme Management
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ✅ DONE (overlay has dark/light) → extend to dashboard
**Depends on**: D0.2
**Summary**: Sync theme between system preference, dashboard, and overlay. Dashboard uses CSS custom properties with `prefers-color-scheme` media query. On toggle, sends `ThemeChanged` command to overlay via daemon IPC.
**Design source**: CUE-CURRENT-STATE.md §B6.4 (overlay theme already works)
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/hooks/useTheme.ts
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';

type Theme = 'light' | 'dark' | 'system';

export function useTheme() {
  const [theme, setTheme] = useState<Theme>('system');

  useEffect(() => {
    const resolved = theme === 'system'
      ? (window.matchMedia('(prefers-color-scheme: dark)').matches ? 'dark' : 'light')
      : theme;
    document.documentElement.classList.toggle('dark', resolved === 'dark');
    invoke('set_theme', { theme: resolved }).catch(() => {});
  }, [theme]);

  return { theme, setTheme };
}
```
**Acceptance criteria**:
- Dashboard respects system dark/light preference on launch
- Manual toggle persists across restarts (stored in localStorage)
- Overlay receives `ThemeChanged` command when dashboard theme changes
**Verification commands**:
- Toggle macOS appearance → dashboard follows
- `cargo tauri dev` in dark mode → dark background renders
**Risks + mitigations**:
- None significant

---

### Task B6.5 🟡 [M] rAF Streaming Buffer
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.2
**Summary**: Implement `useStreamBuffer` React hook that coalesces incoming LLM tokens via `requestAnimationFrame` to prevent excessive re-renders. Tokens arrive at up to 60Hz from daemon events; the hook batches them into single state updates per animation frame.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §5 Streaming Response Handling (React side)
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/hooks/useStreamBuffer.ts
import { useRef, useState, useCallback } from 'react';

export function useStreamBuffer() {
  const bufferRef = useRef('');
  const rafRef = useRef<number | null>(null);
  const [displayText, setDisplayText] = useState('');

  const append = useCallback((chunk: string) => {
    bufferRef.current += chunk;
    if (rafRef.current === null) {
      rafRef.current = requestAnimationFrame(() => {
        setDisplayText(prev => prev + bufferRef.current);
        bufferRef.current = '';
        rafRef.current = null;
      });
    }
  }, []);

  const reset = useCallback(() => {
    setDisplayText('');
    bufferRef.current = '';
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current);
      rafRef.current = null;
    }
  }, []);

  return { displayText, append, reset };
}
```
**Acceptance criteria**:
- Hook batches 60 token events/sec into ~60 state updates/sec (1 per rAF)
- No dropped tokens: final `displayText` matches concatenation of all inputs
- `reset()` clears buffer and cancels pending rAF
- Unit test: feed 100 tokens synchronously, verify single rAF flush
**Verification commands**:
- `cd crates/cue-dashboard/ui && npm test -- --grep useStreamBuffer`
**Risks + mitigations**:
- React 19 concurrent mode may batch differently → useRef ensures no lost tokens

---

### Task B6.6 🟢 [S] React.memo MessageRow
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B6.3
**Summary**: Create a memoized `MessageRow` component with a custom comparator that only re-renders when content or lane changes. Prevents O(n) re-renders of the message list when new tokens stream in.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §5 (rAF coalescing pattern from NativelyInterface.tsx)
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/components/MessageRow.tsx
import { memo } from 'react';

interface MessageRowProps {
  id: string;
  role: 'user' | 'assistant' | 'system';
  content: string;
  lane?: 'snap' | 'solve' | 'think';
  isStreaming: boolean;
}

export const MessageRow = memo(function MessageRow({ role, content, lane, isStreaming }: MessageRowProps) {
  return (
    <div className={`px-4 py-3 ${role === 'assistant' ? 'bg-muted/50' : ''}`}>
      {lane && (
        <span className={`text-xs font-mono px-1.5 py-0.5 rounded ${
          lane === 'snap' ? 'bg-green-500/20 text-green-400' :
          lane === 'solve' ? 'bg-blue-500/20 text-blue-400' :
          'bg-purple-500/20 text-purple-400'
        }`}>{lane}</span>
      )}
      <div className="mt-1 text-sm whitespace-pre-wrap">{content}</div>
      {isStreaming && <span className="inline-block w-2 h-4 bg-foreground/60 animate-pulse" />}
    </div>
  );
}, (prev, next) => prev.content === next.content && prev.isStreaming === next.isStreaming);
```
**Acceptance criteria**:
- MessageRow does not re-render when sibling messages update (verified via React DevTools profiler)
- Lane badge renders with correct color per lane
- Streaming cursor animates during active generation
**Verification commands**:
- `npm test -- --grep MessageRow`
**Risks + mitigations**:
- None

---

### Task B7.8 🔴 [S] CSP Configuration
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: D0.1
**Summary**: Configure Content Security Policy in `tauri.conf.json` to restrict the webview. Allow only self-origin scripts, styles, and connections to localhost (daemon IPC). Block all external network from the webview — all cloud API calls go through the daemon.
**Design source**: CUE-DESIGN-04-UX-OPS-DEV.md §B7.8
**Code sketch**:
```json
// In tauri.conf.json → app.security
{
  "csp": "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; connect-src 'self' http://127.0.0.1:57321 ws://127.0.0.1:57321; img-src 'self' data:; font-src 'self'"
}
```
**Acceptance criteria**:
- Webview cannot load external scripts (test: inject `<script src="https://evil.com">` → blocked)
- Webview can connect to localhost:57321 (daemon IPC works)
- Inline styles allowed (needed for dynamic theming)
- No CSP violations in console during normal operation
**Verification commands**:
- `cargo tauri dev` → open devtools → Console shows no CSP errors
- Attempt `fetch('https://httpbin.org/get')` in console → blocked
**Risks + mitigations**:
- `unsafe-inline` for styles is acceptable for Tauri apps (no user-generated content)

---

### Task B7.9 🟢 [S] Error Boundaries
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B6.3
**Summary**: Wrap each route in a React error boundary that catches render errors and displays a recovery UI instead of a white screen. Log errors to daemon via IPC for crash reporting.
**Design source**: CUE-DESIGN-04-UX-OPS-DEV.md §B7.9
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/components/ErrorBoundary.tsx
import { Component, type ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';

interface Props { children: ReactNode; fallback?: ReactNode; }
interface State { error: Error | null; }

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null };

  static getDerivedStateFromError(error: Error) { return { error }; }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    invoke('log_error', { message: error.message, stack: info.componentStack }).catch(() => {});
  }

  render() {
    if (this.state.error) {
      return this.props.fallback ?? (
        <div className="p-4 text-center">
          <p className="text-destructive font-medium">Something went wrong</p>
          <button onClick={() => this.setState({ error: null })}
            className="mt-2 text-sm underline">Try again</button>
        </div>
      );
    }
    return this.props.children;
  }
}
```
**Acceptance criteria**:
- Throwing component in /settings doesn't crash /sessions route
- Error boundary shows "Something went wrong" with retry button
- Error logged to daemon (visible in daemon logs)
**Verification commands**:
- Add `throw new Error('test')` in SettingsPage → boundary catches it
- `cargo tauri dev` → navigate to /settings → error UI shows, /sessions still works
**Risks + mitigations**:
- None

---

## Phase deliverable
- User presses Cmd+Shift+D → dashboard window appears with sidebar navigation
- Dashboard shows daemon online/offline status
- Theme follows system preference
- Streaming buffer hook ready for Phase 4 integration
- Codex review should check: CSP blocks external requests, error boundaries on all routes, no console errors


---

# Phase 2 — Session UX (~2 weeks)

## Entry criteria
- Phase 1 complete: dashboard opens via hotkey, sidebar nav works, IPC bridge functional
- SQLite session store (C0.1) operational

## Exit criteria
- User can create, switch, and archive sessions from dashboard
- Messages persist across daemon restarts (SQLite WAL)
- Overlay shows current session title and updates on switch
- Graceful shutdown saves in-flight state

## Batch structure
Ships as 1 PR:
- **PR4**: B6.3.1 + B6.3.2 + B6.3.3 + B5.5 + B10.8

## Tasks (in execution order)

### Task B6.3.1 🔴 [M] Session List Page
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: C0.1, B6.3
**Summary**: Dashboard page showing all sessions sorted by last activity. Each row shows title, lane distribution badge (how many snap/solve/think messages), relative timestamp, and archive button. "New Session" button at top. Fetches data via `invoke('get_sessions')`.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Phase 2 tasks
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/pages/Sessions.tsx
import { useEffect, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { useNavigate } from 'react-router-dom';

interface Session { id: string; title: string; state: string; updated_at: string; }

export function SessionsPage() {
  const [sessions, setSessions] = useState<Session[]>([]);
  const navigate = useNavigate();

  useEffect(() => {
    invoke<string>('get_sessions').then(json => setSessions(JSON.parse(json)));
  }, []);

  const createSession = async () => {
    const id = await invoke<string>('create_session', { title: 'New Session' });
    navigate(`/session/${id}`);
  };

  return (
    <div className="flex-1 overflow-y-auto p-4">
      <div className="flex justify-between items-center mb-4">
        <h1 className="text-lg font-semibold">Sessions</h1>
        <button onClick={createSession}
          className="px-3 py-1.5 text-sm bg-primary text-primary-foreground rounded-md">
          New Session
        </button>
      </div>
      <ul className="space-y-1">
        {sessions.map(s => (
          <li key={s.id} onClick={() => navigate(`/session/${s.id}`)}
            className="flex items-center justify-between px-3 py-2 rounded-md hover:bg-muted cursor-pointer">
            <span className="text-sm font-medium">{s.title}</span>
            <span className="text-xs text-muted-foreground">
              {new Date(s.updated_at).toLocaleDateString()}
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}
```
**Acceptance criteria**:
- Page loads sessions from SQLite via daemon IPC
- "New Session" creates a session and navigates to detail page
- Sessions sorted by `updated_at` descending
- Empty state shows "No sessions yet" message
**Verification commands**:
- `cargo tauri dev` → Sessions page shows list after creating via CLI
- Create 3 sessions via CLI → all appear in dashboard
**Risks + mitigations**:
- Large session lists → paginate at 50 items (defer to Phase 6)

---

### Task B6.3.2 🔴 [M] Session Detail Page
**Layer**: DASHBOARD
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B6.3.1
**Summary**: Shows message history for a session with lane badges per message. Uses `MessageRow` component from B6.6. Subscribes to daemon events for live streaming when session is active. Shows lane distribution summary at top.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md (B6.3.2 "Now shows lane badge per message")
**Code sketch**:
```typescript
// crates/cue-dashboard/ui/src/pages/SessionDetail.tsx
import { useEffect, useState } from 'react';
import { useParams } from 'react-router-dom';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { MessageRow } from '../components/MessageRow';
import { useStreamBuffer } from '../hooks/useStreamBuffer';

interface Msg { id: string; role: 'user'|'assistant'; content: string; lane?: string; }

export function SessionDetailPage() {
  const { id } = useParams<{ id: string }>();
  const [messages, setMessages] = useState<Msg[]>([]);
  const { displayText, append, reset } = useStreamBuffer();

  useEffect(() => {
    invoke<string>('get_session_messages', { sessionId: id })
      .then(json => setMessages(JSON.parse(json)));

    const unlisten = listen<string>('daemon-event', (event) => {
      const data = JSON.parse(event.payload);
      if (data.type === 'llm-token' && data.session_id === id) {
        append(data.text);
      } else if (data.type === 'llm-complete' && data.session_id === id) {
        setMessages(prev => [...prev, { id: data.msg_id, role: 'assistant', content: displayText, lane: data.lane }]);
        reset();
      }
    });
    return () => { unlisten.then(fn => fn()); };
  }, [id]);

  return (
    <div className="flex-1 flex flex-col overflow-hidden">
      <div className="flex-1 overflow-y-auto">
        {messages.map(m => (
          <MessageRow key={m.id} id={m.id} role={m.role}
            content={m.content} lane={m.lane as any} isStreaming={false} />
        ))}
        {displayText && (
          <MessageRow id="streaming" role="assistant"
            content={displayText} isStreaming={true} />
        )}
      </div>
    </div>
  );
}
```
**Acceptance criteria**:
- Navigating to `/session/:id` loads message history
- Each message shows lane badge (snap=green, solve=blue, think=purple)
- Live streaming tokens appear in real-time via `useStreamBuffer`
- Scroll-to-bottom on new messages
**Verification commands**:
- Create session, send question via CLI, verify messages appear in dashboard
- `npm test -- --grep SessionDetail`
**Risks + mitigations**:
- Large message histories → virtualize list in Phase 6 (B6.7)

---

### Task B6.3.3 🟡 [S] Session Switching Logic
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: C0.1, D0.4
**Summary**: When user switches sessions in dashboard, daemon updates its active session pointer and sends `SessionChanged` command to overlay. Overlay updates its title display. CLI `sessions switch <id>` also triggers the same flow.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md (B6.3.3 "overlay + dashboard sync")
**Code sketch**:
```rust
// crates/cue-daemon/src/app.rs — new handler
async fn handle_switch_session(&self, session_id: &str) -> anyhow::Result<()> {
    // Validate session exists
    let session = self.session_store.get_session(session_id)?;

    // Update daemon state
    *self.active_session_id.lock().await = Some(session_id.to_string());

    // Notify overlay
    self.overlay_socket.send_command(
        &cue_core::overlay::OverlayCommand::SessionChanged {
            session_id: session_id.to_string(),
            title: session.title.clone(),
        }
    ).await?;

    // Emit event for dashboard
    tracing::info!(session_id, "Switched active session");
    Ok(())
}
```
**Acceptance criteria**:
- Clicking a session in dashboard list makes it active
- Overlay receives `SessionChanged` and updates title display
- CLI `bluey sessions switch <id>` triggers same flow
- Switching session does not lose in-flight streaming (current generation continues)
**Verification commands**:
- Create 2 sessions → switch between them → overlay title updates
- `cargo test -p cue-daemon handle_switch_session`
**Risks + mitigations**:
- Race condition if switching during active generation → generation tagged with session_id at start, not affected by switch

---

### Task B5.5 🟡 [M] InterviewTranscriptBuffer with Persistence
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (conversation history exists, capped at 80 turns, no summarization)
**Depends on**: C0.1
**Summary**: Upgrade the existing `MeetingRecord.conversation` (80-turn cap) to a proper `TranscriptBuffer` that persists to SQLite and supports rolling summarization. Recent 5 Q&A pairs kept verbatim; older pairs compressed to summaries. This is the precursor to CM3 (Phase 5 epoch summarization).
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §5 InterviewTranscriptBuffer
**Code sketch**:
```rust
// crates/cue-daemon/src/transcript_buffer.rs
use std::collections::VecDeque;

pub struct TranscriptBuffer {
    recent_pairs: VecDeque<QAPair>,
    summaries: Vec<String>,
    max_recent: usize,
    session_id: String,
}

#[derive(Clone, Debug)]
pub struct QAPair {
    pub question: String,
    pub answer: String,
    pub timestamp_ms: u64,
}

impl TranscriptBuffer {
    pub fn new(session_id: String) -> Self {
        Self { recent_pairs: VecDeque::new(), summaries: Vec::new(), max_recent: 5, session_id }
    }

    pub fn add_pair(&mut self, pair: QAPair) {
        self.recent_pairs.push_back(pair);
        // Summarization deferred to Phase 5 (CM3) — for now just drop oldest
        while self.recent_pairs.len() > self.max_recent * 2 {
            self.recent_pairs.pop_front();
        }
    }

    /// Build context string for LLM prompt (~850 tokens max)
    pub fn get_context(&self) -> String {
        let mut ctx = String::with_capacity(2048);
        if !self.summaries.is_empty() {
            ctx.push_str("Previous discussion:\n");
            for s in &self.summaries { ctx.push_str(&format!("- {}\n", s)); }
            ctx.push('\n');
        }
        ctx.push_str("Recent exchanges:\n");
        for pair in &self.recent_pairs {
            ctx.push_str(&format!("Q: {}\nA: {}\n\n", pair.question, pair.answer));
        }
        ctx
    }
}
```
**Acceptance criteria**:
- Buffer stores Q&A pairs with timestamps
- `get_context()` returns formatted string <2KB
- Pairs persist to SQLite messages table (via session_id)
- Buffer loads from SQLite on session resume
**Verification commands**:
- `cargo test -p cue-daemon transcript_buffer`
- Add 10 pairs → `get_context()` returns last 10 (summarization deferred)
**Risks + mitigations**:
- No summarization yet → buffer grows unbounded for long sessions → hard cap at 10 pairs until CM3

---

### Task B10.8 🟡 [S] Graceful Shutdown with Session Save
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (shutdown exists, no in-flight tracking)
**Depends on**: C0.1
**Summary**: On SIGTERM/SIGINT, daemon flushes all pending SQLite writes, closes the overlay socket cleanly, and saves the active session's transcript buffer. Track in-flight LLM generations via a counter; wait up to 2s for them to complete before force-exit.
**Design source**: CUE-CURRENT-STATE.md §B10.8 (existing shutdown_daemon)
**Code sketch**:
```rust
// crates/cue-daemon/src/shutdown.rs
use std::sync::atomic::{AtomicU32, Ordering};
use tokio::signal;

static IN_FLIGHT: AtomicU32 = AtomicU32::new(0);

pub fn increment_in_flight() { IN_FLIGHT.fetch_add(1, Ordering::Relaxed); }
pub fn decrement_in_flight() { IN_FLIGHT.fetch_sub(1, Ordering::Relaxed); }

pub async fn graceful_shutdown(
    session_store: &crate::storage::sqlite::SessionStore,
    overlay: &mut crate::overlay_socket::OverlaySocket,
) {
    signal::ctrl_c().await.ok();
    tracing::info!("Shutdown signal received, draining in-flight requests...");

    // Wait up to 2s for in-flight generations
    let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(2);
    while IN_FLIGHT.load(Ordering::Relaxed) > 0 && tokio::time::Instant::now() < deadline {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }

    // Flush SQLite WAL
    session_store.checkpoint().ok();

    // Close overlay socket
    overlay.send_command(&cue_core::overlay::OverlayCommand::Shutdown).await.ok();

    // Remove socket file
    let _ = std::fs::remove_file("/tmp/bluey-overlay.sock");
    tracing::info!("Shutdown complete");
}
```
**Acceptance criteria**:
- `kill -TERM <daemon_pid>` → daemon waits for in-flight, then exits cleanly
- SQLite WAL is checkpointed (no data loss)
- Socket file removed on clean shutdown
- If in-flight takes >2s, force exit anyway
**Verification commands**:
- Start daemon, send long query, `kill -TERM` → query completes or times out, DB intact
- `cargo test -p cue-daemon shutdown`
**Risks + mitigations**:
- Crash before checkpoint → WAL mode ensures recovery on next open (SQLite handles this)

---

## Phase deliverable
- User can create sessions from dashboard, switch between them, see message history with lane badges
- Overlay reflects current session title
- Data persists across daemon restarts
- Codex review should check: SQLite WAL checkpoint on shutdown, no data loss on kill -9 (WAL recovery), session switch doesn't drop in-flight generation


---

# Phase 3 — Listening Upgrade (~3 weeks)

## Entry criteria
- Phase 0 complete (can run parallel with Phase 1-2)
- Native audio helpers compile (Swift SCK, C WASAPI)
- `cpal` and `webrtc-vad` crates resolve in workspace

## Exit criteria
- Two-stage VAD filters silence before STT billing
- Deepgram Nova-3 streaming WebSocket produces final transcripts
- Audio recovers from device disconnect within 2s
- Speaker ID deferred to Phase 5 if Deepgram diarization sufficient
- `cargo test` passes with mock audio + mock WebSocket tests

## Batch structure
Ships as 3 PRs:
- **PR5**: B2.1 + B2.2 + B2.3 + B2.13 (capture pipeline)
- **PR6**: B2.4 + B2.14 (VAD + supervisor)
- **PR7**: B2.5 + B2.8 + B2.11 (STT providers + state machine + question extractor)

## Tasks (in execution order)

### Task B2.1 🔴 [L] SystemAudioStream Trait
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (native helpers exist as standalone binaries, no Rust trait)
**Depends on**: —
**Summary**: Define a `SystemAudioStream` trait yielding mono f32 samples at device native rate. macOS impl wraps the existing Swift SCK helper via subprocess stdout pipe (interim) with a future pure-Rust cidre path. Windows impl wraps WASAPI helper similarly. The trait enables swapping backends without changing the DSP pipeline.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §1 (pluely SpeakerStream pattern)
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/capture.rs
use tokio::sync::mpsc;

#[async_trait::async_trait]
pub trait SystemAudioStream: Send + 'static {
    fn sample_rate(&self) -> u32;
    async fn next_chunk(&mut self) -> Option<Vec<f32>>;
    fn stop(&mut self);
}

#[cfg(target_os = "macos")]
pub struct MacOsSystemAudio {
    rx: mpsc::Receiver<Vec<f32>>,
    rate: u32,
    child: Option<tokio::process::Child>,
}

#[cfg(target_os = "macos")]
impl MacOsSystemAudio {
    pub async fn new() -> anyhow::Result<Self> {
        let mut child = tokio::process::Command::new("cue-audio")
            .args(["--source", "system", "--duration-ms", "0"])
            .stdout(std::process::Stdio::piped())
            .spawn()?;
        let stdout = child.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel(128);
        tokio::spawn(async move {
            Self::read_f32_frames(stdout, tx).await;
        });
        Ok(Self { rx, rate: 48_000, child: Some(child) })
    }

    async fn read_f32_frames(
        mut stdout: tokio::process::ChildStdout,
        tx: mpsc::Sender<Vec<f32>>,
    ) {
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 3840]; // 960 f32 samples = 20ms at 48kHz
        loop {
            match stdout.read_exact(&mut buf).await {
                Ok(_) => {
                    let samples: Vec<f32> = buf.chunks_exact(4)
                        .map(|c| f32::from_le_bytes([c[0],c[1],c[2],c[3]]))
                        .collect();
                    if tx.send(samples).await.is_err() { break; }
                }
                Err(_) => break,
            }
        }
    }
}

#[cfg(target_os = "macos")]
#[async_trait::async_trait]
impl SystemAudioStream for MacOsSystemAudio {
    fn sample_rate(&self) -> u32 { self.rate }
    async fn next_chunk(&mut self) -> Option<Vec<f32>> { self.rx.recv().await }
    fn stop(&mut self) {
        if let Some(ref mut c) = self.child { let _ = c.start_kill(); }
    }
}
```
**Acceptance criteria**:
- `MacOsSystemAudio::new()` spawns helper and yields f32 chunks
- Chunks arrive at ~50/sec (20ms intervals at 48kHz)
- `stop()` kills child process and channel closes
- Trait is object-safe (`Box<dyn SystemAudioStream>` compiles)
**Verification commands**:
- `cargo test -p cue-daemon audio::capture` (mock test with fake stdout)
- Manual: run with real audio, verify non-zero samples
**Risks + mitigations**:
- Helper binary not found → check PATH, bundle alongside daemon binary in build script

---

### Task B2.2 🟡 [M] CPAL Microphone Capture
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (native helpers do mic, no CPAL in Rust)
**Depends on**: —
**Summary**: CPAL-based microphone capture with stream recreation on every start (fixes silent crash bug from natively-cluely). Atomic sample rate tracking. Error signaling via channel. Yields f32 chunks matching the SystemAudioStream interface.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §2 (CPAL stream recreation pattern)
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/microphone.rs
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use std::sync::{Arc, atomic::{AtomicU32, Ordering}};
use tokio::sync::mpsc;

pub struct MicrophoneCapture {
    rx: Option<mpsc::Receiver<Vec<f32>>>,
    sample_rate: Arc<AtomicU32>,
    stream: Option<cpal::Stream>,
}

impl MicrophoneCapture {
    pub fn start(&mut self) -> anyhow::Result<()> {
        // Recreate stream every time (fixes silent crash)
        self.stream = None;
        let host = cpal::default_host();
        let device = host.default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No input device"))?;
        let config = device.default_input_config()?;
        self.sample_rate.store(config.sample_rate().0, Ordering::Relaxed);

        let (tx, rx) = mpsc::channel(128);
        self.rx = Some(rx);
        let chunk_size = (config.sample_rate().0 / 50) as usize; // 20ms
        let mut buf = Vec::with_capacity(chunk_size);

        let stream = device.build_input_stream(
            &config.into(),
            move |data: &[f32], _| {
                buf.extend_from_slice(data);
                while buf.len() >= chunk_size {
                    let chunk: Vec<f32> = buf.drain(..chunk_size).collect();
                    let _ = tx.try_send(chunk);
                }
            },
            |err| tracing::error!("CPAL error: {}", err),
            None,
        )?;
        stream.play()?;
        self.stream = Some(stream);
        Ok(())
    }

    pub fn sample_rate(&self) -> u32 { self.sample_rate.load(Ordering::Relaxed) }
    pub fn stop(&mut self) { self.stream = None; self.rx = None; }
}
```
**Acceptance criteria**:
- `start()` → `stop()` → `start()` works without panic (stream recreation)
- `sample_rate()` returns actual hardware rate (not hardcoded)
- Chunks arrive via `rx` at expected interval
- Error callback logs but doesn't panic
**Verification commands**:
- `cargo test -p cue-daemon audio::microphone`
- Manual: speak into mic, verify non-silent chunks
**Risks + mitigations**:
- No mic permission on macOS → detect TCC denial, surface as user-facing error

---

### Task B2.3 🟡 [S] Zero-Copy DSP Loop
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (f32→i16 exists but naive)
**Depends on**: B2.1, B2.2
**Summary**: DSP processing loop as a tokio task. Drains audio chunks from capture, converts f32→i16 via bytemuck-compatible path, processes in 20ms frames, feeds VAD, emits speech frames to STT channel. Zero-copy where possible using `bytemuck::cast_slice`.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §3 (DSP loop pattern)
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/dsp.rs
use tokio::sync::mpsc;

pub struct AudioFrame {
    pub data: Vec<u8>,       // i16 LE bytes
    pub speech_ended: bool,
}

#[inline]
pub fn f32_to_i16(samples: &[f32], out: &mut Vec<i16>) {
    out.clear();
    out.extend(samples.iter().map(|&s| (s * 32767.0).clamp(-32768.0, 32767.0) as i16));
}

pub async fn dsp_loop(
    mut audio_rx: mpsc::Receiver<Vec<f32>>,
    vad: &mut crate::audio::vad::TwoStageVad,
    stt_tx: mpsc::Sender<AudioFrame>,
) {
    let mut i16_buf = Vec::with_capacity(960);
    while let Some(chunk) = audio_rx.recv().await {
        f32_to_i16(&chunk, &mut i16_buf);
        let action = vad.process(&i16_buf);
        match action {
            crate::audio::vad::VadAction::Speech(ended) => {
                let bytes = bytemuck::cast_slice::<i16, u8>(&i16_buf).to_vec();
                let _ = stt_tx.send(AudioFrame { data: bytes, speech_ended: ended }).await;
            }
            _ => {}
        }
    }
}
```
**Acceptance criteria**:
- f32→i16 conversion matches expected values (±1 LSB)
- `bytemuck::cast_slice` produces correct LE byte layout
- DSP loop processes chunks without allocation per frame (reuses `i16_buf`)
- Only speech frames reach `stt_tx` (silence suppressed)
**Verification commands**:
- `cargo test -p cue-daemon audio::dsp` — unit test with known f32 input
**Risks + mitigations**:
- Endianness on non-LE platforms → bytemuck handles this; all targets are LE

---

### Task B2.13 🟡 [S] Sample Rate Detection + Rubato Resampler
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (hardcoded 48k→16k by skipping samples)
**Depends on**: B2.1, B2.2
**Summary**: Detect actual device sample rate from capture trait, resample to 16kHz (STT requirement) using rubato crate for high-quality sinc interpolation. Replaces the naive skip-every-3rd-sample approach.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §2 (sample rate detection bug fix from natively-cluely)
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/resample.rs
use rubato::{SincFixedIn, SincInterpolationType, SincInterpolationParameters, WindowFunction};

pub struct Resampler {
    inner: SincFixedIn<f32>,
    input_rate: u32,
}

impl Resampler {
    pub fn new(input_rate: u32, output_rate: u32, chunk_size: usize) -> anyhow::Result<Self> {
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        let ratio = output_rate as f64 / input_rate as f64;
        let resampler = SincFixedIn::new(ratio, 2.0, params, chunk_size, 1)?;
        Ok(Self { inner: resampler, input_rate })
    }

    pub fn process(&mut self, input: &[f32]) -> Vec<f32> {
        let input_frames = vec![input.to_vec()];
        match self.inner.process(&input_frames, None) {
            Ok(output) => output.into_iter().next().unwrap_or_default(),
            Err(_) => Vec::new(),
        }
    }
}
```
**Acceptance criteria**:
- 48kHz input → 16kHz output (3:1 ratio) with correct sample count
- 44.1kHz input → 16kHz output works (non-integer ratio)
- Output quality: no audible aliasing on speech (manual listen test)
- Resampler created dynamically based on detected rate (not hardcoded)
**Verification commands**:
- `cargo test -p cue-daemon audio::resample` — sine wave test, verify frequency preserved
**Risks + mitigations**:
- rubato adds ~2ms latency per chunk → acceptable for 20ms frame budget


---

### Task B2.4 🔴 [M] Two-Stage VAD
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B2.3
**Summary**: Two-stage voice activity detection: Stage 1 adaptive RMS threshold (fast reject of silence/noise), Stage 2 WebRTC ML VAD confirmation. Hangover FSM preserves trailing consonants. Ported directly from natively-cluely's `silence_suppression.rs` pattern.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §4 (full algorithm + state machine)
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/vad.rs
use webrtc_vad::{Vad as WebRtcVad, VadMode};

pub enum VadAction { Speech(bool), Silence, Suppress }

#[derive(PartialEq, Clone, Copy)]
enum State { Active, Hangover, Suppressed }

pub struct TwoStageVad {
    noise_floor: f32,
    threshold_multiplier: f32,
    min_floor: f32,
    webrtc: WebRtcVad,
    state: State,
    hangover_remaining: u32,
    hangover_frames: u32,
    was_speaking: bool,
    silence_count: u32,
}

impl TwoStageVad {
    pub fn new(mode: VadMode) -> Self {
        let mut webrtc = WebRtcVad::new();
        webrtc.set_mode(mode);
        Self {
            noise_floor: 20.0, threshold_multiplier: 2.5, min_floor: 20.0,
            webrtc, state: State::Suppressed,
            hangover_remaining: 0, hangover_frames: 15,
            was_speaking: false, silence_count: 0,
        }
    }

    pub fn process(&mut self, samples: &[i16]) -> VadAction {
        let rms = (samples.iter().map(|&s| (s as f64).powi(2)).sum::<f64>()
            / samples.len() as f64).sqrt() as f32;
        let threshold = (self.noise_floor * self.threshold_multiplier).max(self.min_floor);

        if self.state == State::Suppressed {
            self.noise_floor = self.noise_floor * 0.98 + rms * 0.02;
        }

        let rms_pass = rms > threshold;
        let ml_pass = rms_pass && self.webrtc.is_voice_segment(samples).unwrap_or(false);
        let mut speech_ended = false;

        match self.state {
            State::Suppressed if ml_pass => { self.state = State::Active; self.was_speaking = true; }
            State::Active if !ml_pass => { self.state = State::Hangover; self.hangover_remaining = self.hangover_frames; }
            State::Hangover if ml_pass => { self.state = State::Active; }
            State::Hangover => {
                self.hangover_remaining = self.hangover_remaining.saturating_sub(1);
                if self.hangover_remaining == 0 {
                    self.state = State::Suppressed;
                    if self.was_speaking { speech_ended = true; self.was_speaking = false; }
                }
            }
            _ => {}
        }

        match self.state {
            State::Active | State::Hangover => VadAction::Speech(speech_ended),
            State::Suppressed => {
                self.silence_count += 1;
                if self.silence_count >= 5 { self.silence_count = 0; VadAction::Silence }
                else { VadAction::Suppress }
            }
        }
    }
}
```
**Acceptance criteria**:
- Silence input → `Suppress` (no STT billing)
- Speech input → `Speech(false)` during, `Speech(true)` on end
- Hangover preserves 300ms trailing audio after speech stops
- Noise floor adapts: loud environment raises threshold
- Unit test with synthetic speech/silence patterns
**Verification commands**:
- `cargo test -p cue-daemon audio::vad`
**Risks + mitigations**:
- WebRTC VAD requires 16kHz input → resample before VAD (B2.13 provides this)

---

### Task B2.5 🔴 [L] SttProvider Trait + Deepgram Nova-3 WS
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (only OpenAI Whisper REST exists)
**Depends on**: B2.3
**Summary**: Define `SttProvider` async trait with `write()`, `start()`, `stop()`, `state()`. Implement Deepgram Nova-3 as primary provider using persistent WebSocket (binary frames in, JSON transcripts out). Add OpenAI REST as fallback. The WebSocket stays open for the entire session (L1 lifecycle managed here).
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §5 (trait + Deepgram impl)
**Code sketch**:
```rust
// crates/cue-daemon/src/stt/mod.rs
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub struct Transcript { pub text: String, pub is_final: bool, pub timestamp_ms: u64 }

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SttState { Connected, Reconnecting, Failed }

#[async_trait::async_trait]
pub trait SttProvider: Send + Sync {
    async fn write(&self, audio: &[u8]) -> anyhow::Result<()>;
    async fn start(&mut self, sample_rate: u32, language: &str) -> anyhow::Result<()>;
    async fn stop(&mut self) -> anyhow::Result<()>;
    fn state(&self) -> SttState;
    fn name(&self) -> &'static str;
}

// crates/cue-daemon/src/stt/deepgram.rs
pub struct DeepgramStt {
    tx: mpsc::Sender<Transcript>,
    audio_tx: Option<mpsc::Sender<Vec<u8>>>,
    state: std::sync::Arc<std::sync::atomic::AtomicU8>,
}

#[async_trait::async_trait]
impl SttProvider for DeepgramStt {
    async fn write(&self, audio: &[u8]) -> anyhow::Result<()> {
        if let Some(ref tx) = self.audio_tx {
            tx.send(audio.to_vec()).await.map_err(|e| anyhow::anyhow!(e.to_string()))?;
        }
        Ok(())
    }

    async fn start(&mut self, sample_rate: u32, language: &str) -> anyhow::Result<()> {
        let url = format!(
            "wss://api.deepgram.com/v1/listen?model=nova-3&encoding=linear16&sample_rate={}&channels=1&language={}&punctuate=true&interim_results=true",
            sample_rate, language
        );
        // Connect WebSocket, spawn read/write tasks
        // (full impl follows design doc pattern)
        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(64);
        self.audio_tx = Some(audio_tx);
        self.state.store(0, std::sync::atomic::Ordering::Relaxed);
        Ok(())
    }

    async fn stop(&mut self) -> anyhow::Result<()> {
        self.audio_tx = None;
        Ok(())
    }

    fn state(&self) -> SttState {
        match self.state.load(std::sync::atomic::Ordering::Relaxed) {
            0 => SttState::Connected, 1 => SttState::Reconnecting, _ => SttState::Failed,
        }
    }
    fn name(&self) -> &'static str { "deepgram" }
}
```
**Acceptance criteria**:
- Deepgram WS connects with valid API key and receives transcripts
- Partial (interim) transcripts arrive within 200ms of speech
- Final transcripts arrive within 500ms of speech end
- WebSocket stays open between utterances (persistent per session)
- Graceful reconnect on WS drop (state → Reconnecting → Connected)
**Verification commands**:
- `cargo test -p cue-daemon stt::deepgram` (mock WS server)
- Manual: speak → see transcripts in daemon logs
**Risks + mitigations**:
- Deepgram rate limits → L1 (Phase 9) handles lifecycle; here we just implement connect/write/read

---

### Task B2.8 🟡 [S] STT State Machine + Backoff
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (minimal error handling exists)
**Depends on**: B2.5
**Summary**: Classified error handling for STT: auth errors (fatal), quota errors (fatal), transient errors (retry with exponential backoff). State machine broadcasts transitions. Max 5 retries before marking failed.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §6 (error classification + state machine)
**Code sketch**:
```rust
// crates/cue-daemon/src/stt/state_machine.rs
pub struct SttStateMachine {
    state: SttState,
    errors: u32,
    backoff_ms: u64,
}

impl SttStateMachine {
    pub fn new() -> Self { Self { state: SttState::Connected, errors: 0, backoff_ms: 1000 } }

    pub fn on_error(&mut self, status: Option<u16>, msg: &str) -> SttState {
        match status {
            Some(401) | Some(403) => { self.state = SttState::Failed; }
            Some(402) => { self.state = SttState::Failed; }
            _ => {
                self.errors += 1;
                if self.errors >= 5 { self.state = SttState::Failed; }
                else {
                    self.state = SttState::Reconnecting;
                    self.backoff_ms = (self.backoff_ms * 2).min(30_000);
                }
            }
        }
        self.state
    }

    pub fn on_success(&mut self) { self.state = SttState::Connected; self.errors = 0; self.backoff_ms = 1000; }
    pub fn backoff(&self) -> std::time::Duration { std::time::Duration::from_millis(self.backoff_ms) }
}
```
**Acceptance criteria**:
- Auth error → immediate Failed state (no retry)
- Transient error → Reconnecting with doubling backoff (1s, 2s, 4s, 8s, 16s)
- 5th transient error → Failed
- Successful transcript resets error counter
**Verification commands**:
- `cargo test -p cue-daemon stt::state_machine`
**Risks + mitigations**:
- None — pure state logic, well-tested

---

### Task B2.11 🟡 [S] Question Extractor + Noise Filter
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (basic `is_question()` exists in cue-core)
**Depends on**: —
**Summary**: Upgrade the basic question detector to filter noise (greetings, filler, too-short utterances) and detect coding questions. Output feeds the intent classifier (R1) in Phase 4. Heuristic-based, no ML.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §B2.11, CUE-REFERENCE-ANALYSIS.md pattern #36
**Code sketch**:
```rust
// crates/cue-core/src/intelligence.rs — upgrade existing
#[derive(Debug, PartialEq)]
pub enum QuestionType { General, Coding, Behavioral, Noise }

pub fn classify_utterance(text: &str) -> QuestionType {
    let trimmed = text.trim();
    if trimmed.len() < 10 { return QuestionType::Noise; }

    let lower = trimmed.to_lowercase();
    // Noise filter
    let noise_patterns = ["hello", "hi there", "thank you", "thanks", "okay", "um", "uh"];
    if noise_patterns.iter().any(|p| lower == *p || lower.starts_with(p) && lower.len() < 20) {
        return QuestionType::Noise;
    }

    // Coding detection
    let code_signals = ["implement", "algorithm", "time complexity", "data structure",
        "function", "leetcode", "binary tree", "linked list", "array"];
    if code_signals.iter().any(|s| lower.contains(s)) {
        return QuestionType::Coding;
    }

    // Behavioral detection
    let behavioral = ["tell me about a time", "describe a situation", "give an example",
        "what would you do", "how did you handle"];
    if behavioral.iter().any(|s| lower.contains(s)) {
        return QuestionType::Behavioral;
    }

    QuestionType::General
}
```
**Acceptance criteria**:
- "um" → Noise, "hello" → Noise
- "implement a binary tree" → Coding
- "tell me about a time you failed" → Behavioral
- "what's your approach to system design?" → General
- ≥10 unit tests covering edge cases
**Verification commands**:
- `cargo test -p cue-core intelligence::classify`
**Risks + mitigations**:
- Heuristic accuracy ~70% → R1 (Phase 4) adds embedding-based classification on top

---

### Task B2.14 🟡 [M] Audio Supervisor + Recovery
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B2.1, B2.2
**Summary**: Supervisor task that monitors audio capture health. Detects: device disconnect (no samples for 2s), error signals from CPAL, sleep/wake events. On failure: stops capture, waits 1s, restarts. Emits status events for overlay/dashboard.
**Design source**: CUE-DESIGN-02-AUDIO-STT.md §B2.14, CUE-REFERENCE-ANALYSIS.md pattern #27
**Code sketch**:
```rust
// crates/cue-daemon/src/audio/supervisor.rs
use tokio::time::{interval, Duration, Instant};

pub struct AudioSupervisor {
    last_sample_time: Instant,
    restart_count: u32,
    max_restarts: u32,
}

impl AudioSupervisor {
    pub fn new() -> Self {
        Self { last_sample_time: Instant::now(), restart_count: 0, max_restarts: 10 }
    }

    pub fn on_samples_received(&mut self) { self.last_sample_time = Instant::now(); }

    pub fn check_health(&self) -> AudioHealth {
        if self.last_sample_time.elapsed() > Duration::from_secs(2) {
            AudioHealth::DeviceDisconnected
        } else {
            AudioHealth::Healthy
        }
    }

    pub fn on_restart(&mut self) -> bool {
        self.restart_count += 1;
        self.restart_count <= self.max_restarts
    }
}

pub enum AudioHealth { Healthy, DeviceDisconnected, ErrorSignaled(String) }
```
**Acceptance criteria**:
- No samples for 2s → triggers restart
- Restart succeeds and audio resumes within 3s total
- After 10 consecutive restarts, marks as permanently failed
- Sleep/wake: on wake, forces restart regardless of health
**Verification commands**:
- `cargo test -p cue-daemon audio::supervisor`
- Manual: unplug headphones → audio recovers when re-plugged
**Risks + mitigations**:
- Infinite restart loop → max_restarts cap prevents this

---

## Phase deliverable
- Audio pipeline captures system + mic, resamples to 16kHz, filters via two-stage VAD
- Deepgram Nova-3 produces streaming transcripts with <500ms latency
- Audio recovers from device disconnect automatically
- Question extractor classifies utterances for downstream routing
- Codex review should check: VAD actually reduces STT API calls (log speech vs suppress ratio), WebSocket stays open between utterances, no audio data leaks to logs


---

# Phase 4 — Reasoning Upgrade: Three-Lane Router + Patch-Mode (~5 weeks)

## Entry criteria
- Phase 0 complete (IPC, SQLite, Unix socket)
- B2.5 done (STT produces transcripts)
- B2.11 done (question extractor feeds intent classifier)

## Exit criteria
- Three-lane routing dispatches to Snap/Solve/Think based on intent
- Streaming with cancel-on-upgrade semantics works
- Patch-mode PATCH/KEEP/MODIFY/ADD/REMOVE parser produces structured diffs
- Overlay renders diff highlights for follow-up responses
- Progressive enhancement: Snap answer shows immediately, replaced by Solve if better
- Manual overrides (Alt+D, Alt+F, /think, /snap) work

## Batch structure
Ships as 3 PRs:
- **PR8** (Batch 4A, weeks 1-2): B3.1 + B3.2 + B3.3 + B3.4 + B3.5 + B3.9 + B3.12
- **PR9** (Batch 4B, weeks 2-3): B4.1 + B4.2 + B4.6 + B4.8 + R1 + R2 + R3 + R4 + R5
- **PR10** (Batch 4C, weeks 3-5): F1 + F2 + F3 + F4 + F5 + F6 + F7 + F8 + B3.11

## Tasks (in execution order)

---

## Batch 4A — LLM Foundation (weeks 1-2)

### Task B3.1 🔴 [L] Multi-Provider LLM Trait (Three-Lane Aware)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (routing exists as match+loop, no trait)
**Depends on**: —
**Summary**: Define `LlmProvider` async trait with `stream()` and `generate()` methods. Implement for Anthropic (Claude Sonnet 4.5 / Opus), OpenAI-compatible (Cerebras, Groq, OpenAI), and o3. Each provider tagged with supported lanes. The router dispatches based on lane assignment, not provider name.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §1 Provider Router Design
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/mod.rs
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Lane { Snap, Solve, Think }

#[derive(Debug, Clone)]
pub struct LlmRequest {
    pub messages: Vec<ChatMessage>,
    pub lane: Lane,
    pub max_tokens: u32,
    pub temperature: f32,
    pub session_id: String,
}

#[derive(Debug, Clone)]
pub struct ChatMessage { pub role: String, pub content: String }

#[derive(Debug, Clone)]
pub struct Token { pub text: String, pub done: bool }

#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    fn name(&self) -> &str;
    fn supported_lanes(&self) -> &[Lane];
    async fn stream(
        &self, request: &LlmRequest, cancel: CancellationToken,
    ) -> anyhow::Result<mpsc::Receiver<Token>>;
}

// crates/cue-daemon/src/llm/cerebras.rs
pub struct CerebrasProvider {
    client: reqwest::Client,
    api_key: String,
    model: String,
}

#[async_trait::async_trait]
impl LlmProvider for CerebrasProvider {
    fn name(&self) -> &str { "cerebras" }
    fn supported_lanes(&self) -> &[Lane] { &[Lane::Snap] }

    async fn stream(&self, request: &LlmRequest, cancel: CancellationToken,
    ) -> anyhow::Result<mpsc::Receiver<Token>> {
        let (tx, rx) = mpsc::channel(64);
        let client = self.client.clone();
        let url = "https://api.cerebras.ai/v1/chat/completions";
        let body = serde_json::json!({
            "model": &self.model,
            "messages": request.messages.iter().map(|m| serde_json::json!({"role": &m.role, "content": &m.content})).collect::<Vec<_>>(),
            "max_tokens": request.max_tokens,
            "stream": true,
        });
        let api_key = self.api_key.clone();

        tokio::spawn(async move {
            let resp = client.post(url)
                .bearer_auth(&api_key)
                .json(&body)
                .send().await;
            if let Ok(resp) = resp {
                let mut stream = resp.bytes_stream();
                use futures::StreamExt;
                while let Some(Ok(chunk)) = stream.next().await {
                    if cancel.is_cancelled() { break; }
                    // Parse SSE lines, extract content delta
                    let text = String::from_utf8_lossy(&chunk);
                    for line in text.lines().filter(|l| l.starts_with("data: ")) {
                        let data = &line[6..];
                        if data == "[DONE]" { let _ = tx.send(Token { text: String::new(), done: true }).await; return; }
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(delta) = v["choices"][0]["delta"]["content"].as_str() {
                                let _ = tx.send(Token { text: delta.to_string(), done: false }).await;
                            }
                        }
                    }
                }
            }
        });
        Ok(rx)
    }
}
```
**Acceptance criteria**:
- Trait is object-safe: `Box<dyn LlmProvider>` compiles
- CerebrasProvider streams tokens from Cerebras API (Snap lane)
- AnthropicProvider streams from Claude API (Solve/Think lanes)
- CancellationToken stops generation mid-stream
- 3 unit tests with mock HTTP responses
**Verification commands**:
- `cargo test -p cue-daemon llm` — mock provider tests
- Manual: set CEREBRAS_API_KEY, send query, see streaming tokens
**Risks + mitigations**:
- SSE parsing edge cases → use `eventsource-stream` crate for robust parsing

---

### Task B3.2 🟡 [M] ModelVersionManager
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B3.1
**Summary**: Background task that polls provider `/models` endpoints hourly to discover available models. Maintains a capability map (vision, streaming, JSON mode, thinking, max context). Used by router to select best model per lane.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §3 ModelVersionManager
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/model_versions.rs
use std::collections::HashMap;
use tokio::sync::RwLock;

pub struct ModelVersionManager {
    models: RwLock<HashMap<String, ModelInfo>>,
}

#[derive(Clone, Debug)]
pub struct ModelInfo {
    pub id: String,
    pub provider: String,
    pub max_context: u32,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub supports_thinking: bool,
}

impl ModelVersionManager {
    pub async fn refresh(&self, providers: &[Box<dyn super::LlmProvider>]) {
        // Poll each provider's model list endpoint
        // Update internal map
    }

    pub fn best_model_for_lane(&self, lane: super::Lane) -> Option<ModelInfo> {
        let models = self.models.blocking_read();
        match lane {
            super::Lane::Snap => models.values().find(|m| m.provider == "cerebras").cloned(),
            super::Lane::Solve => models.values().find(|m| m.id.contains("sonnet")).cloned(),
            super::Lane::Think => models.values().find(|m| m.supports_thinking).cloned(),
        }
    }
}
```
**Acceptance criteria**:
- Polls every hour (configurable)
- Returns correct model for each lane
- Handles API failures gracefully (keeps stale data)
**Verification commands**:
- `cargo test -p cue-daemon llm::model_versions`
**Risks + mitigations**:
- Provider API changes → hardcoded fallback model IDs as defaults

---

### Task B3.3 🟡 [M] Fallback Chains with Per-Lane Backoff
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (fallback iteration exists, no backoff)
**Depends on**: B3.1
**Summary**: Per-lane fallback chains. Snap: Cerebras → Groq → local. Solve: Claude → GPT-4o → Gemini. Think: o3 → Claude Opus. Exponential backoff per provider (30s base, 600s cap). Failed providers auto-recover when backoff expires.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §2 Fallback Chain Strategy
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/fallback.rs
use std::collections::HashMap;
use std::time::{Duration, Instant};

pub struct FallbackChain {
    chains: HashMap<super::Lane, Vec<String>>,
    failures: HashMap<String, (u32, Instant)>,
}

impl FallbackChain {
    pub fn available_for_lane(&self, lane: super::Lane) -> Vec<&str> {
        let now = Instant::now();
        self.chains.get(&lane).map(|chain| {
            chain.iter().filter(|p| {
                match self.failures.get(*p) {
                    None => true,
                    Some((count, last)) => now.duration_since(*last) >= Self::backoff(*count),
                }
            }).map(|s| s.as_str()).collect()
        }).unwrap_or_default()
    }

    fn backoff(failures: u32) -> Duration {
        Duration::from_secs((30 * 2u64.pow(failures.saturating_sub(1))).min(600))
    }

    pub fn mark_failure(&mut self, provider: &str) {
        let entry = self.failures.entry(provider.to_string()).or_insert((0, Instant::now()));
        entry.0 += 1; entry.1 = Instant::now();
    }

    pub fn mark_success(&mut self, provider: &str) { self.failures.remove(provider); }
}
```
**Acceptance criteria**:
- First failure → 30s backoff, second → 60s, capped at 600s
- Provider auto-recovers after backoff expires
- Each lane has independent chain (Snap failure doesn't affect Solve)
**Verification commands**:
- `cargo test -p cue-daemon llm::fallback`
**Risks + mitigations**:
- All providers down → surface error to user with "check API keys" message

---

### Task B3.4 🔴 [S] Rate Limiters (Per-Lane)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B3.1
**Summary**: Token-bucket rate limiters via `governor` crate. Per-provider limits matching API quotas. Snap lane (Cerebras): 60 RPM. Solve lane (Claude): 50 RPM. Think lane (o3): 20 RPM. Limiter checked before dispatch; if exhausted, falls to next in chain.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §4 Rate Limiters
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/rate_limit.rs
use governor::{Quota, RateLimiter, clock::DefaultClock, state::InMemoryState, state::NotKeyed};
use std::num::NonZeroU32;

pub type Limiter = RateLimiter<NotKeyed, InMemoryState, DefaultClock>;

pub fn create_limiters() -> std::collections::HashMap<String, Limiter> {
    let mut m = std::collections::HashMap::new();
    m.insert("cerebras".into(), RateLimiter::direct(Quota::per_minute(NonZeroU32::new(60).unwrap())));
    m.insert("anthropic".into(), RateLimiter::direct(Quota::per_minute(NonZeroU32::new(50).unwrap())));
    m.insert("openai".into(), RateLimiter::direct(Quota::per_minute(NonZeroU32::new(500).unwrap())));
    m.insert("groq".into(), RateLimiter::direct(Quota::per_minute(NonZeroU32::new(30).unwrap())));
    m
}
```
**Acceptance criteria**:
- Exceeding rate limit returns `Err` (caller falls to next provider)
- Limits are per-provider, not per-lane
- No blocking: `check()` is non-blocking, returns immediately
**Verification commands**:
- `cargo test -p cue-daemon llm::rate_limit`
**Risks + mitigations**:
- None — governor is well-tested

---

### Task B3.5 🔴 [M] Streaming 60Hz + Cancel-on-Upgrade
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (SSE streaming exists, no batching or cancellation)
**Depends on**: B3.1, D0.2
**Summary**: Stream emitter batches tokens at 60Hz before sending to overlay (Unix socket) and dashboard (Tauri event). CancellationToken per generation. Lane upgrade (Snap→Solve) cancels the Snap generation and drops its answer — the Solve answer replaces it entirely (no anchoring on wrong answer).
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §5 StreamEmitter, Skeleton "Lane upgrades drop previous"
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/stream_emitter.rs
use tokio::sync::mpsc;
use tokio::time::{interval, Duration};
use tokio_util::sync::CancellationToken;

pub struct StreamEmitter {
    overlay_tx: mpsc::Sender<String>,
    generation_id: u64,
}

impl StreamEmitter {
    pub async fn run(
        &self,
        mut token_rx: mpsc::Receiver<super::Token>,
        cancel: CancellationToken,
    ) -> Option<String> {
        let mut buffer = String::new();
        let mut full_response = String::new();
        let mut tick = interval(Duration::from_millis(16)); // 60Hz

        loop {
            tokio::select! {
                _ = cancel.cancelled() => { return None; } // Dropped on upgrade
                _ = tick.tick() => {
                    if !buffer.is_empty() {
                        let chunk = std::mem::take(&mut buffer);
                        let _ = self.overlay_tx.send(chunk).await;
                    }
                }
                token = token_rx.recv() => {
                    match token {
                        Some(t) if t.done => {
                            if !buffer.is_empty() {
                                let _ = self.overlay_tx.send(std::mem::take(&mut buffer)).await;
                            }
                            return Some(full_response);
                        }
                        Some(t) => { buffer.push_str(&t.text); full_response.push_str(&t.text); }
                        None => return Some(full_response),
                    }
                }
            }
        }
    }
}
```
**Acceptance criteria**:
- Tokens batched: 60 individual tokens/sec → ~60 IPC messages/sec (not 1:1)
- Cancel mid-stream: `cancel.cancel()` → `run()` returns `None`
- Lane upgrade scenario: Snap starts, Solve dispatched, Snap cancelled, Solve replaces
- Full response returned on completion for persistence
**Verification commands**:
- `cargo test -p cue-daemon llm::stream_emitter`
**Risks + mitigations**:
- Token loss on cancel → acceptable (we're dropping the answer intentionally)

---

### Task B3.9 🟡 [S] Key Scrubbing (zeroize)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: —
**Summary**: Wrap all API keys in a `SecretString` type that uses `zeroize` crate to clear memory on drop. Prevents keys from lingering in memory after use or on crash.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §9 scrubKeys
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/credentials.rs
use zeroize::{Zeroize, ZeroizeOnDrop};

#[derive(Clone, ZeroizeOnDrop)]
pub struct SecretString(#[zeroize] String);

impl SecretString {
    pub fn new(s: String) -> Self { Self(s) }
    pub fn expose(&self) -> &str { &self.0 }
}

impl std::fmt::Debug for SecretString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "SecretString(***)")
    }
}
```
**Acceptance criteria**:
- `SecretString` zeroes memory on drop (verified via `zeroize` guarantee)
- Debug output never shows key value
- All provider constructors accept `SecretString`, not raw `String`
**Verification commands**:
- `cargo test -p cue-daemon llm::credentials`
**Risks + mitigations**:
- Compiler optimizations may elide zeroing → `zeroize` uses volatile writes

---

### Task B3.12 🟡 [M] Vision Fallback + Parallel Race
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (vision fallback exists, no parallel race)
**Depends on**: B3.1
**Summary**: 3-tier vision fallback (current model → Gemini Flash → Groq Llama 4 Scout). Parallel race for Gemini: dispatch to both Flash and Pro simultaneously, use first response. Cancels loser.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §12 Parallel Race
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/vision.rs
impl super::LlmRouter {
    pub async fn generate_with_vision(&self, request: &super::LlmRequest, image_b64: &str) -> anyhow::Result<String> {
        let tiers = [
            vec!["claude-sonnet-4-5"],
            vec!["gemini-2.5-flash", "gemini-2.5-pro"], // parallel race
            vec!["groq-llama-4-scout"],
        ];
        for tier in &tiers {
            if tier.len() > 1 {
                // Parallel race: first success wins
                let futs: Vec<_> = tier.iter().map(|m| self.try_vision(request, image_b64, m)).collect();
                if let Ok((result, _)) = futures::future::select_ok(futs.into_iter().map(Box::pin)).await {
                    return Ok(result);
                }
            } else if let Ok(result) = self.try_vision(request, image_b64, tier[0]).await {
                return Ok(result);
            }
        }
        Err(anyhow::anyhow!("All vision providers exhausted"))
    }
}
```
**Acceptance criteria**:
- Vision request tries tiers in order
- Parallel tier races two models, uses first response
- Loser cancelled via CancellationToken
- Falls through all tiers before returning error
**Verification commands**:
- `cargo test -p cue-daemon llm::vision` (mock providers)
**Risks + mitigations**:
- Double billing on parallel race → acceptable (cost is not a concern per spec)


---

## Batch 4B — Prompt + Routing (weeks 2-3)

### Task B4.1 🔴 [M] Prompt Composition System (Per-Lane Templates)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (hardcoded system prompt string)
**Depends on**: —
**Summary**: Composable prompt system with XML-tagged shared blocks. Each lane gets a different composition: Snap gets TINY (<500 tokens), Solve gets full composition + skill template, Think gets full + extended-thinking preamble. Blocks: CORE_IDENTITY, EXECUTION_CONTRACT, CONTEXT_LAYER, CODING_RULES, ANTI_CHATBOT.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md Part 2 §1 Composition Pattern
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/composer.rs
use super::Lane;

pub struct PromptComposer {
    pub core_identity: &'static str,
    pub execution_contract: &'static str,
    pub context_layer: &'static str,
    pub coding_rules: &'static str,
    pub anti_chatbot: &'static str,
}

impl PromptComposer {
    pub fn compose(&self, lane: Lane, skill: Option<&str>, context: &str) -> String {
        match lane {
            Lane::Snap => format!(
                "Interview copilot. Output=user's exact words. 2-4 sentences. First person. No meta. STOP.\n\n{}",
                context
            ),
            Lane::Solve => format!(
                "{}\n{}\n{}\n{}\n{}\n{}\n\n<context>\n{}\n</context>",
                self.core_identity, self.execution_contract, self.context_layer,
                self.coding_rules, self.anti_chatbot,
                skill.unwrap_or(""), context
            ),
            Lane::Think => format!(
                "<thinking-mode>\nYou have extended thinking enabled. Use it for complex reasoning.\nShow your work step by step before giving the final answer.\n</thinking-mode>\n\n{}\n{}\n{}\n\n<context>\n{}\n</context>",
                self.core_identity, self.execution_contract, self.context_layer, context
            ),
        }
    }
}
```
**Acceptance criteria**:
- Snap prompt <500 tokens (measured via len/4 estimate)
- Solve prompt includes all 5 blocks + skill template
- Think prompt includes thinking-mode preamble
- Context injected into all variants
**Verification commands**:
- `cargo test -p cue-daemon prompts::composer` — token count assertions
**Risks + mitigations**:
- Prompt too long for Snap models → hard cap at 500 tokens, truncate context

---

### Task B4.2 🟡 [M] Answer Modes → Lane Mapping
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (5 modes exist with different naming)
**Depends on**: B4.1
**Summary**: Map the three answer modes (Assist/Answer/WhatToAnswer) to lane-appropriate prompt variants. Assist mode uses Solve lane (screenshot analysis). Answer mode uses router decision. WhatToAnswer uses Solve with first-person enforcement. This replaces the old 5-mode system.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md Part 2 §2 Three Modes
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/modes.rs
#[derive(Debug, Clone, Copy)]
pub enum AnswerMode { Assist, Answer, WhatToAnswer }

impl AnswerMode {
    pub fn default_lane(&self) -> super::Lane {
        match self {
            Self::Assist => super::Lane::Solve,
            Self::Answer => super::Lane::Solve, // overridden by router
            Self::WhatToAnswer => super::Lane::Solve,
        }
    }

    pub fn mode_suffix(&self) -> &'static str {
        match self {
            Self::Assist => "<mode>PASSIVE OBSERVER analyzing screenshots. Solve completely or say unsure.</mode>",
            Self::Answer => "<mode>ACTIVE CO-PILOT. Priority: answer > define > advance.</mode>",
            Self::WhatToAnswer => "<mode>Generate EXACT TEXT user will speak. First person. No wrapper.</mode>",
        }
    }
}
```
**Acceptance criteria**:
- Each mode maps to a default lane
- Mode suffix appended to composed prompt
- Router can override default lane (Answer mode respects R1 decision)
**Verification commands**:
- `cargo test -p cue-daemon prompts::modes`
**Risks + mitigations**:
- None

---

### Task B4.6 🟡 [S] Anti-Chatbot Constraints
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (basic brevity instruction exists)
**Depends on**: B4.1
**Summary**: Explicit negative constraints preventing AI-like preambles, coaching language, sign-offs. Embedded in EXECUTION_CONTRACT block. Includes HUMAN ANSWER LENGTH RULE (2-4 sentences, speakable in 30s).
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md Part 2 §6-7 Anti-Chatbot + Length Rule
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/blocks.rs
pub const ANTI_CHATBOT: &str = r#"<forbidden-patterns>
NEVER output: "That's a great question!" / "I'd be happy to help" / "Would you like me to elaborate?" / "Here's what you could say:" / Any meta-commentary / Any coaching preamble / Any sign-off / Any AI acknowledgment
</forbidden-patterns>
<answer-length>
Maximum 2-4 sentences. Speakable in under 30 seconds. STOP after answer.
Code blocks don't count toward sentence limit. Verbal explanation: still 2-4 sentences max.
</answer-length>"#;
```
**Acceptance criteria**:
- Block is included in Solve and Think prompts (not Snap — too short)
- Forbidden patterns list has ≥8 entries
- Length rule specifies 30-second speakability constraint
**Verification commands**:
- `cargo test -p cue-daemon prompts::blocks` — assert ANTI_CHATBOT contains key phrases
**Risks + mitigations**:
- Models may still violate → post-processing strip in Phase 6

---

### Task B4.8 🟡 [S] Context Prioritization (Per-Lane Budgets)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: 🟡 PARTIAL (per-kind character limits exist)
**Depends on**: B4.1
**Summary**: Per-lane token budgets for context injection. Snap: 2K total (last 3 turns only). Solve: 16K input (relevant turns via future RAG). Think: 32K input (full history + epoch summaries). Truncation strategy: newest-first for Snap, relevance-ranked for Solve, chronological for Think.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 (Three-Lane Routing Spec token budgets)
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/context_budget.rs
use super::Lane;

pub struct ContextBudget {
    pub max_input_tokens: u32,
    pub max_output_tokens: u32,
    pub strategy: ContextStrategy,
}

pub enum ContextStrategy { LastNTurns(usize), RelevanceRanked, FullChronological }

pub fn budget_for_lane(lane: Lane) -> ContextBudget {
    match lane {
        Lane::Snap => ContextBudget { max_input_tokens: 2_000, max_output_tokens: 1_000, strategy: ContextStrategy::LastNTurns(3) },
        Lane::Solve => ContextBudget { max_input_tokens: 16_000, max_output_tokens: 4_000, strategy: ContextStrategy::RelevanceRanked },
        Lane::Think => ContextBudget { max_input_tokens: 32_000, max_output_tokens: 8_000, strategy: ContextStrategy::FullChronological },
    }
}
```
**Acceptance criteria**:
- Snap context never exceeds 2K tokens (hard truncation)
- Solve context uses relevance ranking (placeholder until RAG in Phase 5)
- Think context includes full history up to 32K
**Verification commands**:
- `cargo test -p cue-daemon prompts::context_budget`
**Risks + mitigations**:
- Relevance ranking not available until Phase 5 → fall back to LastNTurns(10) for Solve initially

---

### Task R1 🔴 [L] Intent Classifier (<5ms)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B2.11, B3.1
**Summary**: Rule-based intent classifier that routes to Snap/Solve/Think in <5ms. Uses question type from B2.11, utterance length, keyword signals, and explicit user overrides. No ML model (decision: rule-based only for zero latency). Confidence score determines routing.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 Router Decision Flow
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/classifier.rs
use crate::llm::Lane;

pub struct ClassifierResult { pub lane: Lane, pub confidence: f32 }

pub fn classify_intent(
    text: &str,
    question_type: cue_core::intelligence::QuestionType,
    explicit_override: Option<Lane>,
) -> ClassifierResult {
    // Explicit override always wins
    if let Some(lane) = explicit_override {
        return ClassifierResult { lane, confidence: 1.0 };
    }

    let lower = text.to_lowercase();
    let word_count = text.split_whitespace().count();

    // Think signals (high confidence)
    let think_signals = ["think harder", "explain in detail", "step by step", "prove", "analyze deeply"];
    if think_signals.iter().any(|s| lower.contains(s)) {
        return ClassifierResult { lane: Lane::Think, confidence: 0.9 };
    }

    // Snap signals: short, conversational, follow-ups
    if word_count < 8 && matches!(question_type, cue_core::intelligence::QuestionType::General | cue_core::intelligence::QuestionType::Noise) {
        return ClassifierResult { lane: Lane::Snap, confidence: 0.85 };
    }

    // Coding/behavioral → Solve
    if matches!(question_type, cue_core::intelligence::QuestionType::Coding | cue_core::intelligence::QuestionType::Behavioral) {
        return ClassifierResult { lane: Lane::Solve, confidence: 0.8 };
    }

    // Default: Solve for medium-length, Snap for short
    if word_count > 15 {
        ClassifierResult { lane: Lane::Solve, confidence: 0.7 }
    } else {
        ClassifierResult { lane: Lane::Snap, confidence: 0.75 }
    }
}
```
**Acceptance criteria**:
- Classification completes in <1ms (no I/O, pure logic)
- "what's 2+2" → Snap (short, simple)
- "implement a binary search tree with delete" → Solve (coding)
- "think harder about this" → Think (explicit signal)
- Explicit override (/think, /snap, Alt+D, Alt+F) → confidence 1.0
- ≥15 unit tests covering all paths
**Verification commands**:
- `cargo test -p cue-daemon routing::classifier`
- Benchmark: `cargo bench` shows <1ms p99
**Risks + mitigations**:
- Rule-based accuracy ~75% → acceptable for V3; embedding classifier deferred to future

---

### Task R2 🔴 [M] Parallel Fast+Deep with Progressive UI
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: R1, B3.5
**Summary**: When classifier confidence is ambiguous (<0.7), dispatch both Snap and Solve in parallel. Show Snap result immediately in overlay. When Solve completes, replace Snap answer entirely (don't anchor on potentially wrong fast answer). Uses CancellationToken to stop Snap streaming if Solve arrives first.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 "parallel Snap+Solve, progressive enhancement"
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/progressive.rs
use crate::llm::{Lane, LlmRequest, LlmRouter, Token};
use tokio_util::sync::CancellationToken;

pub async fn progressive_dispatch(
    router: &LlmRouter,
    request: &LlmRequest,
    overlay_tx: &tokio::sync::mpsc::Sender<String>,
) -> (String, Lane) {
    let snap_cancel = CancellationToken::new();
    let solve_cancel = CancellationToken::new();

    let snap_req = LlmRequest { lane: Lane::Snap, ..request.clone() };
    let solve_req = LlmRequest { lane: Lane::Solve, ..request.clone() };

    let snap_rx = router.dispatch(&snap_req, snap_cancel.clone()).await;
    let solve_rx = router.dispatch(&solve_req, solve_cancel.clone()).await;

    // Stream Snap immediately
    let snap_handle = tokio::spawn({
        let tx = overlay_tx.clone();
        let cancel = snap_cancel.clone();
        async move {
            if let Ok(mut rx) = snap_rx {
                let mut full = String::new();
                while let Some(token) = rx.recv().await {
                    if cancel.is_cancelled() { break; }
                    full.push_str(&token.text);
                    let _ = tx.send(token.text).await;
                }
                full
            } else { String::new() }
        }
    });

    // Wait for Solve
    if let Ok(mut rx) = solve_rx {
        let mut solve_full = String::new();
        while let Some(token) = rx.recv().await {
            solve_full.push_str(&token.text);
        }
        // Solve arrived — cancel Snap, replace answer
        snap_cancel.cancel();
        let _ = overlay_tx.send("\x1B[REPLACE]".to_string()).await; // signal overlay to clear
        let _ = overlay_tx.send(solve_full.clone()).await;
        return (solve_full, Lane::Solve);
    }

    // Solve failed — keep Snap answer
    let snap_result = snap_handle.await.unwrap_or_default();
    (snap_result, Lane::Snap)
}
```
**Acceptance criteria**:
- Ambiguous query → Snap answer visible within 300ms
- Solve answer replaces Snap within 2-4s
- Overlay clears Snap content before showing Solve (no mixing)
- If Solve fails, Snap answer persists
**Verification commands**:
- `cargo test -p cue-daemon routing::progressive` (mock providers with delays)
**Risks + mitigations**:
- User reads Snap answer then it disappears → acceptable UX tradeoff per spec ("don't anchor on wrong answer")

---

### Task R3 🟡 [S] Manual Override System
**Layer**: CROSS
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: R1, B6.1
**Summary**: Alt+D forces Think lane, Alt+F forces Snap lane. Prefix commands /think, /snap, /deep in the input also override. Override is passed to classifier as `explicit_override` which returns confidence 1.0.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 (Override column in routing table)
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/overrides.rs
use crate::llm::Lane;

pub fn parse_override(input: &str) -> (Option<Lane>, &str) {
    let trimmed = input.trim();
    if let Some(rest) = trimmed.strip_prefix("/think") { return (Some(Lane::Think), rest.trim()); }
    if let Some(rest) = trimmed.strip_prefix("/deep") { return (Some(Lane::Think), rest.trim()); }
    if let Some(rest) = trimmed.strip_prefix("/snap") { return (Some(Lane::Snap), rest.trim()); }
    (None, trimmed)
}
```
**Acceptance criteria**:
- "/think what is X" → Think lane, query = "what is X"
- "/snap quick answer" → Snap lane, query = "quick answer"
- Alt+D hotkey sets override for next query (stateful in daemon)
- Override clears after one use
**Verification commands**:
- `cargo test -p cue-daemon routing::overrides`
**Risks + mitigations**:
- None

---

### Task R4 🟡 [M] Skill-Template Library
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B4.1, R1
**Summary**: 9+ skill templates (DSA, System Design, Behavioral, Programming, Sales, Negotiation, Presentation, DevOps, Data Science) loaded from embedded strings. Each skill provides a structured response format. Router selects skill based on question type + keyword matching. Skills inject into Solve/Think prompts only.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md Part 2 §5 Skill Library (Vysper 9 skills)
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/skills.rs
pub struct Skill { pub name: &'static str, pub template: &'static str, pub keywords: &'static [&'static str] }

pub const SKILLS: &[Skill] = &[
    Skill { name: "dsa", template: include_str!("skills/dsa.txt"), keywords: &["algorithm", "data structure", "leetcode", "binary tree", "linked list", "dynamic programming"] },
    Skill { name: "system_design", template: include_str!("skills/system_design.txt"), keywords: &["design a system", "architecture", "scalability", "distributed"] },
    Skill { name: "behavioral", template: include_str!("skills/behavioral.txt"), keywords: &["tell me about a time", "describe a situation", "leadership", "conflict"] },
    Skill { name: "programming", template: include_str!("skills/programming.txt"), keywords: &["implement", "write a function", "code", "refactor"] },
    Skill { name: "sales", template: include_str!("skills/sales.txt"), keywords: &["pitch", "objection", "close", "prospect"] },
    Skill { name: "negotiation", template: include_str!("skills/negotiation.txt"), keywords: &["salary", "offer", "negotiate", "compensation"] },
];

pub fn match_skill(text: &str) -> Option<&'static Skill> {
    let lower = text.to_lowercase();
    SKILLS.iter().find(|s| s.keywords.iter().any(|k| lower.contains(k)))
}
```
**Acceptance criteria**:
- "implement a binary search" → matches DSA skill
- "design a URL shortener" → matches system_design skill
- Matched skill template injected into Solve/Think prompt
- No skill match → generic prompt (no skill section)
**Verification commands**:
- `cargo test -p cue-daemon prompts::skills`
**Risks + mitigations**:
- Wrong skill match → user can override via /snap (bypasses skill) or future skill picker in dashboard

---

### Task R5 🟡 [S] Overlay Mode-Indicator Badge
**Layer**: NATIVE
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: R1, D0.4
**Summary**: Overlay renders a small badge showing current lane (Snap/Solve/Think) + model name. Badge updates on each generation start via `ModeIndicator` overlay command. Color-coded: green=Snap, blue=Solve, purple=Think.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md R5 description, D0.4 protocol
**Code sketch**:
```swift
// native/macos/cue-overlay/main.swift — addition to card rendering
// Handle ModeIndicator command
case "ModeIndicator":
    let lane = json["lane"] as? String ?? "solve"
    let model = json["model"] as? String ?? ""
    DispatchQueue.main.async {
        self.laneBadge.stringValue = "\(lane.uppercased()) · \(model)"
        self.laneBadge.backgroundColor = lane == "snap" ? .systemGreen.withAlphaComponent(0.2) :
            lane == "think" ? .systemPurple.withAlphaComponent(0.2) : .systemBlue.withAlphaComponent(0.2)
    }
```
**Acceptance criteria**:
- Badge visible in overlay header area
- Updates within 100ms of generation start
- Shows lane name + abbreviated model (e.g., "SNAP · cerebras")
- Badge hidden when no active generation
**Verification commands**:
- Send `{"type":"ModeIndicator","lane":"snap","model":"cerebras"}` to overlay socket → badge appears
**Risks + mitigations**:
- Badge takes space → keep it small (12px font, pill shape)


---

## Batch 4C — Patch-Mode Follow-ups (weeks 3-5)

### Task F1 🔴 [M] Three-Lane Router Integration Point
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: R1, B3.1
**Summary**: Central dispatch function that ties together: intent classification → lane selection → prompt composition → provider dispatch → stream emission. This is the main entry point for all user queries. Handles progressive enhancement (R2) when confidence is low.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 Router Decision Flow
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/dispatch.rs
use crate::llm::{Lane, LlmRequest, LlmRouter, ChatMessage};
use crate::prompts::{PromptComposer, skills};
use crate::routing::classifier;

pub async fn handle_query(
    router: &LlmRouter,
    composer: &PromptComposer,
    text: &str,
    context: &str,
    session_id: &str,
    explicit_override: Option<Lane>,
    overlay_tx: &tokio::sync::mpsc::Sender<String>,
) -> anyhow::Result<(String, Lane)> {
    let question_type = cue_core::intelligence::classify_utterance(text);
    let (override_lane, clean_text) = crate::routing::overrides::parse_override(text);
    let effective_override = override_lane.or(explicit_override);

    let result = classifier::classify_intent(clean_text, question_type, effective_override);

    // Progressive enhancement for low confidence
    if result.confidence < 0.7 && effective_override.is_none() {
        return crate::routing::progressive::progressive_dispatch(router, &LlmRequest {
            messages: vec![ChatMessage { role: "user".into(), content: clean_text.into() }],
            lane: result.lane, max_tokens: 4000, temperature: 0.3, session_id: session_id.into(),
        }, overlay_tx).await.map_err(Into::into);
    }

    let skill = skills::match_skill(clean_text);
    let system_prompt = composer.compose(result.lane, skill.map(|s| s.template), context);

    let request = LlmRequest {
        messages: vec![
            ChatMessage { role: "system".into(), content: system_prompt },
            ChatMessage { role: "user".into(), content: clean_text.into() },
        ],
        lane: result.lane, max_tokens: crate::prompts::context_budget::budget_for_lane(result.lane).max_output_tokens,
        temperature: 0.3, session_id: session_id.into(),
    };

    let cancel = tokio_util::sync::CancellationToken::new();
    let token_rx = router.dispatch(&request, cancel).await?;
    let emitter = crate::llm::stream_emitter::StreamEmitter::new(overlay_tx.clone(), 0);
    let response = emitter.run(token_rx, tokio_util::sync::CancellationToken::new()).await;

    Ok((response.unwrap_or_default(), result.lane))
}
```
**Acceptance criteria**:
- Query flows through: classify → compose → dispatch → stream → persist
- Low confidence triggers progressive enhancement
- High confidence dispatches directly to selected lane
- Override bypasses classifier entirely
**Verification commands**:
- `cargo test -p cue-daemon routing::dispatch` (integration test with mock providers)
**Risks + mitigations**:
- Complex orchestration → extensive integration tests with mock providers

---

### Task F2 🔴 [M] Solve-Lane Streaming (Claude Sonnet 4.5)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: F1, B3.5
**Summary**: Anthropic-specific streaming implementation for the Solve lane. Handles Claude's SSE format (`event: content_block_delta`), supports prompt caching headers (prep for CO1), and skill template dispatch. Streaming tokens flow through the 60Hz emitter to overlay.
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §1 (Anthropic provider)
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/providers/anthropic.rs
pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: crate::llm::credentials::SecretString,
    model: String,
}

#[async_trait::async_trait]
impl crate::llm::LlmProvider for AnthropicProvider {
    fn name(&self) -> &str { "anthropic" }
    fn supported_lanes(&self) -> &[crate::llm::Lane] { &[crate::llm::Lane::Solve, crate::llm::Lane::Think] }

    async fn stream(&self, request: &crate::llm::LlmRequest, cancel: tokio_util::sync::CancellationToken,
    ) -> anyhow::Result<tokio::sync::mpsc::Receiver<crate::llm::Token>> {
        let (tx, rx) = tokio::sync::mpsc::channel(64);
        let body = serde_json::json!({
            "model": &self.model,
            "max_tokens": request.max_tokens,
            "stream": true,
            "messages": request.messages.iter().map(|m| serde_json::json!({"role": &m.role, "content": &m.content})).collect::<Vec<_>>(),
        });
        let client = self.client.clone();
        let key = self.api_key.expose().to_string();
        let cancel_clone = cancel.clone();

        tokio::spawn(async move {
            let resp = client.post("https://api.anthropic.com/v1/messages")
                .header("x-api-key", &key)
                .header("anthropic-version", "2023-06-01")
                .header("anthropic-beta", "prompt-caching-2024-07-31")
                .json(&body).send().await;
            if let Ok(resp) = resp {
                use futures::StreamExt;
                let mut stream = resp.bytes_stream();
                let mut buf = String::new();
                while let Some(Ok(chunk)) = stream.next().await {
                    if cancel_clone.is_cancelled() { break; }
                    buf.push_str(&String::from_utf8_lossy(&chunk));
                    while let Some(pos) = buf.find('\n') {
                        let line = buf[..pos].to_string();
                        buf = buf[pos+1..].to_string();
                        if line.starts_with("data: ") {
                            let data = &line[6..];
                            if let Ok(v) = serde_json::from_str::<serde_json::Value>(data) {
                                if v["type"] == "content_block_delta" {
                                    if let Some(text) = v["delta"]["text"].as_str() {
                                        let _ = tx.send(crate::llm::Token { text: text.to_string(), done: false }).await;
                                    }
                                } else if v["type"] == "message_stop" {
                                    let _ = tx.send(crate::llm::Token { text: String::new(), done: true }).await;
                                }
                            }
                        }
                    }
                }
            }
        });
        Ok(rx)
    }
}
```
**Acceptance criteria**:
- Claude Sonnet 4.5 streams tokens via SSE
- Prompt caching header included (actual caching in CO1, Phase 10)
- CancellationToken stops reading stream
- Handles `message_stop` event as completion signal
**Verification commands**:
- `cargo test -p cue-daemon llm::providers::anthropic` (mock SSE server)
- Manual: set ANTHROPIC_API_KEY, send coding question, see streaming response
**Risks + mitigations**:
- Anthropic API changes → pin `anthropic-version` header

---

### Task F3 🔴 [L] Think-Lane Integration (o3 + Visible Thinking)
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: F1
**Summary**: o3/Claude Opus integration for the Think lane. Supports extended thinking with progress indication. For o3: uses `reasoning_effort: "high"`. For Claude Opus: uses `thinking` block in response. Overlay shows progress bar during 15-60s thinking period. Visible-thinking text displayed in collapsed section.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md Part 3 (Think Lane spec)
**Code sketch**:
```rust
// crates/cue-daemon/src/llm/providers/think_lane.rs
pub async fn dispatch_think(
    provider: &dyn crate::llm::LlmProvider,
    request: &crate::llm::LlmRequest,
    overlay_tx: &tokio::sync::mpsc::Sender<String>,
    cancel: tokio_util::sync::CancellationToken,
) -> anyhow::Result<String> {
    // Send progress start to overlay
    let _ = overlay_tx.send("{\"type\":\"progress\",\"state\":\"thinking\"}".to_string()).await;

    let token_rx = provider.stream(request, cancel.clone()).await?;
    let mut full = String::new();
    let mut thinking_text = String::new();
    let mut in_thinking = false;
    let mut rx = token_rx;

    while let Some(token) = rx.recv().await {
        if cancel.is_cancelled() { break; }
        if token.done { break; }
        // Detect thinking blocks (Claude format: <thinking>...</thinking>)
        if token.text.contains("<thinking>") { in_thinking = true; continue; }
        if token.text.contains("</thinking>") { in_thinking = false; continue; }
        if in_thinking {
            thinking_text.push_str(&token.text);
            // Update progress with thinking preview
            let _ = overlay_tx.send(format!("{{\"type\":\"progress\",\"state\":\"thinking\",\"preview\":\"{}\"}}", 
                thinking_text.chars().take(100).collect::<String>())).await;
        } else {
            full.push_str(&token.text);
            let _ = overlay_tx.send(token.text).await;
        }
    }

    let _ = overlay_tx.send("{\"type\":\"progress\",\"state\":\"done\"}".to_string()).await;
    Ok(full)
}
```
**Acceptance criteria**:
- Think lane shows progress indicator in overlay during reasoning
- Thinking text (Claude `<thinking>` blocks) captured but not shown as main answer
- Progress updates every ~2s with thinking preview
- Final answer rendered after thinking completes
- Timeout at 60s with partial result returned
**Verification commands**:
- `cargo test -p cue-daemon llm::providers::think_lane` (mock slow provider)
- Manual: `/think explain quantum computing` → progress bar → answer
**Risks + mitigations**:
- 60s timeout → show partial answer + "thinking timed out" badge

---

### Task F4 🟡 [M] Follow-up Intent Classifier
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: R1, F8
**Summary**: Determines if a new utterance is a refinement of the previous answer (→ patch mode) or a new problem (→ fresh dispatch). Uses topic fingerprint similarity (F8) + keyword signals ("also", "what about", "can you change" = refinement; new question words = fresh).
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md F4 description
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/followup.rs
pub enum FollowUpIntent { Refinement, NewProblem }

pub fn classify_followup(
    current_text: &str,
    previous_text: &str,
    topic_similarity: f32, // from F8
) -> FollowUpIntent {
    let lower = current_text.to_lowercase();

    // Strong refinement signals
    let refine_signals = ["also", "what about", "can you change", "modify", "add to that",
        "remove the", "instead", "but what if", "elaborate", "more detail"];
    if refine_signals.iter().any(|s| lower.contains(s)) {
        return FollowUpIntent::Refinement;
    }

    // Topic similarity threshold
    if topic_similarity > 0.75 && current_text.split_whitespace().count() < 15 {
        return FollowUpIntent::Refinement;
    }

    FollowUpIntent::NewProblem
}
```
**Acceptance criteria**:
- "can you add error handling" after code answer → Refinement
- "what's the time complexity of quicksort" after behavioral answer → NewProblem
- Topic similarity >0.75 + short utterance → Refinement
- ≥10 unit tests
**Verification commands**:
- `cargo test -p cue-daemon routing::followup`
**Risks + mitigations**:
- Topic similarity not available until F8 → default to keyword-only initially

---

### Task F5 🔴 [M] Patch-Mode PATCH/KEEP/MODIFY/ADD/REMOVE Parser
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: F2
**Summary**: When follow-up is classified as Refinement, prompt the LLM to respond in patch format. Parse the structured response into diff blocks. Format: each section of the previous response is tagged KEEP (unchanged), MODIFY (replacement text), ADD (new section), REMOVE (delete section). Prompt instructs LLM to use this format.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md "Patch-mode follow-ups use PATCH/KEEP/MODIFY/ADD/REMOVE"
**Code sketch**:
```rust
// crates/cue-daemon/src/patch/parser.rs
use cue_core::overlay::DiffAction;

#[derive(Debug, Clone)]
pub struct PatchBlock { pub action: DiffAction, pub content: String }

pub fn parse_patch_response(response: &str) -> Vec<PatchBlock> {
    let mut blocks = Vec::new();
    let mut current_action = DiffAction::Keep;
    let mut current_content = String::new();

    for line in response.lines() {
        let trimmed = line.trim();
        if let Some(new_action) = parse_action_tag(trimmed) {
            if !current_content.is_empty() {
                blocks.push(PatchBlock { action: current_action, content: std::mem::take(&mut current_content) });
            }
            current_action = new_action;
        } else {
            if !current_content.is_empty() { current_content.push('\n'); }
            current_content.push_str(line);
        }
    }
    if !current_content.is_empty() {
        blocks.push(PatchBlock { action: current_action, content: current_content });
    }
    blocks
}

fn parse_action_tag(line: &str) -> Option<DiffAction> {
    match line {
        "[KEEP]" => Some(DiffAction::Keep),
        "[MODIFY]" => Some(DiffAction::Modify),
        "[ADD]" => Some(DiffAction::Add),
        "[REMOVE]" => Some(DiffAction::Remove),
        _ => None,
    }
}

pub const PATCH_PROMPT_SUFFIX: &str = r#"
The user is refining their previous answer. Respond using PATCH format:
- [KEEP] sections that don't change (include the text)
- [MODIFY] sections with updated text
- [ADD] new sections to insert
- [REMOVE] sections to delete
Start each section with the tag on its own line."#;
```
**Acceptance criteria**:
- Parser correctly splits response into typed blocks
- `[KEEP]` blocks preserve original text
- `[MODIFY]` blocks contain replacement text
- `[ADD]`/`[REMOVE]` blocks handled correctly
- Malformed response (no tags) → treated as single MODIFY block (graceful fallback)
**Verification commands**:
- `cargo test -p cue-daemon patch::parser` — 8+ test cases
**Risks + mitigations**:
- LLM doesn't follow format → fallback: treat entire response as replacement (no diff)

---

### Task F6 🟡 [S] Response-Block State Tracker
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: F5
**Summary**: Tracks the currently displayed response as a list of content blocks. When a patch arrives, applies the diff to produce the new displayed state. Maintains block IDs for overlay rendering.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md F6 description
**Code sketch**:
```rust
// crates/cue-daemon/src/patch/state.rs
pub struct ResponseState {
    blocks: Vec<ContentBlock>,
}

#[derive(Clone)]
struct ContentBlock { id: String, text: String }

impl ResponseState {
    pub fn new(initial_response: &str) -> Self {
        Self { blocks: vec![ContentBlock { id: uuid::Uuid::new_v4().to_string(), text: initial_response.to_string() }] }
    }

    pub fn apply_patch(&mut self, patches: &[super::parser::PatchBlock]) -> Vec<cue_core::overlay::DiffBlock> {
        let mut diff_blocks = Vec::new();
        self.blocks.clear();
        for patch in patches {
            let block = ContentBlock { id: uuid::Uuid::new_v4().to_string(), text: patch.content.clone() };
            self.blocks.push(block);
            diff_blocks.push(cue_core::overlay::DiffBlock { action: patch.action, content: patch.content.clone() });
        }
        diff_blocks
    }

    pub fn full_text(&self) -> String {
        self.blocks.iter().map(|b| b.text.as_str()).collect::<Vec<_>>().join("\n")
    }
}
```
**Acceptance criteria**:
- Initial response stored as single block
- `apply_patch` produces diff blocks for overlay
- `full_text()` returns current complete response
- State is per-session, per-message
**Verification commands**:
- `cargo test -p cue-daemon patch::state`
**Risks + mitigations**:
- None — simple state management

---

### Task F7 🔴 [M] Overlay Diff Rendering
**Layer**: NATIVE
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: F5, F6, L5
**Summary**: Overlay renders patch diffs with visual highlighting. MODIFY blocks highlighted in yellow (fade after 1s). ADD blocks highlighted in green (fade after 1s). REMOVE blocks shown with strikethrough briefly then removed. Uses the `PatchDiff` overlay command from D0.4.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md F7 "Highlight modified/added/removed"
**Code sketch**:
```swift
// native/macos/cue-overlay/main.swift — diff rendering
case "PatchDiff":
    guard let blocks = json["blocks"] as? [[String: Any]] else { return }
    DispatchQueue.main.async {
        self.clearCards()
        for block in blocks {
            let action = block["action"] as? String ?? "KEEP"
            let content = block["content"] as? String ?? ""
            let view = self.createTextView(content)
            switch action {
            case "MODIFY":
                view.layer?.backgroundColor = NSColor.systemYellow.withAlphaComponent(0.15).cgColor
                self.fadeBackground(view, delay: 1.0)
            case "ADD":
                view.layer?.backgroundColor = NSColor.systemGreen.withAlphaComponent(0.15).cgColor
                self.fadeBackground(view, delay: 1.0)
            case "REMOVE":
                view.alphaValue = 0.5
                // Attributed string with strikethrough
                let attr = NSAttributedString(string: content, attributes: [.strikethroughStyle: NSUnderlineStyle.single.rawValue])
                view.textStorage?.setAttributedString(attr)
                DispatchQueue.main.asyncAfter(deadline: .now() + 1.5) { view.removeFromSuperview() }
            default: break // KEEP — no highlight
            }
            self.cardStack.addArrangedSubview(view)
        }
    }
```
**Acceptance criteria**:
- MODIFY blocks show yellow highlight that fades after 1s
- ADD blocks show green highlight that fades after 1s
- REMOVE blocks show strikethrough then disappear after 1.5s
- KEEP blocks render normally (no highlight)
- Smooth animation (no flicker)
**Verification commands**:
- Send PatchDiff command via socket with mixed block types → visual verification
**Risks + mitigations**:
- Animation performance → use Core Animation layers (already in overlay architecture)

---

### Task F8 🟡 [S] Topic Fingerprint Embedding
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B5.3 (embedding trait — but can use simple TF-IDF as interim)
**Summary**: Compute a lightweight topic fingerprint for each utterance to determine if consecutive queries are about the same topic. Used by F4 (follow-up classifier) for similarity check. Interim: TF-IDF cosine similarity (no external API call). Future: embedding via B5.3.
**Design source**: CUE-BLUEY-V3-PLAN-SKELETON.md F8 "Similarity continuity check"
**Code sketch**:
```rust
// crates/cue-daemon/src/routing/fingerprint.rs
use std::collections::HashMap;

pub fn cosine_similarity(a: &str, b: &str) -> f32 {
    let tf_a = term_freq(a);
    let tf_b = term_freq(b);
    let dot: f32 = tf_a.iter().map(|(k, v)| v * tf_b.get(k.as_str()).unwrap_or(&0.0)).sum();
    let mag_a: f32 = tf_a.values().map(|v| v * v).sum::<f32>().sqrt();
    let mag_b: f32 = tf_b.values().map(|v| v * v).sum::<f32>().sqrt();
    if mag_a == 0.0 || mag_b == 0.0 { return 0.0; }
    dot / (mag_a * mag_b)
}

fn term_freq(text: &str) -> HashMap<&str, f32> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let len = words.len() as f32;
    let mut freq = HashMap::new();
    for w in words { *freq.entry(w).or_insert(0.0) += 1.0 / len; }
    freq
}
```
**Acceptance criteria**:
- Same topic → similarity >0.6 (e.g., "implement binary search" vs "add error handling to the search")
- Different topic → similarity <0.3 (e.g., "binary search" vs "tell me about yourself")
- Computation <1ms (no I/O)
**Verification commands**:
- `cargo test -p cue-daemon routing::fingerprint`
**Risks + mitigations**:
- TF-IDF is crude → upgrade to embedding similarity in Phase 5 when B5.3 is available

---

### Task B3.11 🟢 [S] Triple-Layer Language Injection
**Layer**: DAEMON
**Status from CUE-CURRENT-STATE**: ❌ NOT STARTED
**Depends on**: B4.1
**Summary**: Inject language instructions at 3 points in the prompt (header, inline, footer) to enforce non-English response language. Only activates when user's configured language ≠ "en"/"auto".
**Design source**: CUE-DESIGN-03-LLM-PROMPTS-RAG.md §11 Triple-Layer Language Injection
**Code sketch**:
```rust
// crates/cue-daemon/src/prompts/language.rs
pub fn inject_language(prompt: &str, language: &str) -> String {
    if language == "auto" || language == "en" { return prompt.to_string(); }
    format!(
        "[LANGUAGE: Respond entirely in {lang}. This overrides all other instructions.]\n\n\
         {prompt}\n\n\
         [REMINDER: Your entire response MUST be in {lang}.]",
        lang = language, prompt = prompt
    )
}
```
**Acceptance criteria**:
- English/auto → no injection (prompt unchanged)
- "es" → Spanish instruction header + footer added
- Injection doesn't break XML tags in prompt
**Verification commands**:
- `cargo test -p cue-daemon prompts::language`
**Risks + mitigations**:
- Models may still respond in English → this is a best-effort instruction

---

## Phase deliverable
- User speaks → intent classified → routed to correct lane → streaming response in overlay
- Three lanes working: Snap (<300ms), Solve (streaming), Think (progress bar)
- Follow-up questions produce patch diffs with visual highlighting
- Manual overrides (Alt+D, /think, /snap) work instantly
- Progressive enhancement: ambiguous queries show Snap then upgrade to Solve
- Codex review should check: cancel-on-upgrade actually drops Snap answer, patch parser handles malformed input gracefully, rate limiters prevent 429s, no API keys in logs


---

# Part B — PHASES 5-10 (Execution plan with code sketches)

# bluey Master Plan V3 — Part B: Phases 5-10

**Author**: Principal Engineer (Step 2B of 4)
**Date**: 2026-05-12
**Scope**: Memory/RAG, Dashboard Polish, Ops/Security, Dev Discipline, Latency Engineering, Cost Optimization
**Target**: ~1800 lines, compilable code sketches, design-doc citations

---

## Phase 5: Memory + RAG + Context Management (~2 weeks)

### Entry Criteria
- B3.1 (LLM trait) merged — embedding providers need the trait interface
- C0.1 (SQLite session model) merged — vector store shares the DB
- Phase 4 Batch 4A complete (streaming infrastructure available)

### Exit Criteria
- `cargo test --features rag` passes with mock embeddings
- Live indexing produces searchable chunks within 2s of transcript final
- Epoch summarization triggers at 50K token threshold
- Hybrid retrieval returns relevant chunks for test queries (precision >0.7 on eval set)
- Context assembly produces lane-appropriate payloads under token budgets

### Batch Structure

**PR 5A — Vector Infrastructure (days 1-4)**:
- B5.1 sqlite-vec vector store
- B5.2 SemanticChunker
- B5.3 Embedding trait + 4 providers

**PR 5B — Live Indexing + Search (days 5-8)**:
- B5.4 Live RAG indexer
- B5.7 Async vector search
- B5.8 Hybrid retrieval

**PR 5C — Context Management (days 9-14)**:
- CM1 SessionState + Turn model
- CM2 Context-assembly (lane-aware)
- CM3 Epoch summarization
- CM4 Token counter + compaction trigger

---

### B5.1 — sqlite-vec Vector Store

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | C0.1 (SQLite), D0.1 (Tauri scaffold) |
| **Design source** | CUE-DESIGN-03:L1200-1270 (VectorStore.ts pattern #13) |

**Summary**: Local vector store using sqlite-vec extension with per-dimension virtual tables. Supports insert, search (KNN), and delete operations.

**Code sketch**:
```rust
// src-tauri/src/rag/vector_store.rs
use rusqlite::{Connection, params};
use std::path::Path;
use anyhow::Result;

pub struct VectorStore {
    conn: Connection,
    dim: u32,
}

pub struct SearchResult {
    pub chunk_id: String,
    pub distance: f32,
}

impl VectorStore {
    pub fn new(db_path: &Path, dim: u32) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        unsafe { conn.load_extension("vec0", None)?; }
        conn.execute_batch(&format!(
            "CREATE VIRTUAL TABLE IF NOT EXISTS vec_chunks_{dim}
             USING vec0(embedding float[{dim}], chunk_id TEXT, session_id TEXT);"
        ))?;
        Ok(Self { conn, dim })
    }

    pub fn insert(&self, chunk_id: &str, session_id: &str, embedding: &[f32]) -> Result<()> {
        self.conn.execute(
            &format!("INSERT INTO vec_chunks_{} (embedding, chunk_id, session_id) VALUES (?, ?, ?)", self.dim),
            params![embedding_to_blob(embedding), chunk_id, session_id],
        )?;
        Ok(())
    }

    pub fn search(&self, query: &[f32], limit: usize) -> Result<Vec<SearchResult>> {
        let mut stmt = self.conn.prepare(&format!(
            "SELECT chunk_id, distance FROM vec_chunks_{} WHERE embedding MATCH ? ORDER BY distance LIMIT ?",
            self.dim
        ))?;
        let rows = stmt.query_map(params![embedding_to_blob(query), limit as i64], |row| {
            Ok(SearchResult { chunk_id: row.get(0)?, distance: row.get(1)? })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    pub fn delete_session(&self, session_id: &str) -> Result<()> {
        self.conn.execute(
            &format!("DELETE FROM vec_chunks_{} WHERE session_id = ?", self.dim),
            params![session_id],
        )?;
        Ok(())
    }
}

fn embedding_to_blob(v: &[f32]) -> Vec<u8> {
    v.iter().flat_map(|f| f.to_le_bytes()).collect()
}
```

**Acceptance**: Insert 1000 chunks, KNN search returns correct top-5 by cosine distance in <50ms.

**Verification**: Integration test with known embeddings; verify distance ordering matches manual cosine computation.

**Risks**: sqlite-vec extension loading may conflict with tauri-plugin-sql's bundled SQLite. Mitigation: use separate rusqlite connection for RAG (not the Tauri plugin DB).

---

### B5.2 — SemanticChunker

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | None (pure logic) |
| **Design source** | CUE-DESIGN-03:L1275-1360 (SemanticChunker.ts pattern #115) |

**Summary**: Speaker-aware chunker with sliding-window overlap. Parameters: TARGET=300, MAX=400, MIN=100, OVERLAP=50 tokens.

**Code sketch**:
```rust
// src-tauri/src/rag/chunker.rs
const TARGET_TOKENS: usize = 300;
const MAX_TOKENS: usize = 400;
const MIN_TOKENS: usize = 100;
const OVERLAP_TOKENS: usize = 50;

#[derive(Clone, Debug)]
pub struct Chunk {
    pub id: String,
    pub text: String,
    pub speaker: Option<String>,
    pub token_count: usize,
    pub start_ms: u64,
    pub end_ms: u64,
}

pub struct TranscriptSegment {
    pub text: String,
    pub speaker: String,
    pub timestamp_ms: u64,
}

pub fn chunk_segments(segments: &[TranscriptSegment]) -> Vec<Chunk> {
    let mut chunks = Vec::new();
    let mut buf = String::new();
    let mut tok_count = 0usize;
    let mut start_ms = 0u64;
    let mut speaker: Option<&str> = None;

    for seg in segments {
        let seg_tok = seg.text.len() / 4;
        let speaker_changed = speaker.is_some() && speaker != Some(&seg.speaker);

        if (speaker_changed || tok_count + seg_tok > MAX_TOKENS) && tok_count >= MIN_TOKENS {
            chunks.push(Chunk {
                id: uuid::Uuid::new_v4().to_string(),
                text: buf.clone(),
                speaker: speaker.map(String::from),
                token_count: tok_count,
                start_ms,
                end_ms: seg.timestamp_ms,
            });
            let overlap = take_tail(&buf, OVERLAP_TOKENS * 4);
            buf = overlap;
            tok_count = buf.len() / 4;
            start_ms = seg.timestamp_ms;
        }

        if buf.is_empty() { start_ms = seg.timestamp_ms; }
        if !buf.is_empty() { buf.push(' '); }
        buf.push_str(&seg.text);
        tok_count += seg_tok;
        speaker = Some(&seg.speaker);
    }

    if tok_count >= MIN_TOKENS {
        chunks.push(Chunk {
            id: uuid::Uuid::new_v4().to_string(),
            text: buf,
            speaker: speaker.map(String::from),
            token_count: tok_count,
            start_ms,
            end_ms: segments.last().map(|s| s.timestamp_ms).unwrap_or(0),
        });
    }
    chunks
}

fn take_tail(s: &str, chars: usize) -> String {
    if s.len() <= chars { s.to_string() } else { s[s.len() - chars..].to_string() }
}
```

**Acceptance**: 10-minute transcript (600 segments) produces chunks all within [100, 400] token range; overlap verified between consecutive chunks.

**Verification**: Unit test with synthetic segments; assert no chunk exceeds MAX, speaker boundaries respected.

**Risks**: Token estimation (len/4) may drift for non-English. Mitigation: swap to tiktoken-rs if accuracy matters post-v1.

---

### B5.3 — Embedding Trait + 4 Providers

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | B3.1 (LLM trait for HTTP client reuse) |
| **Design source** | CUE-DESIGN-03:L1365-1430 (EmbeddingPipeline.ts pattern #12) |

**Summary**: Async trait with cascading fallback: OpenAI text-embedding-3-small (1536d) → Gemini text-embedding-004 (768d) → Cohere embed-v3 (1024d) → local all-MiniLM-L6-v2 ONNX (384d).

**Code sketch**:
```rust
// src-tauri/src/rag/embedding.rs
use async_trait::async_trait;
use anyhow::Result;

#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    fn name(&self) -> &str;
    fn dimension(&self) -> u32;
    async fn embed(&self, text: &str) -> Result<Vec<f32>>;
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>>;
}

pub struct EmbeddingResolver {
    providers: Vec<Box<dyn EmbeddingProvider>>,
}

impl EmbeddingResolver {
    pub fn new(providers: Vec<Box<dyn EmbeddingProvider>>) -> Self {
        Self { providers }
    }

    pub async fn embed(&self, text: &str) -> Result<(Vec<f32>, u32)> {
        for p in &self.providers {
            match p.embed(text).await {
                Ok(emb) => return Ok((emb, p.dimension())),
                Err(e) => tracing::warn!(provider = p.name(), err = %e, "embedding failed, trying next"),
            }
        }
        anyhow::bail!("all embedding providers failed")
    }
}

// OpenAI implementation (1536-dim)
pub struct OpenAIEmbedding { pub client: reqwest::Client, pub api_key: String }

#[async_trait]
impl EmbeddingProvider for OpenAIEmbedding {
    fn name(&self) -> &str { "openai" }
    fn dimension(&self) -> u32 { 1536 }

    async fn embed(&self, text: &str) -> Result<Vec<f32>> {
        let resp = self.client.post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({"input": text, "model": "text-embedding-3-small"}))
            .send().await?.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        let arr = body["data"][0]["embedding"].as_array().unwrap();
        Ok(arr.iter().map(|v| v.as_f64().unwrap() as f32).collect())
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>> {
        let resp = self.client.post("https://api.openai.com/v1/embeddings")
            .bearer_auth(&self.api_key)
            .json(&serde_json::json!({"input": texts, "model": "text-embedding-3-small"}))
            .send().await?.error_for_status()?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body["data"].as_array().unwrap().iter()
            .map(|d| d["embedding"].as_array().unwrap().iter()
                .map(|v| v.as_f64().unwrap() as f32).collect())
            .collect())
    }
}
```

**Acceptance**: Each provider returns correct-dimension vector; fallback chain skips failed provider and succeeds on next.

**Verification**: Mock HTTP responses; verify dimension matches; test cascade with first provider returning 500.

**Risks**: Dimension mismatch if user switches providers mid-session. Mitigation: per-dimension vec0 tables (B5.1 already handles this).

---

### B5.4 — Live RAG Indexer (JIT)

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | B5.1, B5.2, B5.3 |
| **Design source** | CUE-DESIGN-03:L1435-1470 (LiveRAGIndexer.ts pattern #14) |

**Summary**: Feeds final transcript segments in real-time, chunks when buffer hits TARGET, embeds, and inserts. Searchable within 2s.

**Code sketch**:
```rust
// src-tauri/src/rag/live_indexer.rs
use tokio::sync::mpsc;

pub struct LiveIndexer {
    tx: mpsc::Sender<TranscriptSegment>,
}

impl LiveIndexer {
    pub fn spawn(
        embedding: Arc<EmbeddingResolver>,
        store: Arc<VectorStore>,
        session_id: String,
    ) -> Self {
        let (tx, mut rx) = mpsc::channel::<TranscriptSegment>(256);
        tokio::spawn(async move {
            let mut buf: Vec<TranscriptSegment> = Vec::new();
            while let Some(seg) = rx.recv().await {
                buf.push(seg);
                let tok_est: usize = buf.iter().map(|s| s.text.len() / 4).sum();
                if tok_est >= TARGET_TOKENS {
                    let chunks = chunk_segments(&buf);
                    for chunk in &chunks {
                        if let Ok((emb, _)) = embedding.embed(&chunk.text).await {
                            let _ = store.insert(&chunk.id, &session_id, &emb);
                        }
                    }
                    // Keep overlap
                    let keep = buf.len().saturating_sub(2);
                    buf.drain(..keep);
                }
            }
        });
        Self { tx }
    }

    pub async fn feed(&self, segment: TranscriptSegment) -> Result<()> {
        self.tx.send(segment).await.map_err(|_| anyhow::anyhow!("indexer closed"))
    }
}
```

**Acceptance**: Feed 50 segments at 1/s; query after 15s returns relevant chunks from earlier in session.

**Verification**: Integration test with mock embedding (identity vectors); verify chunks appear in search results.

**Risks**: Embedding latency (200-500ms per chunk) may cause backlog. Mitigation: batch embed when possible; buffer absorbs bursts.

---

### B5.7 — Async Vector Search

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | B5.1 |
| **Design source** | CUE-DESIGN-03:L1530-1560 (vectorSearchWorker.ts pattern #116) |

**Summary**: Non-blocking search via `spawn_blocking` with 30s timeout. Replaces Node.js worker thread pattern.

**Code sketch**:
```rust
// src-tauri/src/rag/search.rs
use std::time::Duration;

pub async fn search_async(
    store: &VectorStore,
    query: &[f32],
    limit: usize,
) -> Result<Vec<SearchResult>> {
    let query = query.to_vec();
    let db_path = store.db_path().to_path_buf();
    let dim = store.dim;

    tokio::time::timeout(Duration::from_secs(30), tokio::task::spawn_blocking(move || {
        let s = VectorStore::new(&db_path, dim)?;
        s.search(&query, limit)
    })).await??
}
```

**Acceptance**: Search completes in <100ms for 10K chunks; timeout fires correctly at 30s for pathological cases.

**Verification**: Benchmark with 10K synthetic embeddings; verify timeout with artificially slow query.

**Risks**: Opening new connection per search adds ~5ms. Mitigation: acceptable for cold-path; hot-path uses connection pool.

---

### B5.8 — Hybrid Retrieval (Vector + BM25)

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | 🟡 Partial (BM25 logic designed) |
| **Depends** | B5.1, B5.7 |
| **Design source** | CUE-DESIGN-03:L1565-1640 (RAGRetriever.ts pattern #61) |

**Summary**: Combine vector similarity (weight 0.7) with keyword BM25 scoring (weight 0.3) for robust retrieval.

**Code sketch**:
```rust
// src-tauri/src/rag/hybrid.rs
use std::collections::HashMap;

pub struct HybridRetriever {
    pub vector_weight: f32, // 0.7
}

impl HybridRetriever {
    pub fn merge(
        &self,
        vector_results: &[SearchResult],
        keyword_results: &[(String, f32)], // (chunk_id, bm25_score)
        limit: usize,
    ) -> Vec<(String, f32)> {
        let mut scores: HashMap<&str, f32> = HashMap::new();
        for r in vector_results {
            let sim = 1.0 - r.distance;
            *scores.entry(&r.chunk_id).or_default() += sim * self.vector_weight;
        }
        for (id, s) in keyword_results {
            *scores.entry(id.as_str()).or_default() += s * (1.0 - self.vector_weight);
        }
        let mut ranked: Vec<_> = scores.into_iter().map(|(k, v)| (k.to_string(), v)).collect();
        ranked.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        ranked.truncate(limit);
        ranked
    }
}

/// Simple BM25-like keyword scoring on SQLite FTS5
pub fn keyword_search(conn: &Connection, query: &str, limit: usize) -> Result<Vec<(String, f32)>> {
    let mut stmt = conn.prepare(
        "SELECT chunk_id, bm25(chunks_fts) as score FROM chunks_fts WHERE chunks_fts MATCH ? ORDER BY score LIMIT ?"
    )?;
    let rows = stmt.query_map(params![query, limit as i64], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f32>(1)?))
    })?;
    rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
}
```

**Acceptance**: Query "system design scalability" returns chunks about system design (vector) AND chunks containing exact keywords (BM25), merged correctly.

**Verification**: Test with chunks where vector-similar ≠ keyword-match; verify both contribute to final ranking.

**Risks**: FTS5 index adds storage overhead. Mitigation: only index active session chunks; purge on session archive.

---

### CM1 — SessionState + Turn Model

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Core |
| **Status** | ❌ Not started |
| **Depends** | C0.1 (base SQLite schema) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L470 (CM1 spec) |

**Summary**: Extends C0.1 with lane metadata per message, turn tracking, and session state machine.

**Code sketch**:
```rust
// src-tauri/src/session/model.rs
use rusqlite::params;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Lane { Snap, Solve, Think }

#[derive(Debug, Clone)]
pub struct Turn {
    pub id: String,
    pub session_id: String,
    pub role: Role,
    pub content: String,
    pub lane: Option<Lane>,
    pub token_count: u32,
    pub created_at: i64,
}

pub fn migrate_cm1(conn: &Connection) -> Result<()> {
    conn.execute_batch("
        ALTER TABLE messages ADD COLUMN lane TEXT;
        ALTER TABLE messages ADD COLUMN token_count INTEGER DEFAULT 0;
        CREATE INDEX IF NOT EXISTS idx_msg_session_lane ON messages(conversation_id, lane);
    ")?;
    Ok(())
}

pub fn insert_turn(conn: &Connection, turn: &Turn) -> Result<()> {
    conn.execute(
        "INSERT INTO messages (id, conversation_id, role, content, lane, token_count, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![turn.id, turn.session_id, turn.role.as_str(), turn.content,
                turn.lane.map(|l| l.as_str()), turn.token_count, turn.created_at],
    )?;
    Ok(())
}

pub fn get_recent_turns(conn: &Connection, session_id: &str, limit: u32) -> Result<Vec<Turn>> {
    let mut stmt = conn.prepare(
        "SELECT id, conversation_id, role, content, lane, token_count, created_at
         FROM messages WHERE conversation_id = ? ORDER BY created_at DESC LIMIT ?"
    )?;
    // ... map rows to Turn structs
    todo!()
}

impl Lane {
    pub fn as_str(&self) -> &'static str {
        match self { Lane::Snap => "snap", Lane::Solve => "solve", Lane::Think => "think" }
    }
}
```

**Acceptance**: Turns persist with lane metadata; query by session+lane returns correct subset.

**Verification**: Insert turns across all 3 lanes; verify filtered queries return only matching lane.

**Risks**: ALTER TABLE on existing DB with data. Mitigation: use IF NOT EXISTS pattern; test migration on populated DB.

---

### CM2 — Context Assembly (Lane-Aware)

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Core |
| **Status** | ❌ Not started |
| **Depends** | CM1, R1 (intent classifier) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L38-50 (Three-Lane token budgets) |

**Summary**: Four strategies: Fresh (Snap: last 3 turns), Refinement (Solve: RAG-selected turns), LongHistory (Think: full + epoch summaries), LaneUpgrade (carry forward from lower lane).

**Code sketch**:
```rust
// src-tauri/src/session/context.rs

pub struct ContextBudget {
    pub max_input_tokens: u32,
    pub max_turns: u32,
}

pub const SNAP_BUDGET: ContextBudget = ContextBudget { max_input_tokens: 2000, max_turns: 3 };
pub const SOLVE_BUDGET: ContextBudget = ContextBudget { max_input_tokens: 16000, max_turns: 20 };
pub const THINK_BUDGET: ContextBudget = ContextBudget { max_input_tokens: 32000, max_turns: 50 };

pub struct AssembledContext {
    pub system_prompt: String,
    pub turns: Vec<Turn>,
    pub total_tokens: u32,
}

pub fn assemble_context(
    lane: Lane,
    session_id: &str,
    query: &str,
    turns: &[Turn],
    epoch_summaries: &[String],
    rag_results: &[String],
) -> AssembledContext {
    let budget = match lane {
        Lane::Snap => SNAP_BUDGET,
        Lane::Solve => SOLVE_BUDGET,
        Lane::Think => THINK_BUDGET,
    };

    let selected_turns = match lane {
        Lane::Snap => turns.iter().rev().take(budget.max_turns as usize).cloned().collect(),
        Lane::Solve => {
            // Include RAG-retrieved relevant turns + last 5 recent
            let mut ctx: Vec<Turn> = Vec::new();
            ctx.extend(turns.iter().rev().take(5).cloned());
            // RAG results injected as system context, not turns
            ctx
        }
        Lane::Think => {
            let mut ctx: Vec<Turn> = Vec::new();
            // Epoch summaries as synthetic system turns
            for summary in epoch_summaries {
                ctx.push(Turn { role: Role::System, content: summary.clone(), ..Default::default() });
            }
            ctx.extend(turns.iter().cloned());
            ctx
        }
    };

    // Truncate to token budget
    let mut total = 0u32;
    let final_turns: Vec<Turn> = selected_turns.into_iter()
        .take_while(|t| { total += t.token_count; total <= budget.max_input_tokens })
        .collect();

    AssembledContext { system_prompt: String::new(), turns: final_turns, total_tokens: total }
}
```

**Acceptance**: Snap context ≤2K tokens with exactly last 3 turns; Solve includes RAG context; Think includes epoch summaries.

**Verification**: Unit test with 100-turn session; verify each lane strategy produces correct subset under budget.

**Risks**: Token counting drift may cause budget overflows. Mitigation: CM4 provides accurate counts; 10% safety margin.

---

### CM3 — Epoch Summarization Background Job

| Field | Value |
|-------|-------|
| **Layer** | Daemon / RAG |
| **Status** | ❌ Not started |
| **Depends** | CM1, B3.1 (LLM for summarization) |
| **Design source** | CUE-DESIGN-03:L1490-1530 (SessionTracker.ts pattern #17) |

**Summary**: When session token count exceeds 50K, compress oldest 1/3 of turns into a summary paragraph. Max 5 epoch summaries retained.

**Code sketch**:
```rust
// src-tauri/src/rag/epoch.rs
use std::sync::atomic::{AtomicBool, Ordering};

pub struct EpochSummarizer {
    max_tokens_before_compact: u32, // 50_000
    max_summaries: usize,           // 5
    compacting: AtomicBool,
}

impl EpochSummarizer {
    pub async fn maybe_compact(
        &self,
        turns: &mut Vec<Turn>,
        summaries: &mut Vec<String>,
        llm: &dyn LlmProvider,
    ) -> Result<()> {
        let total: u32 = turns.iter().map(|t| t.token_count).sum();
        if total < self.max_tokens_before_compact { return Ok(()); }
        if self.compacting.swap(true, Ordering::SeqCst) { return Ok(()); }

        let drain_count = turns.len() / 3;
        let to_summarize: Vec<_> = turns.drain(..drain_count).collect();
        let text = to_summarize.iter()
            .map(|t| format!("[{}] {}", t.role.as_str(), t.content))
            .collect::<Vec<_>>().join("\n");

        let prompt = format!(
            "Summarize this conversation section in 3-5 sentences. \
             Preserve key questions and answers:\n\n{}", text
        );
        // Use Snap lane (Cerebras) for cheap summarization — $0.001 per call
        let summary = llm.generate_simple(&prompt).await?;
        summaries.push(summary);
        if summaries.len() > self.max_summaries { summaries.remove(0); }

        self.compacting.store(false, Ordering::SeqCst);
        Ok(())
    }
}
```

**Acceptance**: 60K-token session triggers compaction; resulting summaries are coherent; total context drops below 40K.

**Verification**: Feed synthetic 60K-token session; verify compaction fires once; verify summary quality with LLM judge.

**Risks**: Summarization quality with cheap model. Mitigation: use Cerebras (fast+cheap) for summaries; quality acceptable for context, not user-facing.

---

### CM4 — Token Counter + Compaction Trigger

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Core |
| **Status** | ❌ Not started |
| **Depends** | CM1 |
| **Design source** | CUE-DESIGN-03:L1560 (open question #9 — tiktoken vs estimate) |

**Summary**: Accurate token counting via tiktoken-rs for budget enforcement. Triggers CM3 at threshold.

**Code sketch**:
```rust
// src-tauri/src/session/tokens.rs
use tiktoken_rs::cl100k_base;

static ENCODER: once_cell::sync::Lazy<tiktoken_rs::CoreBPE> =
    once_cell::sync::Lazy::new(|| cl100k_base().unwrap());

pub fn count_tokens(text: &str) -> u32 {
    ENCODER.encode_with_special_tokens(text).len() as u32
}

pub fn session_total_tokens(turns: &[Turn]) -> u32 {
    turns.iter().map(|t| t.token_count).sum()
}

pub fn should_compact(turns: &[Turn], threshold: u32) -> bool {
    session_total_tokens(turns) > threshold
}
```

**Acceptance**: Token counts match OpenAI tokenizer within ±1%; compaction trigger fires at exactly 50K threshold.

**Verification**: Compare count_tokens output against OpenAI API token usage for same strings.

**Risks**: tiktoken-rs adds ~2ms per call. Mitigation: count once on insert, cache in Turn.token_count field.

---

### Phase 5 Deliverable

A working local RAG pipeline: transcript → chunk → embed → store → retrieve (hybrid) → assemble (lane-aware context). Epoch summarization keeps long sessions manageable. All searchable within 2s of utterance.



---

## Phase 6: Dashboard Polish (~2 weeks)

### Entry Criteria
- Phase 1+2 complete (dashboard shell, session pages working)
- Phase 4 partial (LLM providers configured, at least Snap lane functional)
- SQLite schema stable (migrations 001-004 applied)

### Exit Criteria
- All 9 settings pages functional with persistence
- Command palette (Cmd+K) navigates to any page/action
- Onboarding flow completes for new users
- Shortcut rebinding works with conflict detection
- System prompts CRUD with AI-generation option

### Batch Structure

**PR 6A — Settings Pages (days 1-5)**:
- B6.2 Rebindable keybinds
- B4.3 Per-provider prompt variants
- B4.4 TINY prompt for Snap lane
- B3.10 testConnection command

**PR 6B — Content Pages (days 6-10)**:
- B4.5 Skill library UI
- B3.6 Structured JSON generation
- B3.7 Custom cURL provider
- B3.8 Codex CLI integration

**PR 6C — Polish + UX (days 11-14)**:
- B6.7 Inertial scroll
- B6.8 Code expansion animation
- B6.9 Command palette
- B6.10 Onboarding flow
- B4.7 System-prompt protection
- B4.9 First-person enforcement

---

### B6.2 — Rebindable Keybinds

| Field | Value |
|-------|-------|
| **Layer** | Dashboard / Settings |
| **Status** | ❌ Not started |
| **Depends** | B6.1 (hotkey system), C0.1 (SQLite) |
| **Design source** | CUE-DESIGN-04:L45-130 (KeybindManager.ts, shortcuts.rs) |

**Summary**: Settings page with ShortcutRecorder component, conflict detection, persist to SQLite, re-register on save.

**Code sketch (Rust)**:
```rust
// src-tauri/src/shortcuts.rs
#[tauri::command]
pub async fn update_shortcuts(
    app: AppHandle,
    bindings: HashMap<String, String>, // action_id → accelerator
) -> Result<(), String> {
    // Validate all accelerators parse
    for (action, accel) in &bindings {
        accel.parse::<tauri::keyboard::Shortcut>()
            .map_err(|e| format!("Invalid shortcut for {action}: {e}"))?;
    }
    // Check for conflicts
    let mut seen = HashMap::new();
    for (action, accel) in &bindings {
        if let Some(existing) = seen.insert(accel.clone(), action.clone()) {
            return Err(format!("Conflict: {accel} bound to both {existing} and {action}"));
        }
    }
    // Unregister all, re-register with new bindings
    app.global_shortcut().unregister_all().map_err(|e| e.to_string())?;
    for (action, accel) in &bindings {
        let shortcut: tauri::keyboard::Shortcut = accel.parse().unwrap();
        let action = action.clone();
        app.global_shortcut().on_shortcut(shortcut, move |_app, _s, _e| {
            dispatch_action(&action);
        }).map_err(|e| e.to_string())?;
    }
    // Persist
    persist_shortcuts_to_db(&bindings).map_err(|e| e.to_string())?;
    Ok(())
}
```

**Code sketch (React)**:
```typescript
// src/pages/Shortcuts.tsx
export function ShortcutRecorder({ value, onChange }: { value: string; onChange: (v: string) => void }) {
  const [recording, setRecording] = useState(false);

  useEffect(() => {
    if (!recording) return;
    const handler = (e: KeyboardEvent) => {
      e.preventDefault();
      const parts: string[] = [];
      if (e.metaKey) parts.push("Cmd");
      if (e.ctrlKey) parts.push("Ctrl");
      if (e.altKey) parts.push("Alt");
      if (e.shiftKey) parts.push("Shift");
      if (e.key.length === 1) parts.push(e.key.toUpperCase());
      else if (e.key !== "Meta" && e.key !== "Control" && e.key !== "Alt" && e.key !== "Shift") {
        parts.push(e.key);
      }
      if (parts.length > 1) { onChange(parts.join("+")); setRecording(false); }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [recording, onChange]);

  return (
    <button onClick={() => setRecording(true)} className="shortcut-recorder">
      {recording ? "Press keys..." : value || "Click to record"}
    </button>
  );
}
```

**Acceptance**: User can rebind any shortcut; conflicts show error; bindings persist across restart.

**Verification**: Rebind Alt+Z to Alt+X; restart app; verify Alt+X triggers toggle.

**Risks**: Platform differences in key naming (Cmd vs Super). Mitigation: normalize in Rust before registration.

---

### B6.9 — Command Palette (Cmd+K)

| Field | Value |
|-------|-------|
| **Layer** | Dashboard / UX |
| **Status** | ❌ Not started |
| **Depends** | B6.3 (sidebar nav) |
| **Design source** | CUE-DESIGN-04:L176 (cmdk library, page structure) |

**Summary**: Spotlight-style command palette using `cmdk` library. Actions: navigate pages, switch models, toggle modes, search sessions.

**Code sketch**:
```typescript
// src/components/CommandPalette.tsx
import { Command } from "cmdk";
import { useNavigate } from "react-router-dom";

const COMMANDS = [
  { id: "nav-chats", label: "Go to Chats", action: "/chats", group: "Navigation" },
  { id: "nav-settings", label: "Go to Settings", action: "/settings", group: "Navigation" },
  { id: "nav-prompts", label: "Go to System Prompts", action: "/system-prompts", group: "Navigation" },
  { id: "model-snap", label: "Switch to Snap mode", action: "mode:snap", group: "Mode" },
  { id: "model-solve", label: "Switch to Solve mode", action: "mode:solve", group: "Mode" },
  { id: "model-think", label: "Switch to Think mode", action: "mode:think", group: "Mode" },
];

export function CommandPalette({ open, onClose }: { open: boolean; onClose: () => void }) {
  const navigate = useNavigate();

  const execute = (action: string) => {
    if (action.startsWith("/")) navigate(action);
    else if (action.startsWith("mode:")) invoke("set_lane_override", { lane: action.split(":")[1] });
    onClose();
  };

  return (
    <Command.Dialog open={open} onOpenChange={(v) => !v && onClose()} label="Command palette">
      <Command.Input placeholder="Type a command..." />
      <Command.List>
        {Object.entries(groupBy(COMMANDS, "group")).map(([group, items]) => (
          <Command.Group key={group} heading={group}>
            {items.map(cmd => (
              <Command.Item key={cmd.id} onSelect={() => execute(cmd.action)}>
                {cmd.label}
              </Command.Item>
            ))}
          </Command.Group>
        ))}
      </Command.List>
    </Command.Dialog>
  );
}
```

**Acceptance**: Cmd+K opens palette; typing filters commands; selecting navigates or executes action.

**Verification**: Open palette, type "chat", verify only chat-related commands shown; select and verify navigation.

**Risks**: cmdk may conflict with Tauri's global shortcut for Cmd+K. Mitigation: register Cmd+K as app-level, not OS-global.

---

### B6.10 — Onboarding Flow

| Field | Value |
|-------|-------|
| **Layer** | Dashboard / UX |
| **Status** | ❌ Not started |
| **Depends** | B6.3 (dashboard) |
| **Design source** | CUE-DESIGN-04:L530-540 (FeatureSpotlight.tsx pattern) |

**Summary**: First-run wizard: API key setup → audio device selection → shortcut overview → test connection. Tracks completion in localStorage.

**Code sketch**:
```typescript
// src/components/Onboarding.tsx
const STEPS = ["welcome", "api-keys", "audio", "shortcuts", "test", "done"] as const;

export function Onboarding() {
  const [step, setStep] = useState(0);
  const completed = localStorage.getItem("onboarding_complete");
  if (completed) return null;

  const finish = () => { localStorage.setItem("onboarding_complete", "true"); };

  return (
    <Dialog open={!completed}>
      <DialogContent className="max-w-lg">
        {STEPS[step] === "welcome" && <WelcomeStep onNext={() => setStep(1)} />}
        {STEPS[step] === "api-keys" && <ApiKeyStep onNext={() => setStep(2)} />}
        {STEPS[step] === "audio" && <AudioStep onNext={() => setStep(3)} />}
        {STEPS[step] === "shortcuts" && <ShortcutStep onNext={() => setStep(4)} />}
        {STEPS[step] === "test" && <TestStep onNext={() => setStep(5)} />}
        {STEPS[step] === "done" && <DoneStep onFinish={finish} />}
      </DialogContent>
    </Dialog>
  );
}
```

**Acceptance**: New install shows onboarding; completing it sets flag; never shows again.

**Verification**: Clear localStorage; relaunch; verify wizard appears; complete all steps; verify flag set.

**Risks**: Users may skip without configuring API keys. Mitigation: "Skip" button warns that features won't work.

---

### Phase 6 Deliverable

Full-featured dashboard with all settings pages, command palette for power users, onboarding for new users, and rebindable shortcuts with conflict detection. All preferences persist to SQLite.



---

## Phase 7: Ops + Security (~2 weeks)

### Entry Criteria
- Phase 0 complete (Tauri binary builds)
- Phase 4 partial (LLM providers exist for cost tracking)
- B3.9 (key scrubbing logic) merged

### Exit Criteria
- All API keys stored in OS keychain (zero plaintext on disk)
- Log rotation at 10MB with NDJSON format
- OpenTelemetry metrics exporting to Grafana Cloud
- Auto-updater checks and installs updates
- Single-instance lock prevents duplicate processes
- Panic handler logs crash context before exit

### Batch Structure

**PR 7A — Security (days 1-4)**:
- B7.1 Keychain integration
- B7.2 Key scrubbing on drop
- B7.3 Log masking utility
- B7.7 Single-instance lock
- B7.10 Panic handler

**PR 7B — Logging + Telemetry (days 5-9)**:
- B7.4 Log rotation + NDJSON
- B8.1 OpenTelemetry init
- B8.2 Metric definitions
- B8.3 Host identity labels
- B8.4 AI pricing table
- B8.5 In-memory ring buffer

**PR 7C — Release Infrastructure (days 10-14)**:
- B9.1 Auto-updater
- B9.3 Autostart on login
- B9.4 PostHog analytics
- B9.5 Anonymous install ping
- B9.6 Machine UID
- B7.5 SQLite migration system

---

### B7.1 — Keychain Integration

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Security |
| **Status** | ❌ Not started |
| **Depends** | D0.1 (Tauri scaffold) |
| **Design source** | CUE-DESIGN-04:L640-680 (tauri-plugin-keychain pattern #10) |

**Summary**: Store all API keys in OS keychain (macOS Keychain / Windows Credential Vault / Linux Secret Service). Zero plaintext storage.

**Code sketch**:
```rust
// src-tauri/src/keychain.rs
use tauri::AppHandle;

const SERVICE: &str = "com.bluey.app";

#[tauri::command]
pub async fn save_api_key(provider: String, key: String) -> Result<(), String> {
    tauri_plugin_keychain::save(SERVICE, &format!("api_key_{provider}"), &key)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_api_key(provider: String) -> Result<Option<String>, String> {
    match tauri_plugin_keychain::get(SERVICE, &format!("api_key_{provider}")) {
        Ok(key) => Ok(Some(key)),
        Err(_) => Ok(None),
    }
}

#[tauri::command]
pub async fn delete_api_key(provider: String) -> Result<(), String> {
    tauri_plugin_keychain::remove(SERVICE, &format!("api_key_{provider}"))
        .map_err(|e| e.to_string())
}
```

**Acceptance**: API key saved via keychain; retrievable after app restart; not present in any file on disk.

**Verification**: Save key; grep entire app data directory for key value; verify zero matches.

**Risks**: Linux Secret Service may not be available on minimal installs. Mitigation: fall back to encrypted file with machine-derived key.

---

### B7.4 — Log Rotation + NDJSON

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Ops |
| **Status** | ❌ Not started |
| **Depends** | None |
| **Design source** | CUE-DESIGN-04:L830-870 (tracing-appender, 10MB rotation) |

**Summary**: Structured NDJSON logs via tracing-subscriber. Rotate at 10MB, keep one backup.

**Code sketch**:
```rust
// src-tauri/src/logging.rs
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use tracing_appender::non_blocking;
use std::fs;

pub fn init(log_dir: &Path) -> Result<tracing_appender::non_blocking::WorkerGuard> {
    fs::create_dir_all(log_dir)?;
    let log_path = log_dir.join("bluey.jsonl");

    // Rotate if over 10MB
    if log_path.exists() && fs::metadata(&log_path)?.len() > 10 * 1024 * 1024 {
        let backup = log_dir.join("bluey.jsonl.1");
        fs::rename(&log_path, &backup)?;
    }

    let file = fs::OpenOptions::new().create(true).append(true).open(&log_path)?;
    let (writer, guard) = non_blocking(file);

    tracing_subscriber::registry()
        .with(EnvFilter::new("bluey=info,warn"))
        .with(fmt::layer().json().with_writer(writer))
        .with(fmt::layer().with_writer(std::io::stderr).compact())
        .init();

    Ok(guard)
}
```

**Acceptance**: Logs written as valid NDJSON; file rotates at 10MB; only one backup retained.

**Verification**: Write 11MB of logs; verify rotation occurred; verify backup exists; verify new file started.

**Risks**: Non-blocking writer may lose final lines on crash. Mitigation: panic handler flushes before exit.

---

### B8.1 — OpenTelemetry Init

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Observability |
| **Status** | ❌ Not started |
| **Depends** | None |
| **Design source** | CUE-DESIGN-04:L780-830 (opentelemetry-otlp setup) |

**Summary**: Initialize OTLP HTTP exporter to Grafana Cloud. Per-lane metrics for TTFT, total latency, token counts, cost.

**Code sketch**:
```rust
// src-tauri/src/telemetry.rs
use opentelemetry::metrics::{Histogram, Counter, Meter};
use opentelemetry_sdk::metrics::SdkMeterProvider;
use opentelemetry_otlp::WithExportConfig;

pub struct Metrics {
    pub ttft_ms: Histogram<f64>,           // time-to-first-token, label: lane
    pub total_ms: Histogram<f64>,          // total generation time, label: lane
    pub input_tokens: Counter<u64>,        // label: lane, model
    pub output_tokens: Counter<u64>,       // label: lane, model
    pub cost_usd: Counter<f64>,            // label: lane, model
    pub stt_latency_ms: Histogram<f64>,
    pub provider_errors: Counter<u64>,     // label: provider, error_class
}

pub fn init(endpoint: &str, auth: &str) -> Result<Metrics> {
    let exporter = opentelemetry_otlp::new_exporter()
        .http()
        .with_endpoint(endpoint)
        .with_headers(std::collections::HashMap::from([
            ("Authorization".to_string(), format!("Basic {auth}")),
        ]));

    let provider = SdkMeterProvider::builder()
        .with_reader(opentelemetry_sdk::metrics::PeriodicReader::builder(exporter.build_metrics_exporter()?).build())
        .build();

    let meter = provider.meter("bluey");
    Ok(Metrics {
        ttft_ms: meter.f64_histogram("bluey_ttft_ms").build(),
        total_ms: meter.f64_histogram("bluey_generation_total_ms").build(),
        input_tokens: meter.u64_counter("bluey_input_tokens_total").build(),
        output_tokens: meter.u64_counter("bluey_output_tokens_total").build(),
        cost_usd: meter.f64_counter("bluey_cost_usd_total").build(),
        stt_latency_ms: meter.f64_histogram("bluey_stt_latency_ms").build(),
        provider_errors: meter.u64_counter("bluey_provider_errors_total").build(),
    })
}
```

**Acceptance**: Metrics appear in Grafana Cloud within 60s of generation; per-lane labels correct.

**Verification**: Trigger Snap + Solve queries; verify distinct metric series in Grafana; verify cost_usd increments.

**Risks**: OTLP export adds ~5ms per batch. Mitigation: periodic reader batches every 30s, not per-event.

---

### B8.4 — AI Pricing Table + Cost Tracking

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Observability |
| **Status** | 🟡 Partial (table designed) |
| **Depends** | B3.1 (LLM trait) |
| **Design source** | CUE-DESIGN-04:L880-930 (pricing.rs, per-model USD) |

**Summary**: Per-model pricing lookup. Compute cost per generation. Accumulate per-session for dashboard display.

**Code sketch**:
```rust
// src-tauri/src/pricing.rs
pub struct ModelPrice { pub input_per_1m: f64, pub output_per_1m: f64 }

pub fn get_price(model: &str) -> ModelPrice {
    match model {
        m if m.contains("deepseek") => ModelPrice { input_per_1m: 0.14, output_per_1m: 0.28 }, // Cerebras
        m if m.contains("claude-sonnet-4") => ModelPrice { input_per_1m: 3.00, output_per_1m: 15.00 },
        m if m.contains("o3") => ModelPrice { input_per_1m: 10.00, output_per_1m: 40.00 },
        m if m.contains("gpt-4o") => ModelPrice { input_per_1m: 2.50, output_per_1m: 10.00 },
        _ => ModelPrice { input_per_1m: 0.0, output_per_1m: 0.0 },
    }
}

pub fn compute_cost(model: &str, input_tokens: u64, output_tokens: u64) -> f64 {
    let p = get_price(model);
    (input_tokens as f64 / 1_000_000.0) * p.input_per_1m
        + (output_tokens as f64 / 1_000_000.0) * p.output_per_1m
}

// Per-session accumulator
pub struct SessionCost {
    pub total_usd: f64,
    pub by_lane: [f64; 3], // [snap, solve, think]
}

impl SessionCost {
    pub fn record(&mut self, lane: Lane, model: &str, input: u64, output: u64) {
        let cost = compute_cost(model, input, output);
        self.total_usd += cost;
        self.by_lane[lane as usize] += cost;
    }
}
```

**Acceptance**: Cost computed correctly for known models; session accumulator tracks per-lane spend.

**Verification**: 15 Solve queries × 16K/4K tokens = expected $1.44; verify accumulator matches.

**Risks**: Pricing changes frequently. Mitigation: pricing table is a simple match — update on each release.

---

### B9.1 — Auto-Updater

| Field | Value |
|-------|-------|
| **Layer** | App / Release |
| **Status** | ❌ Not started |
| **Depends** | D0.1 (Tauri scaffold) |
| **Design source** | CUE-DESIGN-04:L940-990 (tauri-plugin-updater 2.9.0) |

**Summary**: Check for updates on launch + every 4 hours. Download and install with user confirmation.

**Code sketch**:
```typescript
// src/lib/updater.ts
import { check } from "@tauri-apps/plugin-updater";
import { relaunch } from "@tauri-apps/plugin-process";

export async function checkForUpdates(silent = false): Promise<boolean> {
  try {
    const update = await check();
    if (!update?.available) return false;

    if (silent) {
      // Background download, notify when ready
      await update.downloadAndInstall();
      return true; // Caller shows "restart to update" banner
    }
    // Interactive: download + relaunch immediately
    await update.downloadAndInstall();
    await relaunch();
    return true;
  } catch {
    return false;
  }
}

// Check every 4 hours
setInterval(() => checkForUpdates(true), 4 * 60 * 60 * 1000);
```

**Acceptance**: App detects new version from update endpoint; downloads; installs on restart.

**Verification**: Deploy test version with higher semver; verify app detects and offers update.

**Risks**: Signing key compromise. Mitigation: key stored in CI secrets only; pubkey pinned in tauri.conf.json.

---

### Phase 7 Deliverable

Production-ready ops infrastructure: secure credential storage, structured logging with rotation, real-time telemetry to Grafana, automatic updates, and cost visibility per session/lane.



---

## Phase 8: Dev Discipline (~3 days)

### Entry Criteria
- Phase 0 complete (repo structure established)
- Can run in parallel with any phase

### Exit Criteria
- All dev docs committed and referenced in README
- .codex/agents and .agents/skills directories populated
- PR template enforced via .github/PULL_REQUEST_TEMPLATE.md
- AUDIT.md checklist passes for current codebase

### Batch Structure

**PR 8A — Single PR (days 1-3)**:
- B10.1 CLAUDE.md
- B10.2 CHANGELOG.md
- B10.3 PR template
- B10.4 FIXES.md
- B10.5 AUDIT.md
- B10.6 .codex/agents (7 configs)
- B10.7 .agents/skills (10 skill cards)

---

### B10.1 — CLAUDE.md

| Field | Value |
|-------|-------|
| **Layer** | Dev / Documentation |
| **Status** | ❌ Not started |
| **Depends** | None |
| **Design source** | CUE-DESIGN-04:L1240-1290 (CLAUDE.md rules) |

**Summary**: Root development rules file for AI coding assistants. Defines architecture, code style, key files, and forbidden patterns.

**Code sketch**:
```markdown
# CLAUDE.md — bluey development rules

## Architecture
- Tauri 2 + React 19 + TypeScript + Rust
- Frontend: src/ (React app)
- Backend: src-tauri/src/ (Rust daemon)
- IPC: #[tauri::command] + invoke() (cold path), Unix socket (hot path)
- Three lanes: Snap (Cerebras), Solve (Claude Sonnet 4.5), Think (o3)

## Code style
- Rust: clippy::pedantic, no unwrap() in production, anyhow for errors
- TypeScript: strict mode, no `any`, ESM only
- Events: snake_case ("speech_detected", "llm_token")
- Commands: camelCase (Tauri convention)

## Key files
- src-tauri/src/lib.rs — plugin registration
- src-tauri/src/llm/router.rs — three-lane dispatch
- src-tauri/src/rag/ — vector store + retrieval
- src/routes/index.tsx — page structure
- src/hooks/ — React hook composition

## Never
- Never hardcode API keys
- Never log raw keys (use mask_key())
- Never block tokio runtime with sync I/O
- Never use setTimeout for timing (use rAF)
- Never add state outside Tauri managed state
```

**Acceptance**: File exists at repo root; AI assistants follow rules when given context.

---

### B10.6 — .codex/agents (7 Configs)

| Field | Value |
|-------|-------|
| **Layer** | Dev / Tooling |
| **Status** | ❌ Not started |
| **Depends** | None |
| **Design source** | CUE-DESIGN-04:L1380-1410 (.codex/ directory structure) |

**Summary**: Specialized agent configurations for different development tasks.

**Code sketch**:
```markdown
<!-- .codex/agents/backend-architect.md -->
# Backend Architect Agent

You are a Rust systems architect for a Tauri 2 desktop application.

## Expertise
- Async Rust (tokio), trait-based abstractions, zero-copy patterns
- SQLite (rusqlite, WAL mode, migrations)
- HTTP/2 streaming (reqwest, hyper)
- Unix domain sockets, IPC design

## Constraints
- All state in Tauri managed state (no globals)
- Errors via anyhow::Result, never panic in production
- Streaming via async Stream trait
- Rate limiting via governor crate

## When reviewing code
- Check for blocking calls in async context
- Verify CancellationToken propagation
- Ensure proper Drop implementations for resources
- Validate token budget compliance per lane
```

```markdown
<!-- .codex/agents/test-engineer.md -->
# Test Engineer Agent

## Testing strategy
- Unit tests: #[cfg(test)] mod tests in each file
- Integration tests: tests/ directory with mock providers
- Property tests: proptest for chunker, token counter
- Benchmarks: criterion for hot-path latency

## Mock patterns
- MockLlmProvider: returns canned responses, tracks calls
- MockEmbedding: returns identity vectors for deterministic search
- MockKeychain: in-memory HashMap<String, String>

## Coverage targets
- Core logic (router, chunker, context assembly): >90%
- IPC commands: >80%
- UI components: snapshot tests for critical paths
```

**Acceptance**: 7 agent files exist; each has clear expertise, constraints, and behavioral rules.

**Verification**: Use each agent config in a coding session; verify it produces domain-appropriate output.

**Risks**: None (documentation only).

---

### Phase 8 Deliverable

Complete developer documentation suite: CLAUDE.md for AI assistants, CHANGELOG for release tracking, PR template for review quality, FIXES.md for debugging patterns, AUDIT.md for security posture, and 7+10 agent/skill configs for AI-assisted development.



---

## Phase 9: Latency Engineering + Profiling (~3 weeks)

### Entry Criteria
- Phase 0 complete (daemon binary, Unix socket IPC)
- B2.5 (STT provider trait) merged — Deepgram WebSocket exists
- B3.1 (LLM trait) merged — HTTP clients exist
- R1 (intent classifier) at least stubbed

### Exit Criteria
- p50 Snap (mic → first visible token): ≤500ms
- p50 Solve (mic → first token): ≤4s
- Deepgram WebSocket stays open for entire session (zero reconnects under normal conditions)
- HTTP/2 connection pool: 3 prewarmed, reused across requests
- Latency instrumentation: every hop timestamped, viewable in dashboard
- Stable-partial detector: LLM dispatch only on stabilized transcript

### Batch Structure

**PR 9A — Connection Persistence (days 1-7)**:
- L1 Persistent Deepgram WebSocket
- L2 HTTP/2 keep-alive pool
- L5 Unix domain socket hot path (shared with Phase 0)

**PR 9B — Speculative Dispatch (days 8-14)**:
- L3 Stable-partial detector
- L4 Speculative LLM with cancel-on-change

**PR 9C — Instrumentation + Tuning (days 15-21)**:
- L6 End-to-end latency instrumentation
- Profiling cycles: measure → identify bottleneck → fix → measure

---

### L1 — Persistent Deepgram WebSocket Lifecycle

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Audio |
| **Status** | ❌ Not started |
| **Depends** | B2.5 (SttProvider trait) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L580 (L1 spec: never close mid-session) |

**Summary**: Maintain a single Deepgram Nova-3 WebSocket for the entire session. Reconnect on drop with exponential backoff. Eliminates 200-400ms connection setup per utterance.

**Latency impact**: Saves ~300ms per utterance (WebSocket handshake + TLS + Deepgram auth). Over a 3-hour session with 200 utterances, this saves 60s of cumulative latency.

**Code sketch**:
```rust
// src-tauri/src/stt/deepgram_ws.rs
use tokio_tungstenite::{connect_async, tungstenite::Message};
use futures::{SinkExt, StreamExt};
use tokio::sync::mpsc;

pub struct PersistentDeepgramWs {
    audio_tx: mpsc::Sender<Vec<u8>>,
    transcript_rx: mpsc::Receiver<TranscriptEvent>,
}

#[derive(Debug)]
pub struct TranscriptEvent {
    pub text: String,
    pub is_final: bool,
    pub confidence: f32,
    pub timestamp_ms: u64,
}

impl PersistentDeepgramWs {
    pub async fn connect(api_key: &str, sample_rate: u32) -> Result<Self> {
        let url = format!(
            "wss://api.deepgram.com/v1/listen?model=nova-3&language=en&smart_format=true\
             &interim_results=true&endpointing=300&sample_rate={sample_rate}&encoding=linear16"
        );

        let (audio_tx, mut audio_rx) = mpsc::channel::<Vec<u8>>(512);
        let (tx_out, transcript_rx) = mpsc::channel::<TranscriptEvent>(256);
        let api_key = api_key.to_string();

        tokio::spawn(async move {
            let mut backoff = Duration::from_millis(100);
            loop {
                match Self::run_connection(&url, &api_key, &mut audio_rx, &tx_out).await {
                    Ok(()) => break, // Clean shutdown
                    Err(e) => {
                        tracing::warn!(err = %e, backoff_ms = backoff.as_millis(), "Deepgram WS dropped, reconnecting");
                        tokio::time::sleep(backoff).await;
                        backoff = (backoff * 2).min(Duration::from_secs(10));
                    }
                }
            }
        });

        Ok(Self { audio_tx, transcript_rx })
    }

    async fn run_connection(
        url: &str,
        api_key: &str,
        audio_rx: &mut mpsc::Receiver<Vec<u8>>,
        tx_out: &mpsc::Sender<TranscriptEvent>,
    ) -> Result<()> {
        let request = http::Request::builder()
            .uri(url)
            .header("Authorization", format!("Token {api_key}"))
            .body(())?;
        let (mut ws, _) = connect_async(request).await?;

        loop {
            tokio::select! {
                Some(audio) = audio_rx.recv() => {
                    ws.send(Message::Binary(audio)).await?;
                }
                Some(msg) = ws.next() => {
                    match msg? {
                        Message::Text(json) => {
                            if let Ok(event) = parse_deepgram_response(&json) {
                                tx_out.send(event).await.ok();
                            }
                        }
                        Message::Close(_) => return Ok(()),
                        _ => {}
                    }
                }
                else => break,
            }
        }
        Ok(())
    }

    pub async fn send_audio(&self, pcm: Vec<u8>) -> Result<()> {
        self.audio_tx.send(pcm).await.map_err(|_| anyhow::anyhow!("ws closed"))
    }

    pub async fn recv_transcript(&mut self) -> Option<TranscriptEvent> {
        self.transcript_rx.recv().await
    }

    pub async fn close(&self) {
        // Send close frame via a separate channel (omitted for brevity)
    }
}

fn parse_deepgram_response(json: &str) -> Result<TranscriptEvent> {
    let v: serde_json::Value = serde_json::from_str(json)?;
    let alt = &v["channel"]["alternatives"][0];
    Ok(TranscriptEvent {
        text: alt["transcript"].as_str().unwrap_or("").to_string(),
        is_final: v["is_final"].as_bool().unwrap_or(false),
        confidence: alt["confidence"].as_f64().unwrap_or(0.0) as f32,
        timestamp_ms: (v["start"].as_f64().unwrap_or(0.0) * 1000.0) as u64,
    })
}
```

**Acceptance**: WebSocket stays open for 30+ minutes without reconnect; audio sent continuously; transcripts arrive within 200ms of speech.

**Verification**: Run 30-minute session; count reconnects (should be 0); measure p50 partial latency.

**Risks**: Deepgram may close idle connections after 10s silence. Mitigation: send keepalive frames every 5s during silence (empty audio or ping).

---

### L2 — HTTP/2 Keep-Alive Pool (3 Prewarmed)

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Network |
| **Status** | ❌ Not started |
| **Depends** | B3.1 (LLM trait) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L582 (L2 spec: 3 prewarmed connections) |

**Summary**: Maintain persistent HTTP/2 connections to Cerebras, Anthropic, and OpenAI (for o3). Eliminates TLS handshake + TCP setup per request (~150-300ms savings).

**Latency impact**: First request to cold endpoint: ~300ms (DNS + TCP + TLS + HTTP/2 SETTINGS). With prewarmed pool: ~5ms (reuse existing multiplexed connection). Saves 295ms on every LLM call.

**Code sketch**:
```rust
// src-tauri/src/llm/connection_pool.rs
use reqwest::Client;
use std::time::Duration;

pub struct LlmConnectionPool {
    pub cerebras: Client,   // Snap lane
    pub anthropic: Client,  // Solve lane
    pub openai: Client,     // Think lane (o3)
}

impl LlmConnectionPool {
    pub fn new() -> Self {
        let base = Client::builder()
            .http2_prior_knowledge()       // Force HTTP/2 (skip upgrade)
            .pool_max_idle_per_host(3)     // Keep 3 idle connections
            .pool_idle_timeout(Duration::from_secs(300)) // 5 min idle before close
            .tcp_keepalive(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(120));

        Self {
            cerebras: base.clone().build().unwrap(),
            anthropic: base.clone().build().unwrap(),
            openai: base.build().unwrap(),
        }
    }

    /// Prewarm all connections on session start
    /// Sends a minimal request to establish HTTP/2 connection
    pub async fn prewarm(&self, keys: &CredentialStore) {
        let futs = vec![
            self.prewarm_one(&self.cerebras, "https://api.cerebras.ai/v1/models", keys.get("cerebras")),
            self.prewarm_one(&self.anthropic, "https://api.anthropic.com/v1/messages", keys.get("anthropic")),
            self.prewarm_one(&self.openai, "https://api.openai.com/v1/models", keys.get("openai")),
        ];
        futures::future::join_all(futs).await;
        tracing::info!("Connection pool prewarmed (3 endpoints)");
    }

    async fn prewarm_one(&self, client: &Client, url: &str, key: Option<&str>) {
        if let Some(k) = key {
            // HEAD or lightweight GET to establish connection
            let _ = client.get(url).bearer_auth(k).send().await;
        }
    }
}
```

**Acceptance**: Second request to same endpoint shows <10ms connection time (vs ~300ms for first cold request).

**Verification**: Time first vs second request to each endpoint; verify HTTP/2 multiplexing via connection reuse header.

**Risks**: Endpoints may close idle connections after 60s. Mitigation: pool_idle_timeout=300s covers most inter-query gaps; reqwest auto-reconnects transparently.

---

### L3 — Stable-Partial Detector + Trigger-on-Stable

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Intelligence |
| **Status** | ❌ Not started |
| **Depends** | L1 (Deepgram WS), R1 (intent classifier) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L584 (L3 spec) |

**Summary**: Don't dispatch LLM on every partial transcript. Wait until partial stabilizes (same text for 300ms) OR is_final arrives. This prevents wasted LLM calls on rapidly-changing partials.

**Latency impact**: Without this, we'd fire 5-10 LLM calls per utterance (one per partial). With stable-detection, we fire 1-2 (stable partial + final). Saves $0.01-0.05 per utterance AND reduces p95 latency by avoiding queue contention.

**Code sketch**:
```rust
// src-tauri/src/stt/stable_detector.rs
use tokio::time::{sleep, Duration, Instant};
use tokio::sync::mpsc;

pub struct StablePartialDetector {
    stability_window: Duration, // 300ms
}

#[derive(Debug, Clone)]
pub enum StableEvent {
    StablePartial(String),  // Partial hasn't changed for stability_window
    Final(String),          // Deepgram confirmed final
}

impl StablePartialDetector {
    pub fn new(stability_ms: u64) -> Self {
        Self { stability_window: Duration::from_millis(stability_ms) }
    }

    pub fn spawn(
        self,
        mut transcript_rx: mpsc::Receiver<TranscriptEvent>,
    ) -> mpsc::Receiver<StableEvent> {
        let (tx, rx) = mpsc::channel(64);

        tokio::spawn(async move {
            let mut last_partial = String::new();
            let mut last_change = Instant::now();
            let mut stable_fired = false;

            loop {
                tokio::select! {
                    Some(event) = transcript_rx.recv() => {
                        if event.is_final {
                            tx.send(StableEvent::Final(event.text)).await.ok();
                            last_partial.clear();
                            stable_fired = false;
                        } else if event.text != last_partial {
                            last_partial = event.text;
                            last_change = Instant::now();
                            stable_fired = false;
                        }
                    }
                    _ = sleep(Duration::from_millis(50)) => {
                        if !last_partial.is_empty()
                            && !stable_fired
                            && last_change.elapsed() >= self.stability_window
                        {
                            tx.send(StableEvent::StablePartial(last_partial.clone())).await.ok();
                            stable_fired = true;
                        }
                    }
                    else => break,
                }
            }
        });

        rx
    }
}
```

**Acceptance**: Rapid partial changes (every 100ms) don't trigger dispatch; stable partial (unchanged 300ms) triggers exactly once; final always triggers.

**Verification**: Feed synthetic partials at varying rates; count StableEvent emissions; verify exactly 1 per stable period.

**Risks**: 300ms stability window adds latency to fast speakers. Mitigation: configurable; reduce to 200ms for power users; final always fires immediately regardless.

---

### L4 — Speculative LLM with Cancel-on-Partial-Change

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Intelligence |
| **Status** | ❌ Not started |
| **Depends** | L3 (stable detector), R1 (router) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L586 (L4 spec) |

**Summary**: On StablePartial, speculatively dispatch to Snap lane. If a new partial arrives that changes meaning, cancel the in-flight request. If Final confirms the stable partial, let it complete. This shaves 300ms off perceived latency (we start LLM before final confirmation).

**Latency impact**: Snap lane takes ~200ms. By starting on stable-partial (which arrives ~300ms before final), we overlap LLM processing with STT finalization. Net effect: response appears ~300ms earlier.

**Code sketch**:
```rust
// src-tauri/src/llm/speculative.rs
use tokio_util::sync::CancellationToken;

pub struct SpeculativeDispatcher {
    pool: Arc<LlmConnectionPool>,
    router: Arc<IntentRouter>,
}

impl SpeculativeDispatcher {
    pub async fn handle_stable_event(
        &self,
        event: StableEvent,
        active_speculation: &mut Option<(String, CancellationToken)>,
    ) -> Option<LlmResponse> {
        match event {
            StableEvent::StablePartial(text) => {
                // Cancel any previous speculation
                if let Some((_, cancel)) = active_speculation.take() {
                    cancel.cancel();
                }
                // Start speculative generation
                let cancel = CancellationToken::new();
                let cancel_clone = cancel.clone();
                let text_clone = text.clone();
                let pool = self.pool.clone();
                let router = self.router.clone();

                let handle = tokio::spawn(async move {
                    let lane = router.classify(&text_clone).await;
                    if lane == Lane::Snap {
                        // Only speculate on Snap (fast enough to be worth it)
                        pool.cerebras.generate(&text_clone, cancel_clone).await.ok()
                    } else {
                        None
                    }
                });

                *active_speculation = Some((text, cancel));
                None // Result comes later
            }
            StableEvent::Final(text) => {
                if let Some((speculated_text, cancel)) = active_speculation.take() {
                    if text == speculated_text || text.starts_with(&speculated_text) {
                        // Final confirms speculation — let it complete
                        // (already running, result will arrive shortly)
                        return None; // Caller awaits the spawned task
                    } else {
                        // Final differs — cancel speculation, dispatch fresh
                        cancel.cancel();
                        let lane = self.router.classify(&text).await;
                        return self.dispatch_fresh(&text, lane).await;
                    }
                }
                // No active speculation — dispatch normally
                let lane = self.router.classify(&text).await;
                self.dispatch_fresh(&text, lane).await
            }
        }
    }

    async fn dispatch_fresh(&self, text: &str, lane: Lane) -> Option<LlmResponse> {
        // Normal dispatch through three-lane router
        todo!()
    }
}
```

**Acceptance**: Speculation fires on stable partial; if final matches, response arrives ~300ms earlier than non-speculative path; if final differs, speculation cancelled within 10ms.

**Verification**: Measure time-to-first-token with and without speculation on 100 test utterances; verify p50 improvement ≥200ms.

**Risks**: Wasted Cerebras calls when speculation is wrong (~20% of cases). Cost: 20% × $0.002/call = negligible ($0.0004/wrong speculation). Acceptable tradeoff for 300ms latency win.

---

### L5 — Unix Domain Socket Hot Path

| Field | Value |
|-------|-------|
| **Layer** | Daemon / IPC |
| **Status** | ❌ Not started (shared with Phase 0) |
| **Depends** | D0.1 (Tauri scaffold) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L588 (L5 spec: <10ms streaming) |

**Summary**: Replace stdin/stdout IPC between daemon and native overlay with Unix domain socket. Enables streaming tokens to overlay with <10ms latency (vs 30-50ms for Tauri events through webview).

**Latency impact**: Tauri event path: Rust → serialize → webview IPC → JS → render = ~30ms. Unix socket path: Rust → write bytes → native overlay reads → render = ~2ms. Saves 28ms per token batch at 60Hz = smoother streaming.

**Code sketch**:
```rust
// src-tauri/src/ipc/unix_socket.rs
use tokio::net::{UnixListener, UnixStream};
use tokio::io::{AsyncWriteExt, AsyncReadExt};

const SOCKET_PATH: &str = "/tmp/bluey-overlay.sock";

pub struct OverlaySocket {
    stream: Option<UnixStream>,
}

impl OverlaySocket {
    pub async fn listen() -> Result<Self> {
        let _ = std::fs::remove_file(SOCKET_PATH);
        let listener = UnixListener::bind(SOCKET_PATH)?;
        tracing::info!("Overlay socket listening at {SOCKET_PATH}");

        let (stream, _) = listener.accept().await?;
        Ok(Self { stream: Some(stream) })
    }

    /// Send token batch to overlay (<1ms for typical payload)
    pub async fn send_tokens(&mut self, payload: &OverlayPayload) -> Result<()> {
        if let Some(ref mut stream) = self.stream {
            let bytes = serde_json::to_vec(payload)?;
            let len = (bytes.len() as u32).to_le_bytes();
            stream.write_all(&len).await?;
            stream.write_all(&bytes).await?;
        }
        Ok(())
    }
}

#[derive(serde::Serialize)]
pub struct OverlayPayload {
    pub kind: &'static str,  // "token", "complete", "mode", "clear"
    pub text: Option<String>,
    pub lane: Option<&'static str>,
    pub generation_id: u64,
}
```

**Acceptance**: Token streaming from daemon to overlay measured at <5ms p99; overlay renders within same frame.

**Verification**: Instrument with timestamps on both sides; measure 1000 token deliveries; verify p99 < 5ms.

**Risks**: Windows doesn't have Unix sockets. Mitigation: use named pipes on Windows (`\\.\pipe\bluey-overlay`); abstract behind trait.

---

### L6 — End-to-End Latency Instrumentation

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Observability |
| **Status** | ❌ Not started |
| **Depends** | L1, L2, L5 |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L590 (L6 spec: timestamp every hop) |

**Summary**: Instrument every hop in the pipeline with monotonic timestamps. Export as spans to OTel + display in dashboard as flamegraph.

**Code sketch**:
```rust
// src-tauri/src/latency/instrument.rs
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct LatencyTrace {
    pub id: u64,
    pub hops: Vec<Hop>,
}

#[derive(Debug, Clone)]
pub struct Hop {
    pub name: &'static str,
    pub start: Instant,
    pub end: Option<Instant>,
}

impl LatencyTrace {
    pub fn new() -> Self {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        Self {
            id: COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            hops: Vec::with_capacity(8),
        }
    }

    pub fn start_hop(&mut self, name: &'static str) -> usize {
        let idx = self.hops.len();
        self.hops.push(Hop { name, start: Instant::now(), end: None });
        idx
    }

    pub fn end_hop(&mut self, idx: usize) {
        if let Some(hop) = self.hops.get_mut(idx) {
            hop.end = Some(Instant::now());
        }
    }

    pub fn total_ms(&self) -> f64 {
        if let (Some(first), Some(last)) = (self.hops.first(), self.hops.last()) {
            let end = last.end.unwrap_or_else(Instant::now);
            end.duration_since(first.start).as_secs_f64() * 1000.0
        } else { 0.0 }
    }

    /// Expected hops for Snap lane:
    /// mic_capture → vad_detect → stt_partial → stable_detect →
    /// intent_classify → llm_dispatch → llm_first_token → overlay_render
    pub fn report(&self) -> String {
        self.hops.iter().map(|h| {
            let dur = h.end.map(|e| e.duration_since(h.start).as_millis())
                .unwrap_or(0);
            format!("  {} → {}ms", h.name, dur)
        }).collect::<Vec<_>>().join("\n")
    }
}

// Target validation
pub fn validate_targets(trace: &LatencyTrace, lane: Lane) -> bool {
    let total = trace.total_ms();
    match lane {
        Lane::Snap => total <= 500.0,   // p50 target
        Lane::Solve => total <= 4000.0, // p50 first-token target
        Lane::Think => true,            // No hard target (progress bar)
    }
}
```

**Acceptance**: Every generation produces a LatencyTrace with all hops; dashboard displays flamegraph; p50 targets validated.

**Verification**: Run 100 Snap queries; verify all traces have 8 hops; verify p50 ≤ 500ms; alert on regression.

**Risks**: Instrumentation overhead. Mitigation: Instant::now() is ~20ns on modern hardware; 8 hops = 160ns total — negligible.

---

### Phase 9 Deliverable

Sub-500ms mic-to-first-token for Snap lane. Persistent connections eliminate setup latency. Speculative dispatch overlaps STT finalization with LLM processing. Full instrumentation enables continuous optimization. This is what makes bluey feel instant.



---

## Phase 10: Cost Optimization (~1 week)

### Entry Criteria
- Phase 4 complete (three-lane routing working)
- Phase 5 complete (RAG available for context filtering)
- R1 (intent classifier) production-ready
- CO1 requires Anthropic prompt caching API access

### Exit Criteria
- Solve lane input costs reduced 50-70% via prompt caching
- Average session cost drops from $3.04 to ≤$1.50
- Router correctly splits easy/hard within Solve lane
- Cost dashboard shows real-time per-session spend
- Per-session alerting fires at configurable threshold

### Batch Structure

**PR 10A — Single PR (days 1-7)**:
- CO1 Anthropic prompt caching
- CO2 RAG-filtered context
- CO3 Router split within Solve
- Cost dashboard widget

---

### CO1 — Anthropic Prompt Caching (Solve Lane)

| Field | Value |
|-------|-------|
| **Layer** | Daemon / LLM |
| **Status** | ❌ Not started |
| **Depends** | F2 (Solve-lane streaming), B3.1 (LLM trait) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L130 (50-70% Solve input savings) |

**Summary**: Use Anthropic's prompt caching to avoid re-processing the system prompt + skill template on every Solve request. The static prefix (system prompt + skill template + context preamble) is cached server-side; only the dynamic suffix (user query + recent turns) is billed at full rate.

**Cost justification**:
- Without caching: 15 Solve queries/session × 16K input tokens × $3.00/1M = $0.72/session input cost
- With caching (60% cache hit): 15 × (6.4K full-price + 9.6K cached at $0.30/1M) = $0.33/session
- **Savings: $0.39/session (54% reduction on input tokens)**
- At scale (1000 users × 2 sessions/day): **$780/day saved**

**Code sketch**:
```rust
// src-tauri/src/llm/providers/anthropic.rs
use serde_json::json;

pub struct AnthropicProvider {
    client: reqwest::Client,
    api_key: String,
}

impl AnthropicProvider {
    /// Build request with cache_control on static prefix blocks
    /// Anthropic caches content blocks marked with cache_control: {type: "ephemeral"}
    pub fn build_cached_request(
        &self,
        system_prompt: &str,
        skill_template: &str,
        dynamic_context: &str,
        user_query: &str,
    ) -> serde_json::Value {
        json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 4096,
            "stream": true,
            "system": [
                {
                    "type": "text",
                    "text": system_prompt,
                    "cache_control": {"type": "ephemeral"}  // Cached (stable across requests)
                },
                {
                    "type": "text",
                    "text": skill_template,
                    "cache_control": {"type": "ephemeral"}  // Cached (stable per skill)
                }
            ],
            "messages": [
                {
                    "role": "user",
                    "content": format!("{}\n\n{}", dynamic_context, user_query)
                    // NOT cached (changes every request)
                }
            ]
        })
    }

    pub async fn stream_with_caching(
        &self,
        system_prompt: &str,
        skill_template: &str,
        context: &str,
        query: &str,
        cancel: CancellationToken,
    ) -> Result<impl Stream<Item = Result<Token>>> {
        let body = self.build_cached_request(system_prompt, skill_template, context, query);
        let resp = self.client.post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "prompt-caching-2024-07-31")
            .json(&body)
            .send().await?;

        // Parse SSE stream (same as non-cached path)
        Ok(parse_anthropic_sse(resp.bytes_stream(), cancel))
    }
}
```

**Acceptance**: Response headers show `cache_creation_input_tokens` on first call, `cache_read_input_tokens` on subsequent calls within 5-minute TTL.

**Verification**: Make 5 Solve requests with same system prompt; verify cache hits in response usage metadata; compute actual cost reduction.

**Risks**: Cache TTL is 5 minutes — if user is idle >5min between queries, cache evicts. Mitigation: acceptable; most interview sessions have queries every 1-3 minutes. Cache miss just means one full-price request.

---

### CO2 — RAG-Filtered Context for Solve Lane

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Context |
| **Status** | ❌ Not started |
| **Depends** | B5.4 (Live RAG indexer), CM2 (context assembly) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L132 (RAG-filtered, not full history) |

**Summary**: Instead of sending full conversation history to Solve lane (16K tokens), use RAG to select only the 3-5 most relevant previous turns. Reduces average input from 16K to ~6K tokens.

**Cost justification**:
- Without RAG filtering: 16K avg input tokens per Solve query
- With RAG filtering: 6K avg input tokens (only relevant turns + current query)
- Savings per query: 10K tokens × $3.00/1M = $0.03
- Per session (15 queries): **$0.45 saved**
- Combined with CO1 caching on the 6K: further 60% reduction on cached portion

**Code sketch**:
```rust
// src-tauri/src/session/context_filter.rs

pub async fn build_solve_context(
    query: &str,
    session_turns: &[Turn],
    retriever: &HybridRetriever,
    budget_tokens: u32, // 16000
) -> Vec<Turn> {
    // 1. Always include last 3 turns (recency)
    let recent: Vec<&Turn> = session_turns.iter().rev().take(3).collect();
    let recent_tokens: u32 = recent.iter().map(|t| t.token_count).sum();

    // 2. Use RAG to find relevant older turns
    let remaining_budget = budget_tokens.saturating_sub(recent_tokens + 2000); // Reserve 2K for query+system
    let rag_results = retriever.retrieve(query, None, 10).await.unwrap_or_default();

    // 3. Map RAG chunk_ids back to turns, deduplicate with recent
    let recent_ids: HashSet<&str> = recent.iter().map(|t| t.id.as_str()).collect();
    let mut rag_turns: Vec<&Turn> = Vec::new();
    let mut rag_tokens = 0u32;

    for result in &rag_results {
        if let Some(turn) = session_turns.iter().find(|t| t.id == result.0) {
            if !recent_ids.contains(turn.id.as_str()) && rag_tokens + turn.token_count <= remaining_budget {
                rag_tokens += turn.token_count;
                rag_turns.push(turn);
            }
        }
    }

    // 4. Combine: RAG turns (chronological) + recent turns
    rag_turns.sort_by_key(|t| t.created_at);
    let mut context: Vec<Turn> = rag_turns.into_iter().cloned().collect();
    context.extend(recent.into_iter().rev().cloned());
    context
}
```

**Acceptance**: Solve context averages 6K tokens (vs 16K without filtering); answer quality maintained (human eval on 20 test cases).

**Verification**: Compare answer quality with full context vs RAG-filtered on held-out test set; verify no degradation >5%.

**Risks**: RAG may miss critical context for follow-up questions. Mitigation: always include last 3 turns (covers immediate follow-ups); RAG catches older relevant context.

---

### CO3 — Router Split: Easy → Cerebras, Hard → Claude

| Field | Value |
|-------|-------|
| **Layer** | Daemon / Intelligence |
| **Status** | ❌ Not started |
| **Depends** | R1 (intent classifier), CO1 (Anthropic integration) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L134 (within Solve lane, cost-route) |

**Summary**: Within the Solve lane, further classify queries as "easy" (can be handled by Cerebras at 1/20th the cost) vs "hard" (requires Claude's reasoning). Easy: factual recall, simple explanations, short answers. Hard: multi-step reasoning, code generation, system design.

**Cost justification**:
- Assume 40% of Solve queries are "easy" (factual, short-answer)
- Easy query cost: Cerebras $0.14/1M input vs Claude $3.00/1M = 21x cheaper
- Per session: 6 easy queries × 6K tokens × $0.14/1M = $0.005 (vs $0.108 on Claude)
- **Savings: $0.10/session from easy-query routing**
- Combined with CO1+CO2: total session cost drops from $3.04 to ~$1.20

**Code sketch**:
```rust
// src-tauri/src/llm/cost_router.rs

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SolveDifficulty { Easy, Hard }

pub fn classify_solve_difficulty(query: &str, context_tokens: u32) -> SolveDifficulty {
    // Heuristic rules (fast, no model call needed)
    let indicators_hard = [
        query.contains("design") && query.contains("system"),
        query.contains("implement") || query.contains("code"),
        query.contains("compare") && query.contains("tradeoff"),
        query.len() > 200, // Long queries tend to be complex
        context_tokens > 8000, // Heavy context suggests complex problem
    ];

    let hard_score: usize = indicators_hard.iter().filter(|&&x| x).count();

    let indicators_easy = [
        query.starts_with("what is") || query.starts_with("define"),
        query.contains("example of"),
        query.len() < 50,
        query.split_whitespace().count() < 10,
    ];

    let easy_score: usize = indicators_easy.iter().filter(|&&x| x).count();

    if hard_score >= 2 || (hard_score >= 1 && easy_score == 0) {
        SolveDifficulty::Hard
    } else {
        SolveDifficulty::Easy
    }
}

pub fn select_solve_model(difficulty: SolveDifficulty) -> (&'static str, &'static str) {
    match difficulty {
        SolveDifficulty::Easy => ("cerebras", "deepseek-v3"),      // $0.14/1M
        SolveDifficulty::Hard => ("anthropic", "claude-sonnet-4"), // $3.00/1M
    }
}
```

**Acceptance**: Easy queries route to Cerebras with acceptable quality; hard queries still go to Claude; misclassification rate <15%.

**Verification**: Label 100 test queries as easy/hard; run classifier; verify accuracy >85%; spot-check Cerebras answers on "easy" queries for quality.

**Risks**: Cerebras may produce lower-quality answers for borderline queries. Mitigation: conservative classification (when in doubt, route to Claude); user can always force Think lane for maximum quality.

---

### Cost Dashboard Widget

| Field | Value |
|-------|-------|
| **Layer** | Dashboard / UI |
| **Status** | ❌ Not started |
| **Depends** | B8.4 (pricing table) |
| **Design source** | CUE-BLUEY-V3-PLAN-SKELETON:L160 (cost dashboard) |

**Summary**: Real-time cost display in dashboard showing per-session and cumulative spend, broken down by lane.

**Code sketch**:
```typescript
// src/components/CostWidget.tsx
import { useEffect, useState } from "react";
import { listen } from "@tauri-apps/api/event";

interface CostData {
  session_total: number;
  by_lane: { snap: number; solve: number; think: number };
  queries_count: number;
}

export function CostWidget() {
  const [cost, setCost] = useState<CostData>({ session_total: 0, by_lane: { snap: 0, solve: 0, think: 0 }, queries_count: 0 });

  useEffect(() => {
    const unlisten = listen<CostData>("cost_update", (e) => setCost(e.payload));
    return () => { unlisten.then(f => f()); };
  }, []);

  return (
    <div className="cost-widget p-3 rounded-lg bg-muted">
      <div className="text-2xl font-mono">${cost.session_total.toFixed(3)}</div>
      <div className="text-xs text-muted-foreground mt-1">
        Snap: ${cost.by_lane.snap.toFixed(4)} · Solve: ${cost.by_lane.solve.toFixed(4)} · Think: ${cost.by_lane.think.toFixed(4)}
      </div>
      <div className="text-xs mt-1">{cost.queries_count} queries this session</div>
    </div>
  );
}
```

**Acceptance**: Cost updates in real-time after each LLM call; breakdown by lane is accurate; matches manual calculation.

**Verification**: Run 5 queries across all lanes; verify widget total matches sum of individual costs from pricing table.

---

### Phase 10 Deliverable

60% cost reduction on Solve lane through three complementary optimizations: prompt caching (50-70% input savings), RAG-filtered context (62% fewer input tokens), and smart routing within Solve (21x cheaper for easy queries). Session cost drops from $3.04 to ~$1.20. Real-time cost visibility prevents surprise bills.

### Combined Cost Impact Summary

| Optimization | Mechanism | Per-Session Savings | Annual Savings (1K users, 2 sessions/day) |
|---|---|---|---|
| CO1 Prompt Caching | Cache static prefix server-side | $0.39 | $284K |
| CO2 RAG-Filtered Context | Send 6K instead of 16K tokens | $0.45 | $328K |
| CO3 Easy→Cerebras Routing | 21x cheaper for 40% of queries | $0.10 | $73K |
| **Combined** | | **$0.94** | **$685K** |

**Before optimizations**: $3.04/session → **After**: $1.20/session (60% reduction)

---

## Appendix: Cross-Phase Dependency Summary

```
Phase 5 (RAG) ←── Phase 4 (B3.1 LLM trait, R1 router)
Phase 6 (Dashboard) ←── Phase 1+2 (shell, sessions)
Phase 7 (Ops) ←── Phase 0 (Tauri scaffold)
Phase 8 (Dev) ←── None (parallel anytime)
Phase 9 (Latency) ←── Phase 0 (daemon), B2.5 (STT), B3.1 (LLM)
Phase 10 (Cost) ←── Phase 4 (routing) + Phase 5 (RAG)
```

## Appendix: Target Validation Matrix

| Metric | Target | Measured By | Phase |
|--------|--------|-------------|-------|
| Snap mic→first-token | ≤500ms p50 | L6 instrumentation | 9 |
| Solve mic→first-token | ≤4s p50 | L6 instrumentation | 9 |
| Overlay render latency | <10ms | Unix socket timestamps | 9 |
| RAG search latency | <100ms | spawn_blocking timer | 5 |
| Session cost (with opts) | ≤$1.50 | B8.4 pricing accumulator | 10 |
| Epoch compaction | <5s | Background job timer | 5 |
| Deepgram reconnects/session | 0 (normal) | L1 reconnect counter | 9 |
| Intent classifier latency | <5ms | R1 timer | 4 (validated in 9) |

---

*End of Part B. Phases 5-10 fully specified with compilable code sketches, design-doc citations, cost justifications, and acceptance criteria.*


---

# Part C — APPENDICES (Architecture, competitive, cost, metrics, risks, providers, glossary)

# bluey V3 Plan — Appendices

## S1: Architecture Deep-Dive

### 1.1 Process Topology

bluey V3 runs as four cooperating processes on the user's machine, plus cloud API endpoints:

```
┌─────────────────────────────────────────────────────────────────────────┐
│  PROCESS MAP (user's machine)                                           │
│                                                                         │
│  PID 1: cue-daemon (Rust binary, ~8MB)                                 │
│    • Launched by: CLI `bluey on` or launchd/systemd autostart           │
│    • Lifecycle: long-running background process                         │
│    • Owns: audio capture, STT, LLM routing, RAG, session state, IPC    │
│    • Listens: TCP :57321 (CLI), Unix socket (overlay), Tauri bridge     │
│    • Children: spawns overlay process                                   │
│                                                                         │
│  PID 2: Native Overlay (Swift macOS / C Windows, ~2MB)                  │
│    • Launched by: daemon on startup                                     │
│    • Lifecycle: child of daemon, respawned on crash                     │
│    • Owns: rendering, always-on-top, content protection, user input     │
│    • IPC: Unix domain socket to daemon (hot path)                       │
│                                                                         │
│  PID 3: Tauri Dashboard (Rust+WebView, ~15MB)                           │
│    • Launched by: user hotkey Cmd+Shift+D or CLI `bluey dashboard`      │
│    • Lifecycle: created once, hidden on close (instant re-show)         │
│    • Owns: settings UI, session browser, prompt editor, dev tools       │
│    • IPC: Tauri invoke/events to daemon (cold path)                     │
│                                                                         │
│  PID 4: cue-cli (Rust binary, ~4MB)                                    │
│    • Launched by: user in terminal                                      │
│    • Lifecycle: ephemeral per command                                   │
│    • IPC: TCP localhost:57321 to daemon                                 │
│                                                                         │
│  PID 5: cue-audio (Swift/C helper, ~1MB) [optional]                    │
│    • Launched by: daemon when native audio helpers needed               │
│    • Lifecycle: runs during audio capture session                       │
│    • Output: raw f32 PCM to stdout, consumed by daemon                  │
└─────────────────────────────────────────────────────────────────────────┘
```

**Launch sequence:**
1. User runs `bluey on` (CLI) or system autostart triggers daemon
2. Daemon initializes: SQLite, config, HTTP/2 connection pools (prewarmed)
3. Daemon spawns native overlay as child process
4. Daemon opens Unix domain socket at `~/.local/share/bluey/overlay.sock`
5. Overlay connects to socket, sends `Ready` event
6. Daemon begins audio capture (if configured for auto-start)
7. Dashboard is pre-created but hidden (Tauri window with `visible: false`)

**Crash recovery:**
- Overlay crash → daemon detects broken socket, respawns within 500ms
- Daemon crash → overlay exits (parent died), CLI detects on next command
- Dashboard crash → Tauri webview recreated on next Cmd+Shift+D

### 1.2 Data Flow Diagrams

#### Audio Signal Path (mic-to-first-token)

```
Microphone                                                          Overlay
    │                                                                  ▲
    ▼                                                                  │
┌────────┐   f32 PCM    ┌─────────┐  voiced frames  ┌──────────┐     │
│  CPAL  │─────────────▶│  Ring   │────────────────▶│  VAD     │     │
│ capture│  48kHz mono   │  Buffer │  (skip silence) │ RMS + ML │     │
└────────┘               │ 128KB   │                 └────┬─────┘     │
                         └─────────┘                      │           │
                                                          ▼           │
                                                    ┌──────────┐      │
                                                    │ Resample │      │
                                                    │  rubato  │      │
                                                    │ →16kHz   │      │
                                                    └────┬─────┘      │
                                                         │            │
                                                         ▼            │
                                                   ┌───────────┐      │
                                                   │ Deepgram  │      │
                                                   │ Nova-3 WS │      │
                                                   │(persistent)│      │
                                                   └─────┬─────┘      │
                                                         │            │
                                              partial/final transcript │
                                                         │            │
                                                         ▼            │
                                                   ┌───────────┐      │
                                                   │  Stable   │      │
                                                   │  Partial  │      │
                                                   │ Detector  │      │
                                                   └─────┬─────┘      │
                                                         │            │
                                              stable transcript       │
                                                         │            │
                                                         ▼            │
                                                   ┌───────────┐      │
                                                   │  Intent   │      │
                                                   │ Classifier│      │
                                                   │   <5ms    │      │
                                                   └─────┬─────┘      │
                                                         │            │
                                          ┌──────────────┼────────┐   │
                                          ▼              ▼        ▼   │
                                     ┌────────┐   ┌────────┐ ┌──────┐│
                                     │Cerebras│   │ Claude │ │  o3  ││
                                     │  Snap  │   │ Solve  │ │Think ││
                                     │80-300ms│   │0.5-4s  │ │15-60s││
                                     └───┬────┘   └───┬────┘ └──┬───┘│
                                         │            │          │    │
                                         ▼            ▼          ▼    │
                                     ┌────────────────────────────┐   │
                                     │   Unix Socket Hot Path     │───┘
                                     │   Streaming tokens @60Hz   │
                                     └────────────────────────────┘
```

**Latency budget (Snap lane, mic→first-visible-token):**
| Hop | Budget | Cumulative |
|-----|--------|------------|
| CPAL buffer → ring | 5ms | 5ms |
| VAD decision | 10ms | 15ms |
| Resample (rubato) | 2ms | 17ms |
| Network to Deepgram | 30ms | 47ms |
| Deepgram partial latency | 100ms | 147ms |
| Stable-partial detection | 50ms | 197ms |
| Intent classifier | 5ms | 202ms |
| Network to Cerebras | 20ms | 222ms |
| Cerebras TTFT | 80ms | 302ms |
| Unix socket → overlay render | 5ms | 307ms |
| **Total p50** | | **~310ms** |
| **Total p95** | | **~450ms** |

*Source: Deepgram Nova-3 streaming latency benchmarks, Cerebras published TTFT for DeepSeek V3 [CUE-DESIGN-02:signal path diagram]*

#### LLM Query Path

```
Transcript (stable)
    │
    ▼
┌──────────────────┐
│ Intent Classifier │
│ (rule + embedding)│
└────────┬─────────┘
         │
    ┌────┴────┬──────────┐
    ▼         ▼          ▼
┌───────┐ ┌───────┐ ┌───────┐
│ SNAP  │ │ SOLVE │ │ THINK │
└───┬───┘ └───┬───┘ └───┬───┘
    │         │          │
    ▼         ▼          ▼
┌───────┐ ┌───────┐ ┌───────┐
│Context│ │Context│ │Context│
│Assembly│ │Assembly│ │Assembly│
│Fresh  │ │RAG    │ │Full+  │
│3 turns│ │filtered│ │Epochs │
└───┬───┘ └───┬───┘ └───┬───┘
    │         │          │
    ▼         ▼          ▼
┌───────┐ ┌───────┐ ┌───────┐
│Prompt │ │Prompt │ │Prompt │
│Compose│ │Compose│ │Compose│
│TINY   │ │Full+  │ │Full+  │
│<500tok│ │Skill  │ │Think  │
└───┬───┘ └───┬───┘ └───┬───┘
    │         │          │
    ▼         ▼          ▼
┌───────┐ ┌───────┐ ┌───────┐
│HTTP/2 │ │HTTP/2 │ │HTTP/2 │
│Cerebras│ │Claude │ │OpenAI │
│Pool   │ │Pool   │ │Pool   │
└───┬───┘ └───┬───┘ └───┬───┘
    │         │          │
    ▼         ▼          ▼
┌──────────────────────────┐
│  Stream Emitter (60Hz)   │
│  Batches tokens, emits   │
│  via Unix socket or      │
│  Tauri event             │
└──────────────────────────┘
```

#### Session State Path

```
User action (new session / switch / archive)
    │
    ▼
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  CLI / Dash  │────▶│   Daemon     │────▶│   SQLite     │
│  (command)   │     │  (handler)   │     │   (WAL)      │
└──────────────┘     └──────┬───────┘     └──────────────┘
                            │
                   ┌────────┼────────┐
                   ▼        ▼        ▼
              ┌────────┐┌────────┐┌────────┐
              │Overlay ││Dashboard││ Metrics│
              │SessionChanged│SessionChanged││ counter│
              │(socket)│(event) ││         │
              └────────┘└────────┘└────────┘
```

### 1.3 IPC Contracts

#### Native Overlay ↔ Daemon: Unix Domain Socket Protocol

**Transport**: Unix domain socket at `~/.local/share/bluey/overlay.sock`
**Framing**: Length-prefixed JSON (4-byte little-endian length + UTF-8 JSON payload)
**Direction**: Bidirectional

**Daemon → Overlay messages:**

```json
// Stream tokens (hot path, ~60 per second during generation)
{ "type": "StreamToken", "generation_id": 42, "text": "Here is the answer..." }

// Generation complete
{ "type": "StreamComplete", "generation_id": 42 }

// Full card push (non-streaming)
{ "type": "PushCard", "card": { "id": "uuid", "kind": "answer", "title": "...", "body": "...", "lane": "snap", "model": "deepseek-v3" } }

// Session changed
{ "type": "SessionChanged", "session_id": "uuid", "name": "Technical Interview" }

// Mode indicator update
{ "type": "ModeUpdate", "lane": "solve", "model": "claude-sonnet-4-5", "state": "streaming" }

// Patch-mode diff
{ "type": "PatchDiff", "generation_id": 42, "ops": [
  { "op": "KEEP", "block_id": 0 },
  { "op": "MODIFY", "block_id": 1, "content": "updated text..." },
  { "op": "ADD", "after_block": 1, "content": "new section..." },
  { "op": "REMOVE", "block_id": 3 }
]}

// Visibility control
{ "type": "SetVisible", "visible": true }
{ "type": "SetOpacity", "opacity": 0.85 }

// Clear display
{ "type": "Clear" }
```

**Overlay → Daemon messages:**

```json
// Ready signal after connection
{ "type": "Ready", "platform": "macos", "version": "1.0.0" }

// User typed a question in composer
{ "type": "UserQuery", "text": "explain binary search", "mode": "auto" }

// Manual lane override
{ "type": "LaneOverride", "lane": "think" }

// User action
{ "type": "Action", "action": "recap" | "screenshot" | "new_session" | "toggle_listen" }

// Window state
{ "type": "WindowState", "visible": true, "collapsed": false, "opacity": 0.85 }
```

#### Daemon ↔ Tauri Dashboard: Commands + Events

**Tauri Commands (invoke, cold path 10-30ms):**

```rust
// Session management
#[tauri::command] async fn list_sessions() -> Vec<SessionSummary>;
#[tauri::command] async fn get_session(id: String) -> SessionDetail;
#[tauri::command] async fn create_session(name: String) -> SessionSummary;
#[tauri::command] async fn archive_session(id: String) -> Result<()>;
#[tauri::command] async fn switch_session(id: String) -> Result<()>;

// Settings
#[tauri::command] async fn get_settings() -> Settings;
#[tauri::command] async fn update_settings(patch: SettingsPatch) -> Result<()>;

// Provider management
#[tauri::command] async fn list_providers() -> Vec<ProviderStatus>;
#[tauri::command] async fn test_connection(provider: String) -> ConnectionResult;
#[tauri::command] async fn update_api_key(provider: String, key: String) -> Result<()>;

// Prompt/skill management
#[tauri::command] async fn list_skills() -> Vec<SkillTemplate>;
#[tauri::command] async fn update_skill(id: String, template: SkillTemplate) -> Result<()>;

// Audio control
#[tauri::command] async fn audio_status() -> AudioStatus;
#[tauri::command] async fn toggle_audio(source: AudioSource) -> Result<()>;

// Dev tools
#[tauri::command] async fn get_metrics() -> MetricsSnapshot;
#[tauri::command] async fn get_latency_trace(generation_id: u64) -> LatencyTrace;
```

**Tauri Events (emit, daemon → dashboard):**

```rust
// Real-time streaming (mirrored from overlay for dashboard live view)
app.emit("llm-token", StreamChunk { generation_id, text });
app.emit("llm-complete", generation_id);
app.emit("llm-error", ErrorInfo { generation_id, message });

// State changes
app.emit("session-changed", SessionSummary { id, name, state });
app.emit("audio-status", AudioStatus { system: bool, mic: bool, stt_state });
app.emit("stt-transcript", TranscriptEvent { text, is_final, speaker });
app.emit("lane-active", LaneEvent { lane, model, state });

// Cost tracking
app.emit("cost-update", CostEvent { session_total, last_query_cost });

// Provider health
app.emit("provider-status", ProviderHealth { name, state, latency_ms });
```

#### Daemon → Overlay: Unix Socket Framing Detail

```
┌──────────────────────────────────────────┐
│  Frame Layout (wire format)              │
├──────────────────────────────────────────┤
│  Bytes 0-3: payload length (u32 LE)      │
│  Bytes 4-N: UTF-8 JSON payload           │
└──────────────────────────────────────────┘

Max frame size: 64KB (enforced, reject larger)
Keepalive: daemon sends {"type":"Ping"} every 5s
Overlay responds: {"type":"Pong"}
Timeout: 15s without Pong → daemon respawns overlay
```

### 1.4 Database Schema (SQLite, WAL mode)

```sql
-- Core session management (CM1)
CREATE TABLE sessions (
    id          TEXT PRIMARY KEY,  -- UUID v4
    name        TEXT NOT NULL DEFAULT 'Untitled',
    state       TEXT NOT NULL DEFAULT 'active',  -- active|paused|archived
    created_at  INTEGER NOT NULL,  -- epoch ms
    updated_at  INTEGER NOT NULL,
    archived_at INTEGER,
    total_cost  REAL NOT NULL DEFAULT 0.0,  -- USD
    turn_count  INTEGER NOT NULL DEFAULT 0,
    metadata    TEXT  -- JSON blob for extensibility
);
CREATE INDEX idx_sessions_state ON sessions(state);
CREATE INDEX idx_sessions_updated ON sessions(updated_at DESC);

-- Turn-level message storage (CM1)
CREATE TABLE turns (
    id            TEXT PRIMARY KEY,  -- UUID v4
    session_id    TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    role          TEXT NOT NULL,  -- user|assistant|system
    content       TEXT NOT NULL,
    lane          TEXT,  -- snap|solve|think (NULL for user turns)
    model         TEXT,  -- model ID used
    created_at    INTEGER NOT NULL,
    latency_ms    INTEGER,  -- TTFT for assistant turns
    token_input   INTEGER,
    token_output  INTEGER,
    cost          REAL,  -- USD for this turn
    is_patch      INTEGER NOT NULL DEFAULT 0,  -- 1 if patch-mode follow-up
    parent_turn   TEXT REFERENCES turns(id),  -- for follow-up chains
    metadata      TEXT  -- JSON: patch_ops, thinking_tokens, etc.
);
CREATE INDEX idx_turns_session ON turns(session_id, created_at);
CREATE INDEX idx_turns_lane ON turns(lane);

-- RAG vector store (B5.1)
CREATE TABLE chunks (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    content     TEXT NOT NULL,
    embedding   BLOB,  -- f32 vector via sqlite-vec
    chunk_type  TEXT NOT NULL,  -- transcript|summary|context|document
    created_at  INTEGER NOT NULL,
    token_count INTEGER NOT NULL
);
CREATE INDEX idx_chunks_session ON chunks(session_id);

-- Epoch summaries (CM3)
CREATE TABLE epoch_summaries (
    id          TEXT PRIMARY KEY,
    session_id  TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    summary     TEXT NOT NULL,
    turn_start  TEXT NOT NULL REFERENCES turns(id),
    turn_end    TEXT NOT NULL REFERENCES turns(id),
    token_count INTEGER NOT NULL,
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_epochs_session ON epoch_summaries(session_id);

-- Provider configuration
CREATE TABLE providers (
    name        TEXT PRIMARY KEY,  -- cerebras|anthropic|openai|deepgram|groq
    api_key     TEXT,  -- encrypted at rest via keychain
    base_url    TEXT,
    enabled     INTEGER NOT NULL DEFAULT 1,
    config      TEXT  -- JSON: model overrides, rate limits, etc.
);

-- Skill templates (R4)
CREATE TABLE skills (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    lane        TEXT NOT NULL,  -- snap|solve|think|auto
    system_prompt TEXT NOT NULL,
    user_template TEXT,
    category    TEXT,  -- coding|behavioral|system_design|general
    is_builtin  INTEGER NOT NULL DEFAULT 0,
    created_at  INTEGER NOT NULL,
    updated_at  INTEGER NOT NULL
);

-- Settings (single row, JSON columns)
CREATE TABLE settings (
    id          INTEGER PRIMARY KEY CHECK (id = 1),
    audio       TEXT NOT NULL DEFAULT '{}',
    display     TEXT NOT NULL DEFAULT '{}',
    shortcuts   TEXT NOT NULL DEFAULT '{}',
    providers   TEXT NOT NULL DEFAULT '{}',
    privacy     TEXT NOT NULL DEFAULT '{}',
    updated_at  INTEGER NOT NULL
);

-- Metrics/cost tracking (B8.4)
CREATE TABLE cost_log (
    id          TEXT PRIMARY KEY,
    session_id  TEXT REFERENCES sessions(id),
    provider    TEXT NOT NULL,
    lane        TEXT NOT NULL,
    tokens_in   INTEGER NOT NULL,
    tokens_out  INTEGER NOT NULL,
    cost_usd    REAL NOT NULL,
    cached      INTEGER NOT NULL DEFAULT 0,  -- prompt cache hit
    created_at  INTEGER NOT NULL
);
CREATE INDEX idx_cost_session ON cost_log(session_id);
CREATE INDEX idx_cost_date ON cost_log(created_at);

-- Schema version tracking
CREATE TABLE migrations (
    version     INTEGER PRIMARY KEY,
    applied_at  INTEGER NOT NULL,
    description TEXT
);
```

**FK relationships:**
```
sessions 1──────* turns
sessions 1──────* chunks
sessions 1──────* epoch_summaries
sessions 1──────* cost_log
turns    1──────? turns (parent_turn self-ref for follow-ups)
```

### 1.5 State Machines

#### Session Lifecycle

```
                    ┌─────────┐
         create     │   NEW   │
        ─────────▶  │(transient)│
                    └────┬────┘
                         │ first turn received
                         ▼
                    ┌─────────┐
              ┌────│  ACTIVE  │◄───────────────┐
              │    └────┬────┘                 │
              │         │                      │
    user archives       │ user pauses    user resumes
              │         ▼                      │
              │    ┌─────────┐                 │
              │    │  PAUSED  │────────────────┘
              │    └────┬────┘
              │         │ user archives
              ▼         ▼
         ┌──────────────────┐
         │     ARCHIVED     │
         │  (read-only)     │
         └──────────────────┘

Transitions:
  NEW → ACTIVE:     automatic on first user turn
  ACTIVE → PAUSED:  user action (Cmd+Shift+P or dashboard)
  PAUSED → ACTIVE:  user action (resume)
  ACTIVE → ARCHIVED: user action (archive)
  PAUSED → ARCHIVED: user action (archive)
  ARCHIVED → *:     NOT ALLOWED (terminal state)
```

#### STT Connection State Machine

```
                         ┌────────────────┐
            start()      │  DISCONNECTED  │
           ─────────────▶│                │
                         └───────┬────────┘
                                 │ WebSocket connect
                                 ▼
                         ┌────────────────┐
                    ┌───▶│  CONNECTING    │
                    │    └───────┬────────┘
                    │            │ WS open + auth OK
                    │            ▼
                    │    ┌────────────────┐
                    │    │   CONNECTED    │◄──────────────┐
                    │    └───────┬────────┘               │
                    │            │                        │
                    │    retryable error          first final transcript
                    │    (net/5xx/429/WS drop)    resets error counter
                    │            │                        │
                    │            ▼                        │
                    │    ┌────────────────┐               │
                    └────│ RECONNECTING   │───────────────┘
                         │ (backoff wait) │
                         └───────┬────────┘
                                 │ consecutive errors > 5
                                 ▼
                         ┌────────────────┐
                         │    FAILED      │
                         │ (fatal: auth/  │
                         │  quota/config) │
                         └────────────────┘

Backoff: min(30s × 2^(failures-1), 600s)
Source: CUE-REF-01A ai.service.js:L420, CUE-REFERENCE-ANALYSIS pattern #14
```

#### Lane Routing Decision Tree

```
User input arrives (stable transcript or typed query)
    │
    ├─ Check manual override flags:
    │   ├─ Alt+F pressed or /snap prefix → FORCE SNAP
    │   ├─ Alt+D pressed or /think /deep prefix → FORCE THINK
    │   └─ No override → continue to classifier
    │
    ├─ Rule-based pre-filter (<1ms):
    │   ├─ Length < 20 chars AND no code keywords → SNAP (confidence 0.95)
    │   ├─ Contains "think harder" / "explain in detail" → THINK (0.9)
    │   ├─ Is follow-up to previous answer → check F4 follow-up classifier
    │   └─ No rule match → embedding classifier
    │
    ├─ Embedding classifier (<5ms):
    │   ├─ Compute query embedding (cached model, 384-dim)
    │   ├─ Compare against lane centroids (precomputed)
    │   ├─ confidence ≥ 0.8 for Snap → SNAP
    │   ├─ confidence ≥ 0.7 for Solve → SOLVE
    │   ├─ confidence ≥ 0.6 for Think → THINK
    │   └─ Ambiguous (all < 0.6) → PARALLEL (Snap + Solve)
    │
    └─ PARALLEL mode:
        ├─ Dispatch Snap immediately (show result in <300ms)
        ├─ Dispatch Solve in parallel
        └─ When Solve arrives: replace Snap result with progressive fade
```

### 1.6 Critical Paths with Latency Budgets

| Path | Target p50 | Target p95 | Bottleneck |
|------|-----------|-----------|------------|
| Mic → STT partial | 150ms | 250ms | Deepgram network |
| STT stable → Snap complete | 200ms | 350ms | Cerebras TTFT |
| STT stable → Solve first token | 600ms | 1200ms | Claude TTFT |
| STT stable → Think progress | 1500ms | 3000ms | o3 thinking start |
| Overlay render (socket → pixels) | 3ms | 8ms | NSView redraw |
| Intent classifier | 2ms | 5ms | Embedding lookup |
| Session switch (UI update) | 20ms | 50ms | SQLite + socket |
| Dashboard open (first paint) | 80ms | 150ms | Pre-created window |

### 1.7 Failure Modes + Recovery

| Failure | Detection | Recovery | User Impact |
|---------|-----------|----------|-------------|
| Deepgram WS drop | Socket close event | Reconnect with backoff (L1), buffer audio during reconnect | 1-3s gap in transcription |
| Cerebras timeout | 5s no response | Fallback to Groq (same lane) | +200ms latency on that query |
| Claude rate limit | 429 response | Queue + retry after header delay, or fallback to Gemini 2.5 Pro | +500ms on first retry |
| o3 timeout | 90s no response | Fallback to Claude Opus with extended thinking | User sees progress bar reset |
| Overlay crash | Broken socket pipe | Respawn overlay process within 500ms | Brief flicker, state preserved |
| SQLite locked | SQLITE_BUSY | WAL mode + busy_timeout=5000ms handles most cases | Transparent to user |
| Network loss | All providers fail | Switch to offline mode (local answers only via intelligence.rs) | Degraded quality, instant response |
| Disk full | Write fails | Alert user, stop recording, continue with in-memory state | Session not persisted until space freed |
| API key invalid | 401 from provider | Mark provider as failed, try next in fallback chain | Transparent if fallback available |
| Memory pressure | RSS > 500MB | Flush RAG cache, compact epoch summaries, GC old chunks | Slight latency increase |


### 1.8 Hot Path Protocol: Streaming Token Delivery

The critical performance path is daemon → overlay token streaming. This must sustain 60Hz delivery with <5ms per-frame overhead.

**Wire protocol detail:**

```
Daemon streaming loop (Rust):
┌─────────────────────────────────────────────────────────────┐
│  loop {                                                      │
│    tokio::select! {                                          │
│      token = stream.next() => {                              │
│        buffer.push_str(&token);                              │
│      }                                                       │
│      _ = tick_60hz.tick() => {                               │
│        if !buffer.is_empty() {                               │
│          let frame = Frame::StreamToken {                     │
│            generation_id,                                    │
│            text: mem::take(&mut buffer),                      │
│          };                                                  │
│          let json = serde_json::to_vec(&frame)?;             │
│          let len = (json.len() as u32).to_le_bytes();        │
│          socket.write_all(&len).await?;                      │
│          socket.write_all(&json).await?;                     │
│        }                                                     │
│      }                                                       │
│      _ = cancel.cancelled() => break,                        │
│    }                                                         │
│  }                                                           │
└─────────────────────────────────────────────────────────────┘

Overlay receive loop (Swift/C):
┌─────────────────────────────────────────────────────────────┐
│  while connected {                                           │
│    read 4 bytes → payload_len                                │
│    read payload_len bytes → json_data                        │
│    parse JSON → message                                      │
│    switch message.type {                                     │
│      case .StreamToken:                                      │
│        appendToCurrentCard(message.text)                      │
│        scheduleRedraw()  // coalesced via CADisplayLink       │
│      case .PatchDiff:                                        │
│        applyDiffOps(message.ops)                             │
│        animateChanges()                                      │
│      case .ModeUpdate:                                       │
│        updateBadge(message.lane, message.model)              │
│    }                                                         │
│  }                                                           │
└─────────────────────────────────────────────────────────────┘
```

**Why Unix socket over stdin/stdout (V2 → V3 upgrade):**

| Aspect | stdin/stdout (V2) | Unix socket (V3) |
|--------|-------------------|-------------------|
| Latency | 5-15ms (pipe buffering) | 1-3ms (direct) |
| Bidirectional | Awkward (separate streams) | Natural (single fd) |
| Reconnection | Impossible (process restart) | Reconnect to same path |
| Multiple clients | Impossible | Multiple connections supported |
| Backpressure | Pipe buffer fills, blocks | Non-blocking with flow control |
| Windows equivalent | Named pipe `\\.\pipe\bluey` | Named pipe (same semantics) |

*Source: CUE-DESIGN-02 signal path analysis, CUE-BLUEY-V3-PLAN-SKELETON L5 task description*

### 1.9 Configuration System

```
~/.local/share/bluey/
├── bluey.db              # SQLite database (sessions, turns, chunks, cost_log)
├── overlay.sock          # Unix domain socket (runtime only)
├── daemon.pid            # PID file for single-instance lock
├── config.json           # Non-sensitive settings (theme, shortcuts, audio prefs)
└── logs/
    ├── daemon.log        # Current log (NDJSON format)
    ├── daemon.log.1      # Rotated (10MB max per file)
    └── overlay.log       # Overlay process logs

macOS Keychain / Windows Credential Vault:
├── bluey.cerebras.api_key
├── bluey.anthropic.api_key
├── bluey.openai.api_key
├── bluey.deepgram.api_key
└── bluey.groq.api_key
```

**Config loading order:**
1. Defaults (compiled into binary)
2. `config.json` (user overrides)
3. Environment variables (`BLUEY_*` prefix)
4. CLI flags (highest priority)

**Hot-reload**: `config.json` is watched via `notify` crate. Changes apply without restart (except provider keys which require re-auth).

### 1.10 Tauri Bridge Architecture

The Tauri dashboard runs in the same process as the daemon (shared Rust binary). This is NOT a separate process — it's a WebView managed by the Tauri runtime within the daemon process.

```
┌─────────────────────────────────────────────────────────────┐
│  cue-daemon binary (single process)                          │
│                                                              │
│  ┌──────────────────────┐  ┌──────────────────────────────┐ │
│  │  Daemon Core         │  │  Tauri Runtime               │ │
│  │  (tokio runtime)     │  │  (manages WebView)           │ │
│  │                      │  │                              │ │
│  │  • Audio pipeline    │  │  • Window management         │ │
│  │  • STT connection    │  │  • IPC bridge (invoke/event) │ │
│  │  • LLM routing       │◄─┤  • Plugin system             │ │
│  │  • Session state     │──▶│  • Global shortcuts          │ │
│  │  • RAG engine        │  │                              │ │
│  │  • Socket server     │  │  WebView (React 19):         │ │
│  │  • TCP server        │  │  • Dashboard UI              │ │
│  │                      │  │  • Settings pages            │ │
│  └──────────────────────┘  │  • Session browser           │ │
│                             └──────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘

Tauri invoke flow:
  React calls: invoke('list_sessions')
    → Tauri IPC bridge (WebView → Rust, ~1ms)
    → #[tauri::command] handler
    → Accesses shared AppState (Arc<Mutex<...>>)
    → Returns serialized result
    → React receives JSON response
  Total: 10-30ms depending on DB query

Tauri event flow:
  Daemon emits: app.emit("llm-token", chunk)
    → Tauri serializes to JSON
    → Pushes to WebView event queue
    → React listener fires
  Total: 5-15ms
```

**Why single-binary (not separate processes):**
- Shared memory access (no serialization for internal state)
- Single SQLite connection (no locking issues)
- Simpler deployment (one binary to ship)
- Tauri's architecture naturally embeds in the host process

*Source: CUE-REF-02 (pluely) Tauri 2 architecture, CUE-DESIGN-04 Part 1*

### 1.11 HTTP/2 Connection Pool Design (L2)

```
┌─────────────────────────────────────────────────────────────┐
│  Connection Pool Manager                                     │
│                                                              │
│  Pool 1: Cerebras (Snap)                                     │
│  ├─ Connection A: HTTP/2 multiplexed, prewarmed              │
│  ├─ Connection B: standby (activated on A failure)           │
│  └─ Health check: GET /health every 30s                      │
│                                                              │
│  Pool 2: Anthropic (Solve)                                   │
│  ├─ Connection A: HTTP/2 multiplexed, prewarmed              │
│  ├─ Connection B: standby                                    │
│  └─ Health check: lightweight request every 60s              │
│                                                              │
│  Pool 3: OpenAI (Think)                                      │
│  ├─ Connection A: HTTP/2 multiplexed, prewarmed              │
│  ├─ Connection B: standby                                    │
│  └─ Health check: lightweight request every 60s              │
│                                                              │
│  Prewarm strategy:                                           │
│  • On daemon start: establish all 3 primary connections      │
│  • TLS handshake + HTTP/2 SETTINGS frame completed           │
│  • First real request avoids cold-start penalty (~200ms)     │
│  • Connections kept alive via HTTP/2 PING frames             │
│  • Reconnect on idle timeout (provider-specific, 60-300s)    │
└─────────────────────────────────────────────────────────────┘
```

**Latency savings from prewarming:**
| Without pool | With pool | Savings |
|-------------|-----------|---------|
| DNS: 20ms | 0ms (cached) | 20ms |
| TCP: 30ms | 0ms (reused) | 30ms |
| TLS: 80ms | 0ms (reused) | 80ms |
| HTTP/2 negotiate: 20ms | 0ms (established) | 20ms |
| **Total cold-start avoided** | | **150ms** |

*Source: CUE-BLUEY-V3-PLAN-SKELETON L2 task, CUE-DESIGN-03 HTTP client design*

## S2: Competitive Analysis

### 2.1 Product Landscape

The AI interview/conversation copilot space has 9 notable competitors as of May 2026. Analysis based on source code review (CUE-REF-01A through CUE-REF-06), public documentation, and feature comparison.

### 2.2 Feature Matrix

| Feature | bluey V3 | Cluely (natively) | Final Round AI | LockedIn AI | Interview Coder | natively-cluely (OSS) | pluely | solveWatchAi | Vysper | Aura |
|---------|----------|-------------------|----------------|-------------|-----------------|----------------------|--------|--------------|--------|------|
| **Stealth** | | | | | | | | | | |
| Content protection | ✅ Both platforms | ✅ Both | ❌ | 🟡 macOS only | ❌ | ✅ Both | ✅ macOS | ❌ | ❌ | ❌ |
| Process masquerading | ✅ 3 presets | ✅ 7-step | ❌ | ❌ | ❌ | ✅ 7-step | ❌ | ❌ | ❌ | ❌ |
| Screen-share detection | ✅ Planned | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Always-on-top | ✅ NSPanel | ✅ NSPanel | ✅ | ✅ | ✅ | ✅ NSPanel | ✅ NSPanel | ✅ | ✅ | ❌ |
| Click-through | ✅ Planned | ✅ | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| **STT** | | | | | | | | | | |
| Provider | Deepgram Nova-3 | 9 providers | Unknown (cloud) | Deepgram | None (text input) | 9 providers | REST (custom) | MLX Whisper + Deepgram | Azure Speech | Deepgram |
| Streaming WebSocket | ✅ Persistent | ✅ Per-session | ❌ REST | ✅ | N/A | ✅ | ❌ REST batch | ✅ Local | ✅ | ✅ |
| Speaker ID | ✅ ECAPA-TDNN | ❌ | ❌ | ❌ | N/A | ❌ | ❌ | ✅ SpeechBrain | ❌ | ❌ |
| VAD | ✅ Two-stage | ✅ Two-stage | Unknown | ❌ | N/A | ✅ Two-stage | ✅ Single-stage | ✅ Silero | ❌ | ❌ |
| **LLM** | | | | | | | | | | |
| Providers supported | 5+ (Cerebras, Claude, o3, Groq, Gemini) | 7 (OpenAI, Claude, Gemini, Groq, Ollama, cURL, Pro) | 1 (GPT-4) | 1 (GPT-4) | 1 (GPT-4) | 7 | 2 (API + cURL) | 3 (OpenAI, Groq, Ollama) | 1 (Azure OpenAI) | 4 (OpenAI, Claude, Gemini, Groq) |
| Query-type routing | ✅ Three-lane | ❌ Single model | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Sub-300ms responses | ✅ Snap lane | ❌ ~2s min | ❌ ~3s | ❌ ~2s | ❌ | ❌ ~2s | ❌ ~3s | 🟡 Local Whisper fast | ❌ ~2s | ❌ ~2.5s |
| Extended thinking | ✅ Think lane (o3) | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| Prompt caching | ✅ Anthropic cache | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Follow-ups** | | | | | | | | | | |
| Patch mode (diff) | ✅ PATCH/KEEP/MODIFY/ADD/REMOVE | ❌ Full replace | ❌ Full replace | ❌ Full replace | ❌ | ❌ Full replace | ❌ Full replace | ❌ Full replace | ❌ Full replace | ❌ Full replace |
| Progressive enhancement | ✅ Snap→Solve upgrade | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ |
| **Memory/RAG** | | | | | | | | | | |
| Local RAG | ✅ sqlite-vec | ❌ Cloud only | ❌ | ❌ | ❌ | ✅ sqlite-vec | ❌ | ❌ | ❌ | ❌ |
| Epoch summarization | ✅ Background job | ❌ Truncation | ❌ | ❌ | ❌ | ✅ | ❌ | ❌ | ❌ | ❌ |
| Context window mgmt | ✅ Per-lane strategies | 🟡 Basic truncation | ❌ | ❌ | ❌ | 🟡 Basic | ❌ | 🟡 Rolling window | ❌ | ❌ |
| **Platform** | | | | | | | | | | |
| macOS | ✅ Native Swift | ✅ Electron | ✅ Web | ✅ Electron | ✅ Electron | ✅ Electron | ✅ Tauri | ✅ Electron | ✅ Electron | ✅ Python |
| Windows | ✅ Native C | ✅ Electron | ✅ Web | ❌ | ✅ Electron | ✅ Electron | 🟡 Partial | ✅ Electron | ❌ | ❌ |
| Linux | 🟡 Planned | ✅ Electron | ✅ Web | ❌ | ❌ | ✅ Electron | ✅ Tauri | ❌ | ❌ | ❌ |
| **Other** | | | | | | | | | | |
| Local inference | ❌ (by design) | 🟡 Ollama optional | ❌ | ❌ | ❌ | 🟡 Ollama | ❌ | ✅ MLX Whisper | ❌ | ❌ |
| Skill templates | ✅ 9+ extensible | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ System prompts | ❌ | ✅ 9 templates | ❌ |
| Open source | ✅ | ❌ | ❌ | ❌ | ❌ | ✅ GPL-3.0 | ✅ GPL-3.0 | ✅ MIT | ✅ | ✅ MIT |
| Pricing | TBD ($29/mo target) | $15-25/mo | $10-15/mo | $20/mo | $60 one-time | Free (self-host) | Free (self-host) | Free (self-host) | Free (self-host) | Free (self-host) |
| Binary size | ~30MB | ~270MB (Electron) | N/A (web) | ~250MB | ~200MB | ~270MB | ~10MB | ~180MB | ~150MB | ~50MB (Python) |
| Cost/3hr session | $1-4 | $15-25 (subscription) | $10-15 | $10-15 | N/A | $5-10 (user keys) | $3-8 | $2-5 (local STT) | $3-8 | $3-8 |

*Sources: CUE-REF-01A (natively-cluely), CUE-REF-02 (pluely), CUE-REF-03 (solveWatchAi), CUE-REF-04 (Aura), CUE-REF-05 (Vysper), CUE-REF-06 (OpenCluely), CUE-REFERENCE-ANALYSIS patterns #1-188*

### 2.3 Latency Comparison

| Product | Mic → First Token (p50) | Architecture Bottleneck |
|---------|------------------------|------------------------|
| **bluey V3 (Snap)** | **~310ms** | Deepgram streaming + Cerebras |
| **bluey V3 (Solve)** | **~800ms** | Deepgram + Claude TTFT |
| Cluely | ~2500ms | REST STT + single OpenAI call |
| Final Round AI | ~3000ms | Web-based, REST everything |
| LockedIn AI | ~2000ms | Deepgram WS + GPT-4 |
| natively-cluely | ~2000ms | NAPI bridge + REST STT + OpenAI |
| pluely | ~2500ms | REST batch STT + single provider |
| solveWatchAi | ~1500ms | Local Whisper (fast) + Groq |
| Vysper | ~2000ms | Azure Speech + Azure OpenAI |
| Aura | ~2500ms | Deepgram + multi-provider |

*Source: CUE-BLUEY-V3-PLAN-SKELETON Part 4, CUE-DESIGN-02 latency analysis*

### 2.4 What bluey Does Better Than Each Competitor

| Competitor | bluey's Advantage |
|------------|-------------------|
| **Cluely** | 8× faster conversational responses (Snap lane), 60% cheaper per session, open source, no Electron bloat, patch-mode diffs instead of full regeneration |
| **Final Round AI** | Native desktop app (not web), content protection, 10× faster, local RAG, works offline for basic answers |
| **LockedIn AI** | Cross-platform (Windows), three-lane routing, extended thinking for hard problems, speaker ID, open source |
| **Interview Coder** | Audio-based (not text-only), real-time streaming, multi-provider fallback, 95% cheaper ongoing cost |
| **natively-cluely** | Rust-native (no NAPI bridge), three-lane routing, patch-mode, persistent STT WebSocket, 27× smaller binary |
| **pluely** | Three-lane routing, persistent STT (not REST batch), speaker ID, RAG, extended thinking, Windows support |
| **solveWatchAi** | Three-lane routing, patch-mode, native overlay (not Electron), content protection, skill templates |
| **Vysper** | Multi-provider LLM (not Azure-only), streaming STT, content protection, RAG, cross-platform |
| **Aura** | Native overlay, content protection, three-lane routing, patch-mode, skill templates, production-grade |

### 2.5 Gaps Remaining (vs best-in-class per feature)

| Gap | Best-in-class | bluey Status | Mitigation |
|-----|---------------|--------------|------------|
| Phone mirroring | Cluely (PhoneMirrorService) | Not planned | Low priority — niche use case |
| Calendar integration | Cluely (CalendarManager) | Not planned V3 | Future V4 feature |
| Company dossier | Cluely (KnowledgeOrchestrator) | Not planned | Could add as skill template |
| Local inference | solveWatchAi (MLX Whisper) | Explicitly excluded | Design choice: cloud-first for quality |
| Web client | Final Round AI | Not planned | Desktop-only is a feature (stealth) |
| Team/enterprise features | Cluely (teams, billing) | Not in V3 | Future if productized |

### 2.6 Positioning Statement

> **bluey** is the fastest, most intelligent AI conversation copilot. While competitors use a single LLM for everything (slow, expensive, one-size-fits-none), bluey routes each query to the optimal model in <5ms — delivering sub-300ms conversational answers, streaming technical solutions, and deep extended-thinking for the hardest problems. Patch-mode follow-ups show exactly what changed, not full regenerations. Native overlays on both platforms ensure zero detection risk. Open source, self-hosted, $1-4 per 3-hour session.

**Target user**: Senior engineers in live technical interviews, system design discussions, and coding assessments who need instant, accurate, undetectable assistance across varying difficulty levels.

**Key differentiator**: Three-lane intelligent routing. No other product dispatches queries to different models based on complexity. This is bluey's moat — it's architecturally fundamental, not a feature flag.

### 2.7 Per-Competitor Deep Analysis

#### Cluely (natively-cluely) — The Market Leader

**Architecture**: Electron + React + Rust native module (NAPI bridge)
**Size**: 270MB binary, 42K LOC backend, 204K LOC total
**Strengths**:
- Most complete feature set (9 STT providers, 7 LLM providers, phone mirroring, calendar)
- Battle-tested stealth (7-step masquerading, content protection both platforms)
- Premium features (company dossier, negotiation tracker, personas)
- Large user base provides feedback loop

**Weaknesses**:
- Single-model architecture (no routing) — every query goes to same provider
- ~2500ms mic-to-first-token (NAPI bridge + REST STT + single LLM call)
- Full-replace on follow-ups (regenerates entire answer)
- 270MB binary (Electron bloat)
- Closed source, $15-25/month subscription
- 3894-line god object (LLMHelper.ts) — technical debt

**bluey's edge**: 8× faster on conversational queries, 60% cheaper, open source, patch-mode diffs, three-lane routing means better answers for hard problems AND faster answers for easy ones.

*Source: CUE-REF-01A (3000+ lines of analysis), CUE-REFERENCE-ANALYSIS patterns #1-155*

#### Final Round AI — The Web-Based Competitor

**Architecture**: Web application (no desktop client)
**Strengths**: No install required, works on any platform with a browser
**Weaknesses**:
- No content protection (web apps can't hide from screen recording)
- No system audio capture (browser can't access system audio without extension)
- ~3000ms latency (REST everything, no streaming STT)
- Single LLM provider (GPT-4)
- $10-15/month subscription

**bluey's edge**: Native desktop = content protection + system audio + always-on-top. 10× faster. Works during screen-shared interviews (their primary weakness).

#### solveWatchAi — The Local-First Competitor

**Architecture**: Python + Electron, local MLX Whisper for STT
**Size**: 56K LOC
**Strengths**:
- Local STT (no cloud dependency for transcription)
- Speaker ID via SpeechBrain ECAPA-TDNN
- Silero VAD (good noise rejection)
- MIT license, self-hostable

**Weaknesses**:
- No content protection (Electron without private API)
- No three-lane routing (single model)
- Python backend (slower than Rust for audio processing)
- No patch-mode follow-ups
- Limited to 3 LLM providers

**bluey's edge**: Content protection, three-lane routing, patch-mode, native overlay, Rust performance, 5+ LLM providers with intelligent fallback.

*Source: CUE-REF-03 (1800 lines of analysis)*

#### pluely — The Closest Stack Match

**Architecture**: Tauri 2 + React 19 + Rust (identical stack to bluey)
**Size**: 10MB binary, 27K LOC
**Strengths**:
- Tiny binary (27× smaller than Cluely)
- NSPanel via tauri-nspanel (content protection)
- Clean Rust audio abstraction (SpeakerStream trait)
- GPL-3.0 open source
- Good UI (Radix + shadcn + cmdk)

**Weaknesses**:
- REST-batch STT (not streaming — high latency)
- Single LLM provider per session (no routing)
- No speaker ID
- No RAG / memory
- No extended thinking
- No patch-mode

**bluey's edge**: Streaming STT, three-lane routing, RAG, speaker ID, patch-mode, extended thinking. pluely is a good UI shell but lacks intelligence depth.

*Source: CUE-REF-02 (2000 lines of analysis), CUE-REFERENCE-ANALYSIS patterns #156-188*

#### Vysper — The Skill Template Pioneer

**Architecture**: Electron + Node.js, Azure-only
**Size**: 15K LOC
**Strengths**:
- 9 skill templates (coding, system design, behavioral, etc.)
- LLM-based intelligent question filtering
- Clean prompt architecture

**Weaknesses**:
- Azure-only (single provider, no fallback)
- No content protection
- No streaming STT (Azure continuous recognition)
- macOS only
- No RAG or memory
- No follow-up handling

**bluey's edge**: Multi-provider, content protection, streaming STT, RAG, patch-mode, cross-platform. bluey adopts Vysper's skill template concept (R4) but extends it with lane-aware routing.

*Source: CUE-REF-05 (1000 lines of analysis)*

#### Aura-AI — The Python Lightweight

**Architecture**: Python desktop app, multi-provider
**Size**: 17K LOC
**Strengths**:
- 4 LLM providers with key rotation
- Simple architecture (easy to understand)
- MIT license

**Weaknesses**:
- No content protection (Python can't do NSPanel)
- No overlay (uses system notifications or separate window)
- No streaming STT
- No VAD (sends everything to Deepgram)
- macOS only
- No follow-up handling

**bluey's edge**: Native overlay with content protection, three-lane routing, VAD, streaming STT, patch-mode, cross-platform, production-grade Rust backend.

*Source: CUE-REF-04 (1200 lines of analysis)*

### 2.8 Architecture Comparison

| Aspect | Electron-based (Cluely, solveWatchAi) | Tauri-based (pluely, bluey) | Web-based (Final Round) | Python (Aura) |
|--------|---------------------------------------|----------------------------|------------------------|---------------|
| Binary size | 200-270MB | 10-30MB | 0 (browser) | 50MB+ (with deps) |
| Memory usage | 300-500MB | 80-200MB | Browser tab | 100-200MB |
| Startup time | 3-5s | 1-2s | Instant (if loaded) | 2-3s |
| Content protection | Via Electron API | Via macOS private API | ❌ Impossible | ❌ Impossible |
| Audio access | Via native module (NAPI) | Direct Rust (zero-copy) | ❌ Limited | Via sounddevice |
| IPC overhead | NAPI bridge (~5ms/call) | Direct memory access | N/A | N/A |
| Cross-platform | ✅ (Chromium everywhere) | ✅ (native WebView) | ✅ (browser) | 🟡 (macOS focus) |
| Update size | 200MB+ full binary | 15-30MB | 0 (server-side) | pip install |

**bluey's architectural advantage**: Tauri gives us Electron's cross-platform reach with native performance. The hybrid approach (native overlay + Tauri dashboard) gets the best of both worlds — native rendering speed for the hot path, web tech flexibility for the settings UI.

## S3: Cost Model Deep-Dive

### 3.1 Provider Pricing (May 2026)

| Provider | Model | Input (per 1M tokens) | Output (per 1M tokens) | Cached Input | Notes |
|----------|-------|----------------------|------------------------|--------------|-------|
| **Cerebras** | DeepSeek V3 | $0.20 | $0.60 | N/A | Snap lane primary |
| **Anthropic** | Claude Sonnet 4.5 | $3.00 | $15.00 | $0.30 (90% off) | Solve lane primary |
| **OpenAI** | o3 | $10.00 | $40.00 | N/A | Think lane primary |
| **OpenAI** | o3-mini | $1.10 | $4.40 | N/A | Think lane fallback |
| **Anthropic** | Claude Opus 4 | $15.00 | $75.00 | $1.50 | Think lane fallback |
| **Groq** | Llama 4 Scout | $0.11 | $0.34 | N/A | Snap fallback |
| **Google** | Gemini 2.5 Pro | $1.25 | $10.00 | N/A | Solve fallback |
| **Google** | Gemini 2.5 Flash | $0.15 | $0.60 | N/A | Cost-optimized Solve |
| **Deepgram** | Nova-3 | $0.0043/sec | — | — | ~$0.26/min active speech |
| **OpenAI** | text-embedding-3-small | $0.02 | — | — | RAG embeddings |
| **AssemblyAI** | Universal Streaming | $0.0065/sec | — | — | STT fallback |

*Prices sourced from provider pricing pages as of May 2026. Subject to change.*

### 3.2 Unit Economics: Per-Session Cost Breakdown

#### Scenario: 3-Hour Coding Interview

| Component | Usage | Calculation | Cost |
|-----------|-------|-------------|------|
| **Deepgram STT** | 90 min active speech (VAD filters ~50% silence) | 90 × 60 × $0.0043 | $0.23 |
| **Snap lane** | 40 queries, avg 1.5K in / 800 out | 40 × (1.5K×$0.20 + 0.8K×$0.60) / 1M | $0.03 |
| **Solve lane** | 15 queries, avg 12K in / 3K out | 15 × (12K×$3.00 + 3K×$15.00) / 1M | $1.22 |
| **Solve (cached)** | Same, 60% cache hit on input | 15 × (4.8K×$3.00 + 7.2K×$0.30 + 3K×$15.00) / 1M | $0.92 |
| **Think lane** | 3 queries, avg 25K in / 6K out | 3 × (25K×$10.00 + 6K×$40.00) / 1M | $1.47 |
| **Embeddings** | 200 chunks × 200 tokens | 40K × $0.02 / 1M | $0.001 |
| **Total (no optimization)** | | | **$2.95** |
| **Total (with CO1 caching)** | | | **$2.65** |
| **Total (CO1 + CO2 RAG filter)** | | | **$1.85** |
| **Total (CO1 + CO2 + CO3 split)** | | | **$1.42** |

#### Scenario: 3-Hour System Design Interview

| Component | Usage | Calculation | Cost |
|-----------|-------|-------------|------|
| **Deepgram STT** | 100 min active (more discussion) | 100 × 60 × $0.0043 | $0.26 |
| **Snap lane** | 25 queries (fewer simple Q&A) | 25 × (1.5K×$0.20 + 0.8K×$0.60) / 1M | $0.02 |
| **Solve lane** | 20 queries (more complex) | 20 × (14K×$3.00 + 4K×$15.00) / 1M | $2.04 |
| **Think lane** | 5 queries (architecture decisions) | 5 × (30K×$10.00 + 8K×$40.00) / 1M | $3.10 |
| **Total (no optimization)** | | | **$5.42** |
| **Total (fully optimized)** | | | **$2.80** |

#### Scenario: 3-Hour Technical Phone Screen

| Component | Usage | Calculation | Cost |
|-----------|-------|-------------|------|
| **Deepgram STT** | 70 min active | 70 × 60 × $0.0043 | $0.18 |
| **Snap lane** | 60 queries (mostly conversational) | 60 × (1K×$0.20 + 0.5K×$0.60) / 1M | $0.03 |
| **Solve lane** | 8 queries | 8 × (10K×$3.00 + 2.5K×$15.00) / 1M | $0.54 |
| **Think lane** | 1 query | 1 × (20K×$10.00 + 5K×$40.00) / 1M | $0.40 |
| **Total (no optimization)** | | | **$1.15** |
| **Total (fully optimized)** | | | **$0.72** |

### 3.3 Cost Optimization Levers

| Lever | Task ID | Savings | Mechanism |
|-------|---------|---------|-----------|
| **CO1: Prompt caching** | CO1 | 50-70% on Solve input | Anthropic cache_control breakpoints on system prompt + skill template (stable across turns) |
| **CO2: RAG-filtered context** | CO2 | 30-50% on Solve input | Send only top-5 relevant turns instead of full history (16K → 6K avg) |
| **CO3: Router split** | CO3 | 20-40% on easy Solve | Route "easy" Solve queries (definition lookups, simple explanations) to Cerebras instead of Claude |
| **Speculative cancel** | L4 | 5-10% overall | Cancel in-flight LLM calls when STT partial changes (avoid wasted tokens) |
| **Epoch summarization** | CM3 | 15-25% on long sessions | Compress old turns to summaries, reducing context window for subsequent queries |

**Combined optimization effect:**
- Baseline: $2.95/session (coding interview)
- After all optimizations: $1.42/session (**52% reduction**)

### 3.4 Pricing Tier Recommendations

| Tier | Monthly Price | Included | Cost to Serve | Margin |
|------|--------------|----------|---------------|--------|
| **Free** | $0 | 3 sessions/month, Snap lane only | ~$0.30/session × 3 = $0.90 | -$0.90 (acquisition) |
| **Pro** | $29 | Unlimited sessions, all lanes | ~$1.50/session × 40 = $60 | -$31 (subsidized) |
| **Pro (light user)** | $29 | ~15 sessions/month typical | ~$1.50 × 15 = $22.50 | +$6.50 |
| **BYOK** | $9 | Unlimited, user provides API keys | ~$0.25/session (STT only) × 40 = $10 | -$1 (near break-even) |
| **Team** | $49/seat | + shared prompts, analytics, admin | Same as Pro + $2 infra | Depends on usage |

**Break-even analysis:**
- Pro tier breaks even at ~19 sessions/month ($29 ÷ $1.50)
- Average user does 10-20 sessions/month → Pro tier is viable at 15+ sessions
- Heavy users (40+ sessions) require BYOK tier or usage caps

### 3.5 Scenario Modeling: Scale Economics

#### 100 Users (Early Adopter Phase)

| Metric | Value |
|--------|-------|
| Mix | 60 Pro, 20 BYOK, 20 Free |
| Monthly revenue | 60×$29 + 20×$9 = $1,920 |
| Avg sessions/user/month | 15 |
| Monthly API cost | (60+20)×15×$1.50 + 20×3×$0.30 = $1,818 |
| Gross margin | $102 (5.3%) |
| **Verdict** | Not viable without BYOK emphasis or usage caps |

#### 1,000 Users (Growth Phase)

| Metric | Value |
|--------|-------|
| Mix | 400 Pro, 300 BYOK, 200 Free, 100 Team |
| Monthly revenue | 400×$29 + 300×$9 + 100×$49 = $19,200 |
| Monthly API cost | (400+100)×15×$1.50 + 300×0 + 200×3×$0.30 = $11,430 |
| Infra (servers, monitoring) | $500 |
| Gross margin | $7,270 (37.9%) |
| **Verdict** | Viable with BYOK tier absorbing heavy users |

#### 10,000 Users (Scale Phase)

| Metric | Value |
|--------|-------|
| Mix | 3000 Pro, 4000 BYOK, 2000 Free, 1000 Team |
| Monthly revenue | 3000×$29 + 4000×$9 + 1000×$49 = $172,000 |
| Monthly API cost | (3000+1000)×15×$1.20 + 2000×3×$0.30 = $73,800 |
| Volume discount (negotiated) | -20% = $59,040 |
| Infra | $3,000 |
| Gross margin | $109,960 (63.9%) |
| **Verdict** | Highly viable; volume discounts kick in |

*Note: At 10K users, negotiate dedicated capacity with Cerebras and Anthropic for 20-30% discount.*

### 3.6 BYOK Tier Economics

When users bring their own API keys:
- bluey only pays for: STT (Deepgram, ~$0.25/session) + infra
- User pays directly: LLM costs ($1-4/session to their own accounts)
- bluey charges: $9/month for software + STT
- Break-even: 1 session/month ($9 revenue vs $0.25 cost)
- This tier is **always profitable** and should be the default recommendation for heavy users

### 3.7 Reserve Capacity Considerations

| Provider | Threshold for Negotiation | Expected Discount | Commitment |
|----------|--------------------------|-------------------|------------|
| Cerebras | >$5K/month spend | 20-30% | 6-month minimum |
| Anthropic | >$10K/month spend | 15-25% | Annual contract |
| OpenAI | >$10K/month spend | 10-20% | Annual + volume commit |
| Deepgram | >$3K/month spend | 15-25% | Annual contract |

**Trigger**: Negotiate when reaching 1,000+ paying users (~$10K/month API spend).

### 3.8 Cost Dashboard Spec (What Users See)

```
┌─────────────────────────────────────────────────┐
│  Session Cost: $1.47                            │
│  ├─ STT: $0.23 (Deepgram Nova-3, 87 min)       │
│  ├─ Snap: $0.03 (38 queries, Cerebras)         │
│  ├─ Solve: $0.82 (12 queries, Claude Sonnet)   │
│  │   └─ Cache savings: -$0.34                  │
│  ├─ Think: $0.39 (2 queries, o3)               │
│  └─ Embeddings: $0.001                         │
│                                                 │
│  Monthly Total: $18.42 / $29.00 plan            │
│  ├─ Sessions this month: 14                     │
│  ├─ Avg cost/session: $1.32                     │
│  └─ Projected month-end: $24.50                 │
│                                                 │
│  [View detailed breakdown] [Export CSV]          │
└─────────────────────────────────────────────────┘
```

**Dashboard data source**: `cost_log` SQLite table, aggregated per session and per month.
**Update frequency**: Real-time (each LLM response updates running total via Tauri event `cost-update`).

## S4: Success Metrics + Acceptance Tests

### 4.1 Latency Targets

| Metric | p50 | p95 | p99 | Measurement Point |
|--------|-----|-----|-----|-------------------|
| Mic → STT partial | 150ms | 250ms | 400ms | L6 timestamp: audio_captured → stt_partial_received |
| STT stable → Snap complete | 200ms | 350ms | 500ms | L6: stt_stable → snap_generation_complete |
| STT stable → Solve first token | 600ms | 1200ms | 2000ms | L6: stt_stable → solve_first_token |
| STT stable → Think progress visible | 1500ms | 3000ms | 5000ms | L6: stt_stable → think_progress_shown |
| End-to-end mic → first visible (Snap) | 400ms | 550ms | 700ms | L6: audio_captured → overlay_rendered |
| End-to-end mic → first visible (Solve) | 800ms | 1500ms | 2500ms | L6: audio_captured → overlay_rendered |
| Intent classifier decision | 2ms | 5ms | 10ms | L6: classifier_start → classifier_done |
| Overlay render (socket → pixels) | 3ms | 8ms | 15ms | L6: socket_received → frame_committed |
| Session switch (full UI update) | 20ms | 50ms | 100ms | L6: switch_command → overlay_updated |
| Dashboard open (first paint) | 80ms | 150ms | 300ms | L6: hotkey_pressed → webview_painted |

**Instrumentation**: Task L6 adds timestamps at every hop. Stored in SQLite `latency_traces` table. Exposed via dashboard flamegraph and `bluey metrics` CLI command.

### 4.2 Correctness Targets

#### LeetCode Pass Rates (bluey's generated answers)

| Difficulty | Target Pass Rate | Evaluation Method |
|------------|-----------------|-------------------|
| Easy | ≥95% | Automated: submit to LeetCode API, check accepted |
| Medium | ≥80% | Automated: same |
| Hard | ≥60% | Automated: same |
| System Design | ≥85% (human eval) | Manual: 3 senior engineers score 1-5 on rubric |

**Evaluation corpus**: 50 Easy + 50 Medium + 30 Hard + 20 System Design questions.
**Frequency**: Run full eval suite before each phase ships. Track regression.

#### Intent Classifier Accuracy

| Metric | Target | Evaluation |
|--------|--------|------------|
| Overall accuracy | ≥85% | 500-question held-out test set with ground-truth lane labels |
| Snap precision | ≥90% | False positives (routed to Snap but needed Solve) < 10% |
| Think recall | ≥80% | Hard problems correctly identified as Think-worthy |
| Follow-up detection | ≥90% | Refinement vs new problem classification |

#### Patch-Mode Correctness

| Metric | Target | Evaluation |
|--------|--------|------------|
| Semantic correctness | ≥95% | Patch ops produce valid, meaningful output |
| No content loss | 100% | KEEP blocks never dropped or corrupted |
| Diff rendering accuracy | ≥98% | Visual diff matches actual changes |

### 4.3 Reliability Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Session uptime (no crashes) | ≥99.5% per 3hr session | Crash reports / total sessions |
| Audio recovery from disconnect | <2s | Time from device_lost → audio_resumed |
| Deepgram reconnect time | <3s | Time from ws_close → ws_open (L1) |
| Provider failover success | ≥99% | Fallback chain resolves without user-visible error |
| No message loss on crash | 100% | WAL mode + fsync ensures durability |
| Graceful degradation | 100% | Snap-only mode activates if Solve/Think unavailable |
| SQLite corruption rate | 0% | WAL + integrity checks on startup |

### 4.4 UX Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| Three-lane badge accuracy | 100% | Badge always reflects actual lane used |
| Patch diff render correctness | ≥98% | Visual inspection on 100 patch operations |
| Progressive enhancement smoothness | No flicker | Snap→Solve transition uses crossfade animation |
| Manual override response time | <100ms | Alt+D/F → lane switch acknowledged |
| Dashboard responsiveness | <100ms for all interactions | No janky scrolling, instant tab switches |
| Overlay CPU usage (idle) | <1% | Activity Monitor measurement |
| Overlay CPU usage (streaming) | <5% | During active token streaming |

### 4.5 Cost Targets

| Metric | Target | Measurement |
|--------|--------|-------------|
| Average session cost (coding) | <$2.00 (optimized) | cost_log aggregation |
| Average session cost (system design) | <$3.50 (optimized) | cost_log aggregation |
| Snap query cost | <$0.002 per query | Per-query cost tracking |
| No runaway costs | Max $10/session hard cap | Rate limiter + token budget enforcement |
| Cost dashboard accuracy | ±5% of actual bill | Compare dashboard totals vs provider invoices |

### 4.6 Performance Targets (User's Machine)

| Metric | Target | Measurement |
|--------|--------|-------------|
| Daemon RSS (idle) | <80MB | `ps` measurement after 1hr idle |
| Daemon RSS (active session) | <200MB | During active 3hr session |
| Daemon CPU (idle) | <0.5% | No audio, no queries |
| Daemon CPU (listening) | <3% | Audio capture + VAD active |
| Daemon CPU (generating) | <8% | During LLM streaming |
| Overlay RSS | <30MB | Native process memory |
| Dashboard RSS | <120MB | WebView + React |
| Battery impact (laptop) | <5% per hour active | Measured on M1 MacBook Air |
| Disk usage (SQLite) | <500MB after 100 sessions | With epoch summarization compaction |
| Startup time (cold) | <2s | From `bluey on` to overlay visible |
| Startup time (warm) | <500ms | Daemon already running, show overlay |

### 4.7 Test Suites

#### Unit Tests (Rust: `cargo test`)

| Module | Test Count | Coverage Target | What's Tested |
|--------|-----------|-----------------|---------------|
| `cue-core::ipc` | 30+ | 95% | Message serialization round-trips, all variants |
| `cue-core::ai` | 20+ | 90% | Provider routing logic, budget calculations |
| `cue-daemon::router` | 25+ | 90% | Intent classification, lane selection, override handling |
| `cue-daemon::patch` | 20+ | 95% | PATCH/KEEP/MODIFY/ADD/REMOVE parsing + application |
| `cue-daemon::context` | 15+ | 85% | Context assembly strategies, token counting |
| `cue-daemon::stt` | 10+ | 80% | State machine transitions, backoff calculation |
| `cue-daemon::cost` | 10+ | 90% | Cost calculation accuracy per provider |
| `cue-daemon::db` | 15+ | 85% | Migration system, CRUD operations |

**Run**: `cargo test --workspace` — must pass before every commit.
**CI gate**: All unit tests pass with zero warnings.

#### Integration Tests

| Test | What It Validates | Setup |
|------|-------------------|-------|
| Daemon↔Overlay IPC | Unix socket framing, all message types | Spawn daemon + mock overlay |
| Daemon↔Dashboard IPC | Tauri invoke/event round-trips | Spawn daemon + Tauri test harness |
| STT pipeline | Audio file → Deepgram → transcript | Requires Deepgram API key (CI secret) |
| LLM pipeline | Transcript → router → provider → response | Requires provider API keys |
| Session lifecycle | Create → turns → archive → query | SQLite in-memory |
| RAG pipeline | Index chunks → query → retrieve | sqlite-vec in-memory |
| Fallback chain | Primary fails → fallback succeeds | Mock HTTP server with error injection |
| Cost tracking | Queries → accurate cost_log entries | Mock providers with known token counts |

**Run**: `cargo test --features integration` — requires API keys in env.
**CI gate**: Run nightly (expensive due to API calls).

#### End-to-End Smoke Tests

| Test | Steps | Pass Criteria |
|------|-------|---------------|
| **Cold start** | `bluey on` → verify overlay appears | Overlay visible within 2s |
| **Basic query** | Type question in overlay → get answer | Response within 5s, non-empty |
| **Three-lane routing** | Send simple/medium/hard queries | Each routes to correct lane (check badge) |
| **Patch follow-up** | Ask question → ask refinement | Second response uses PATCH format |
| **Session persistence** | Create session → add turns → restart daemon → verify turns exist | All turns recovered |
| **Provider failover** | Set invalid primary key → query | Fallback provider responds |
| **Audio capture** | Start listening → speak → verify transcript | Transcript appears within 2s |
| **Dashboard** | Cmd+Shift+D → navigate pages | All pages render without errors |
| **Graceful shutdown** | `bluey off` during active session | Session saved, no data loss |
| **Crash recovery** | Kill overlay process | Overlay respawns within 1s |

**Run**: `./scripts/smoke-test.sh` — automated where possible, manual for audio.
**Gate**: Must pass before each phase ships.

#### Latency Benchmarks

| Benchmark | Method | Target |
|-----------|--------|--------|
| Intent classifier throughput | 10,000 classifications, measure p50/p95/p99 | p99 < 10ms |
| Socket round-trip | 10,000 ping/pong messages | p99 < 5ms |
| Context assembly | 100 sessions with 500+ turns each | p99 < 50ms |
| RAG retrieval | 1,000 queries against 10K chunks | p99 < 100ms |
| Token streaming render | 1,000 tokens at 60Hz | Zero dropped frames |

**Run**: `cargo bench` (criterion.rs benchmarks).
**Gate**: No regression >10% from previous run.

#### Correctness Eval Suite

| Suite | Size | Method | Frequency |
|-------|------|--------|-----------|
| LeetCode Easy | 50 problems | Automated submission | Per-phase |
| LeetCode Medium | 50 problems | Automated submission | Per-phase |
| LeetCode Hard | 30 problems | Automated submission | Per-phase |
| System Design | 20 scenarios | Human eval (rubric) | Per-phase |
| Behavioral | 30 questions | Human eval (naturalness) | Per-phase |
| Intent Classification | 500 labeled queries | Automated accuracy check | Per-commit |
| Follow-up Detection | 200 labeled pairs | Automated accuracy check | Per-commit |

### 4.8 Review Checklist (Principal Engineer Gate)

Before merging each phase batch:

**Architecture:**
- [ ] No new IPC message types without schema documentation
- [ ] No synchronous blocking on async runtime
- [ ] All new SQLite tables have migrations + indexes
- [ ] Error types are classified (retryable vs fatal)
- [ ] No unwrap() on fallible operations in production paths

**Performance:**
- [ ] Latency benchmarks show no regression >10%
- [ ] Memory profiling shows no leaks (run 1hr stress test)
- [ ] No allocations in hot path (streaming loop)
- [ ] CPU profiling shows no unexpected hotspots

**Reliability:**
- [ ] All error paths have recovery strategies
- [ ] Provider failures trigger fallback (not panic)
- [ ] SQLite operations use transactions where needed
- [ ] Crash recovery tested (kill -9 during operation)

**Security:**
- [ ] API keys never logged (even at trace level)
- [ ] No secrets in SQLite (use keychain)
- [ ] Content protection verified on both platforms
- [ ] No PII in telemetry events

**Code Quality:**
- [ ] All public APIs documented with rustdoc
- [ ] Unit test coverage ≥80% for new code
- [ ] No clippy warnings
- [ ] CHANGELOG.md updated
- [ ] CLAUDE.md updated if architecture changed

**UX:**
- [ ] Overlay renders correctly at all opacity levels
- [ ] Dashboard pages load without console errors
- [ ] Keyboard shortcuts work in all modes
- [ ] Error states show user-friendly messages

## S5: Risk Register

### 5.1 Technical Risks

| # | Risk | Likelihood | Impact | Mitigation | Early Warning |
|---|------|-----------|--------|------------|---------------|
| T1 | **Cerebras API instability** — Cerebras is newer, may have outages or breaking changes | Medium | High (Snap lane unusable) | Groq as hot fallback (same latency class), auto-switch on 3 consecutive failures | Monitor Cerebras status page; alert on p95 latency >500ms |
| T2 | **Tauri 2 breaking changes** — Tauri 2 is relatively new, plugin ecosystem may shift | Low | Medium (dashboard rebuild) | Pin exact Tauri versions, avoid bleeding-edge plugins, keep dashboard logic in React (portable) | Watch Tauri GitHub releases; subscribe to breaking-change RFCs |
| T3 | **macOS private API removal** — `macos-private-api` feature for NSPanel could break in future macOS | Low | High (overlay unusable on macOS) | Native Swift overlay is independent of Tauri; can fall back to pure AppKit NSPanel (already implemented) | Test on every macOS beta; monitor Apple developer forums |
| T4 | **Deepgram Nova-3 WebSocket protocol change** — Persistent WS relies on specific framing | Low | Medium (STT breaks) | AssemblyAI Universal-Streaming as fallback; abstract behind SttProvider trait | Pin Deepgram SDK version; integration test against live API weekly |
| T5 | **sqlite-vec instability** — sqlite-vec is a community extension, not SQLite core | Medium | Medium (RAG degraded) | Fallback to brute-force cosine similarity for small datasets (<10K chunks); consider porting to built-in FTS5 for BM25 | Monitor sqlite-vec GitHub issues; test with each SQLite upgrade |
| T6 | **o3 API changes or deprecation** — OpenAI frequently changes model availability | Medium | Medium (Think lane fallback) | Claude Opus with extended-thinking as equivalent fallback; abstract behind LLM trait | Monitor OpenAI changelog; alert on 404/deprecated responses |
| T7 | **Unix domain socket unavailable on Windows** — Windows uses named pipes, not Unix sockets | Certain | Low (known, planned) | Use named pipes on Windows (`\\.\pipe\bluey-overlay`); abstract behind IPC trait | N/A — design accounts for this from day 1 |
| T8 | **ONNX runtime compatibility** — ECAPA-TDNN model for speaker ID requires ONNX runtime | Low | Low (speaker ID is optional) | Ship without speaker ID initially; add when ONNX runtime stabilizes for ARM64 | Test on both Intel and ARM macOS |
| T9 | **Prompt caching invalidation** — Anthropic cache has TTL, may not hit as expected | Medium | Low (cost higher, not broken) | Monitor cache hit rates via API response headers; adjust breakpoint placement | Track `cache_creation_input_tokens` vs `cache_read_input_tokens` in cost_log |
| T10 | **Audio permission regression** — macOS TCC changes could break ScreenCaptureKit access | Low | High (no system audio) | Detect permission state on startup; guide user to System Settings; microphone-only fallback | Test on every macOS beta release |

### 5.2 Product Risks

| # | Risk | Likelihood | Impact | Mitigation | Early Warning |
|---|------|-----------|--------|------------|---------------|
| P1 | **Competitor adds three-lane routing** — Cluely or others copy the architecture | Low | Medium (reduced differentiation) | Ship fast; build moat via prompt quality + skill templates + patch-mode (harder to copy) | Monitor competitor changelogs and GitHub repos |
| P2 | **Cluely adds patch-mode** — Full-replace is their current weakness | Low | Low (bluey still faster) | Patch-mode + three-lane combined is the moat, not either alone | Monitor Cluely updates |
| P3 | **Interview platforms add AI detection** — Proctoring tools detect AI copilots | Medium | High (product value drops) | Content protection already handles screen recording; process masquerading handles task manager; stay ahead of detection methods | Monitor proctoring tool updates (HackerRank, Codility, etc.) |
| P4 | **LLM quality plateau** — Models stop improving, competitors catch up on quality | Low | Medium | Quality comes from routing + prompts + context, not just model; skill templates are the differentiator | Track eval suite scores over time |
| P5 | **User expects local inference** — Privacy-conscious users want no cloud | Medium | Low (design choice) | BYOK tier addresses privacy (user's own keys); document why cloud-first is better for quality | User feedback surveys; feature request tracking |

### 5.3 Legal/Policy Risks

| # | Risk | Likelihood | Impact | Mitigation | Early Warning |
|---|------|-----------|--------|------------|---------------|
| L1 | **Provider ToS violation** — Using AI for interview cheating may violate provider ToS | Medium | High (API access revoked) | Use BYOK tier (user's responsibility); don't market as "cheating tool"; position as "AI copilot for conversations" | Review provider ToS quarterly; use multiple providers to avoid single point of failure |
| L2 | **Recording consent laws** — Two-party consent states/countries | Medium | Medium (legal liability) | Document that user is responsible for consent; add disclaimer in onboarding; don't store audio (only transcripts) | Legal review before launch in each jurisdiction |
| L3 | **GDPR/privacy compliance** — Storing transcripts of conversations | Low | Medium | All data local by default; no cloud sync without explicit opt-in; data deletion via `bluey reset`; no PII in telemetry | Privacy audit before any cloud features |
| L4 | **Open source license conflicts** — GPL-3.0 dependencies may conflict | Low | Low | Audit all dependencies for license compatibility; prefer MIT/Apache-2.0; isolate GPL code if any | `cargo deny check licenses` in CI |

### 5.4 Operational Risks

| # | Risk | Likelihood | Impact | Mitigation | Early Warning |
|---|------|-----------|--------|------------|---------------|
| O1 | **Single-engineer dependency** — One person builds entire system | High | High (bus factor = 1) | Comprehensive documentation (this plan); CLAUDE.md for AI-assisted development; modular architecture allows parallel work | If engineer is unavailable >1 week, project stalls |
| O2 | **API key leakage** — User's keys exposed via logs, crash dumps, or memory | Low | High (financial loss) | zeroize crate for key scrubbing; log masking; keychain storage (not plaintext); never log keys even at trace level | Automated grep for key patterns in logs |
| O3 | **Runaway API costs** — Bug causes infinite loop of LLM calls | Low | High (unexpected bills) | Per-session hard cap ($10); per-minute rate limits; token budget per lane; circuit breaker on 10 consecutive errors | cost_log alerts when session exceeds $5 |
| O4 | **Build reproducibility** — Rust + native code + Tauri + React = complex build | Medium | Medium (can't ship) | Pin all dependency versions; document build steps; CI builds on every commit; `scripts/build-*.sh` tested regularly | Build breaks in CI |
| O5 | **Provider API key rotation** — Keys expire or get rotated | Low | Low (temporary outage) | Dashboard shows provider health status; `testConnection` command validates keys; alert on auth failures | 401 responses in provider health monitoring |

### 5.5 Risk Heat Map

```
              LOW IMPACT          MEDIUM IMPACT        HIGH IMPACT
           ┌─────────────────┬─────────────────────┬──────────────────┐
HIGH       │                 │ O1 (bus factor)     │                  │
LIKELIHOOD │                 │ O4 (build complex)  │                  │
           ├─────────────────┼─────────────────────┼──────────────────┤
MEDIUM     │ P5 (local inf)  │ T1 (Cerebras)       │ P3 (AI detect)   │
           │ T9 (cache miss) │ T5 (sqlite-vec)     │ L1 (ToS)         │
           │                 │ T6 (o3 changes)     │                  │
           ├─────────────────┼─────────────────────┼──────────────────┤
LOW        │ T8 (ONNX)       │ T2 (Tauri)          │ T3 (private API) │
           │ L4 (licenses)   │ T4 (Deepgram)       │ T10 (audio perm) │
           │ O5 (key rotate) │ P1 (competitor)     │ O2 (key leak)    │
           │                 │ L2 (consent)        │ O3 (runaway cost)│
           └─────────────────┴─────────────────────┴──────────────────┘
```

**Top 3 risks requiring immediate attention:**
1. **O1 (bus factor)** — Mitigate with documentation + AI-assisted dev workflow
2. **P3 (AI detection)** — Ensure stealth features are robust before launch
3. **T1 (Cerebras stability)** — Validate Groq fallback path early in Phase 9


### 3.9 Cost Optimization Implementation Details

#### CO1: Anthropic Prompt Caching — Implementation Spec

```json
// Request structure with cache_control breakpoints
{
  "model": "claude-sonnet-4-5-20260301",
  "max_tokens": 4096,
  "system": [
    {
      "type": "text",
      "text": "<core_identity>...</core_identity><anti_chatbot>...</anti_chatbot>",
      "cache_control": { "type": "ephemeral" }
    },
    {
      "type": "text",
      "text": "<skill_template name='coding_interview'>...</skill_template>",
      "cache_control": { "type": "ephemeral" }
    }
  ],
  "messages": [
    { "role": "user", "content": "..." }
  ]
}
```

**Cache economics:**
- System prompt: ~4,000 tokens (stable across all turns in a session)
- Skill template: ~1,500 tokens (stable within a skill)
- Cache write cost: 25% premium on first request
- Cache read cost: 90% discount on subsequent requests
- Cache TTL: 5 minutes (refreshed on each use)
- Break-even: 2nd request in same session (always profitable for sessions with 2+ Solve queries)

**Expected savings per session (15 Solve queries):**
- Without caching: 15 × 5,500 tokens × $3.00/M = $0.248 input cost
- With caching: 1 × 5,500 × $3.75/M + 14 × 5,500 × $0.30/M = $0.044 input cost
- **Savings: $0.204/session (82% reduction on cached portion)**

#### CO2: RAG-Filtered Context — Implementation Spec

Instead of sending full conversation history (avg 16K tokens) to Solve lane:
1. Compute query embedding (text-embedding-3-small, <5ms cached)
2. Search `chunks` table for top-5 relevant turns (sqlite-vec ANN, <10ms)
3. Include only those 5 turns + current question (avg 6K tokens)
4. Savings: 10K tokens × $3.00/M × 15 queries = $0.45/session

**Quality safeguard**: If RAG retrieval confidence < 0.6 for all chunks, fall back to last-10-turns strategy (ensures no context starvation).

#### CO3: Intra-Solve Router Split — Implementation Spec

Within the Solve lane, further classify:
- **Easy-Solve** (definitions, simple explanations, syntax questions): Route to Cerebras DeepSeek V3
  - Cost: $0.20/M input vs $3.00/M (15× cheaper)
  - Quality: Adequate for straightforward questions
  - Latency: Actually faster (Cerebras TTFT < Claude TTFT)
- **Hard-Solve** (complex coding, system design, multi-step): Route to Claude Sonnet 4.5
  - Full quality, prompt caching, skill templates

**Classification heuristic** (simple, <1ms):
```
if query_tokens < 50 AND no_code_keywords AND no_multi_step_indicators:
    route = "easy-solve" → Cerebras
else:
    route = "hard-solve" → Claude
```

Expected split: 30% easy / 70% hard
Savings: 30% × 15 queries × (12K×$2.80/M) = $0.14/session

### 3.10 Monthly Cost Projections by User Segment

| User Segment | Sessions/Month | Avg Cost/Session | Monthly Cost | Revenue (Pro) | Net |
|-------------|---------------|-----------------|--------------|---------------|-----|
| Light (casual prep) | 5 | $1.20 | $6.00 | $29 | +$23 |
| Medium (active job search) | 15 | $1.50 | $22.50 | $29 | +$6.50 |
| Heavy (daily practice) | 40 | $1.80 | $72.00 | $29 | -$43 |
| Power (BYOK) | 40 | $0.25 (STT only) | $10.00 | $9 | -$1 |

**Key insight**: The Pro tier is only viable if most users are light-to-medium. Heavy users MUST be migrated to BYOK tier or capped. Recommended: soft cap at 30 sessions/month on Pro, suggest BYOK upgrade above that.

### 4.9 Automated Eval Pipeline

#### LeetCode Evaluation Automation

```
┌──────────────┐     ┌──────────────┐     ┌──────────────┐
│  Question    │     │   bluey      │     │  LeetCode    │
│  Bank (130)  │────▶│  Pipeline    │────▶│  Submission  │
│  JSON format │     │  (simulate)  │     │  API         │
└──────────────┘     └──────────────┘     └──────┬───────┘
                                                  │
                                           ┌──────▼───────┐
                                           │   Results    │
                                           │  Aggregator  │
                                           │  (pass/fail) │
                                           └──────┬───────┘
                                                  │
                                           ┌──────▼───────┐
                                           │  Report      │
                                           │  Generator   │
                                           │  (markdown)  │
                                           └──────────────┘
```

**Pipeline steps:**
1. Load question from bank (problem statement + constraints + examples)
2. Simulate STT input (feed problem text as if transcribed)
3. Route through intent classifier → appropriate lane
4. Capture LLM response (code solution)
5. Extract code block from response
6. Submit to LeetCode API (or local judge with test cases)
7. Record: pass/fail, runtime percentile, memory percentile
8. Aggregate across difficulty levels

**Question bank format:**
```json
{
  "id": "two-sum",
  "difficulty": "easy",
  "expected_lane": "solve",
  "problem": "Given an array of integers nums and an integer target...",
  "test_cases": [...],
  "time_limit_ms": 5000,
  "tags": ["array", "hash-table"]
}
```

**Run frequency**: Full suite before each phase ships (~2 hours with rate limiting).
**Regression threshold**: >5% drop in pass rate on any difficulty level blocks the release.

#### Intent Classifier Evaluation

```
Test set: 500 labeled queries
├── 200 Snap-appropriate (simple Q&A, definitions, conversational)
├── 200 Solve-appropriate (coding, technical, multi-step)
└── 100 Think-appropriate (hard reasoning, architecture, novel problems)

Metrics computed:
├── Overall accuracy (target: ≥85%)
├── Per-lane precision and recall
├── Confusion matrix (which lanes get confused)
├── Latency distribution (must be <5ms p95)
└── Confidence calibration (predicted confidence vs actual correctness)
```

**Run frequency**: On every commit that touches router code.
**Data source**: Hand-labeled by principal engineer from real interview transcripts.

### 5.6 Risk Interdependencies

```
T1 (Cerebras instability) ──────► O3 (runaway costs if fallback is expensive)
                                    │
T3 (macOS private API) ─────────► P3 (AI detection if overlay breaks)
                                    │
O1 (bus factor) ────────────────► ALL RISKS (slower mitigation response)
                                    │
L1 (provider ToS) ─────────────► P5 (user trust if keys revoked)
                                    │
T5 (sqlite-vec) ───────────────► CO2 (RAG filter unavailable, costs rise)
```

**Cascading failure scenario (worst case):**
1. Cerebras goes down (T1)
2. All Snap queries route to Groq fallback (slightly slower, acceptable)
3. Groq also rate-limited due to surge (T1 cascade)
4. Snap queries escalate to Solve lane (Claude)
5. Claude costs spike 15× for those queries (O3 triggered)
6. Session cost cap ($10) hit within 30 minutes
7. User gets degraded service (Snap-only with delays)

**Mitigation**: Circuit breaker pattern — if fallback chain exhaustion detected, immediately switch to "Snap-only degraded mode" rather than escalating costs.

## S6: Provider Strategy

### 6.1 Cerebras — Snap Lane Primary

**Role**: Ultra-fast conversational responses (80-300ms TTFT)
**Model**: DeepSeek V3 (hosted on Cerebras inference hardware)
**Why chosen**: Fastest inference available for open-weight models; 10-50× faster than standard GPU inference

| Attribute | Detail |
|-----------|--------|
| SLA | 99.9% uptime (published) |
| Pricing | $0.20/M input, $0.60/M output |
| Rate limit | 1000 RPM (standard tier) |
| Max context | 64K tokens |
| Streaming | SSE, first token 80-150ms |
| Fallback trigger | 3 consecutive failures OR p95 > 500ms OR 429 response |
| Fallback target | Groq (Llama 4 Scout) — similar latency class |

**Integration details:**
- OpenAI-compatible API (`/v1/chat/completions`)
- HTTP/2 keep-alive pool (prewarmed, task L2)
- Dedicated connection in pool (not shared with other providers)
- Token budget: 2K input / 1K output (enforced by prompt composition)

**Monitoring:**
- Track TTFT per request in `cost_log`
- Alert if p95 TTFT > 500ms (3× normal)
- Alert if error rate > 5% in 5-minute window
- Dashboard shows Cerebras health indicator (green/yellow/red)

**Dedicated capacity path:**
- At >$5K/month spend: negotiate dedicated inference cluster
- Expected benefit: guaranteed latency SLA, 20-30% discount
- Timeline: when reaching 1,000+ active users

### 6.2 Claude Sonnet 4.5 — Solve Lane Primary

**Role**: Technical problem-solving, coding, system design (500ms-4s streaming)
**Model**: Claude Sonnet 4.5 (Anthropic)
**Why chosen**: Best coding performance per dollar; prompt caching reduces cost 50-70%

| Attribute | Detail |
|-----------|--------|
| SLA | 99.5% uptime (Anthropic published) |
| Pricing | $3.00/M input, $15.00/M output, $0.30/M cached input |
| Rate limit | 50 RPM (standard), 4000 RPM (scale tier) |
| Max context | 200K tokens |
| Streaming | SSE, first token 400-800ms |
| Prompt caching | ✅ cache_control breakpoints on system prompt + skill template |
| Fallback trigger | 3 consecutive failures OR 429 OR p95 > 3s |
| Fallback target | Gemini 2.5 Pro (similar quality, different provider) |

**Prompt caching strategy (CO1):**
```
┌─────────────────────────────────────────┐
│ System prompt (stable, cached)          │  ← cache_control: ephemeral
│ • Core identity + anti-chatbot rules    │
│ • Skill template for current mode       │
│ • Context prioritization rules          │
├─────────────────────────────────────────┤
│ Conversation context (varies per turn)  │  ← NOT cached
│ • RAG-retrieved relevant turns          │
│ • Current transcript segment            │
│ • User's question                       │
└─────────────────────────────────────────┘

Cache hit rate target: 60-70% (system prompt is ~4K tokens, reused across turns)
Savings: 60% × 4K tokens × $2.70 savings/M = $0.0065 per query
Over 15 queries/session: ~$0.10 savings/session
```

**Monitoring:**
- Track `cache_creation_input_tokens` vs `cache_read_input_tokens` from response headers
- Alert if cache hit rate drops below 40% (indicates prompt instability)
- Track streaming throughput (tokens/sec) — alert if < 30 tok/s

### 6.3 o3 — Think Lane Primary

**Role**: Hardest problems requiring extended reasoning (15-60s)
**Model**: o3 (OpenAI)
**Why chosen**: Best reasoning capability for complex multi-step problems

| Attribute | Detail |
|-----------|--------|
| SLA | 99.5% uptime (OpenAI published) |
| Pricing | $10.00/M input, $40.00/M output |
| Rate limit | 100 RPM (tier 5) |
| Max context | 200K tokens |
| Streaming | SSE with thinking tokens visible |
| Thinking budget | 32K thinking tokens max (configurable) |
| Fallback trigger | 90s timeout OR 3 consecutive failures OR 429 |
| Fallback target | Claude Opus 4 with extended-thinking enabled |

**Cost controls:**
- Hard cap: 32K thinking tokens per query ($1.28 max per query)
- Session cap: 5 Think queries per session (user can override)
- Progressive disclosure: show thinking tokens in overlay (user sees progress)
- Cancel semantics: user can cancel mid-thinking (partial result discarded)

**Monitoring:**
- Track thinking_tokens consumed per query
- Alert if average thinking_tokens > 20K (cost creep)
- Track quality: does more thinking correlate with better answers?
- Dashboard shows Think lane usage and cost prominently

### 6.4 Deepgram Nova-3 — STT Primary

**Role**: Real-time speech-to-text (persistent WebSocket per session)
**Model**: Nova-3 (Deepgram)
**Why chosen**: Lowest latency streaming STT; best accuracy for technical vocabulary

| Attribute | Detail |
|-----------|--------|
| SLA | 99.9% uptime |
| Pricing | $0.0043/sec of audio processed |
| Streaming | WebSocket, partial results every 100-200ms |
| Languages | 30+ (auto-detect available) |
| Features | Smart formatting, punctuation, diarization |
| Fallback trigger | WebSocket close + 3 reconnect failures |
| Fallback target | AssemblyAI Universal-Streaming |

**Persistent WebSocket lifecycle (L1):**
```
Session start
    │
    ▼
┌──────────────┐
│ Open WS to   │
│ Deepgram     │
│ (keep-alive) │
└──────┬───────┘
       │
       ▼
┌──────────────┐     audio frames      ┌──────────────┐
│ Send audio   │────────────────────────▶│ Receive      │
│ continuously │                         │ partials +   │
│ (16kHz PCM)  │◀────────────────────────│ finals       │
└──────────────┘     transcripts        └──────────────┘
       │
       │ Session end OR user pauses
       ▼
┌──────────────┐
│ Send         │
│ CloseStream  │
│ (graceful)   │
└──────────────┘

NEVER close mid-utterance.
On network drop: buffer audio locally, reconnect, replay buffer.
Reconnect backoff: 1s, 2s, 4s, 8s, 15s, 30s (cap)
```

**Monitoring:**
- Track partial latency (audio_sent → partial_received)
- Alert if partial latency > 500ms (network issue)
- Track word error rate via periodic spot-checks
- Monitor WebSocket connection duration (should be session-length)

### 6.5 Fallback Provider Details

| Primary | Fallback | Trigger | Switch Time | Quality Delta |
|---------|----------|---------|-------------|---------------|
| Cerebras (Snap) | Groq Llama 4 Scout | 3 failures / p95>500ms | <100ms (prewarmed pool) | -5% quality, similar speed |
| Claude Sonnet 4.5 (Solve) | Gemini 2.5 Pro | 3 failures / 429 / p95>3s | <200ms | -10% coding quality, similar speed |
| o3 (Think) | Claude Opus 4 | 90s timeout / 3 failures | <500ms | -5% reasoning, similar depth |
| Deepgram Nova-3 (STT) | AssemblyAI Universal | WS close + 3 reconnects | <3s (new WS) | -5% accuracy, +100ms latency |

### 6.6 Escalation Paths

```
Normal operation
    │
    ├─ Single failure → retry once (same provider)
    │
    ├─ 3 consecutive failures → switch to fallback
    │   └─ Emit provider-status event (yellow indicator)
    │
    ├─ Fallback also fails → try tertiary (if exists)
    │   └─ Emit provider-status event (red indicator)
    │
    ├─ All providers in chain fail → graceful degradation
    │   ├─ Snap: show "temporarily unavailable" in overlay
    │   ├─ Solve: queue request, retry in 30s
    │   ├─ Think: show error, suggest retry later
    │   └─ STT: switch to offline mode (no transcription)
    │
    └─ Provider recovers → automatic switch back after 60s healthy
        └─ Emit provider-status event (green indicator)
```

### 6.7 Provider Degradation Detection

| Signal | Detection Method | Action |
|--------|-----------------|--------|
| Latency spike | p95 > 2× baseline over 5-min window | Log warning, prepare fallback |
| Error rate spike | >5% errors in 5-min window | Switch to fallback |
| Quality degradation | Garbled/truncated responses | Log + alert, manual investigation |
| Rate limiting | 429 responses | Respect Retry-After header, queue requests |
| Maintenance window | Provider status page API | Pre-switch to fallback before window |

### 6.8 Contract Considerations

| Provider | Current Tier | Upgrade Trigger | Upgrade Benefit |
|----------|-------------|-----------------|-----------------|
| Cerebras | Standard | $5K/month spend | Dedicated cluster, guaranteed latency |
| Anthropic | Scale | $10K/month spend | Higher rate limits (4000 RPM), priority support |
| OpenAI | Tier 5 | $10K/month spend | Higher rate limits, dedicated capacity |
| Deepgram | Growth | $3K/month spend | Volume discount (15-25%), priority support |

**Timeline**: Negotiate upgrades when reaching 1,000+ paying users (estimated month 6-9 post-launch).

### 6.9 Provider Comparison: Why These Specific Models

| Lane | Chosen | Runner-up | Why Chosen Over Runner-up |
|------|--------|-----------|---------------------------|
| Snap | Cerebras DeepSeek V3 | Groq Llama 4 Scout | Cerebras 2× faster TTFT; DeepSeek V3 better at code than Llama |
| Solve | Claude Sonnet 4.5 | GPT-4o | Claude better at coding tasks; prompt caching saves 50-70%; 200K context |
| Think | o3 | Claude Opus 4 | o3 reasoning significantly better on hard problems; visible thinking tokens |
| STT | Deepgram Nova-3 | AssemblyAI Universal | Deepgram lower latency partials; better technical vocabulary; persistent WS |

## S7: Glossary + Reference Index

### 7.1 Acronyms + Terms

| Term | Definition |
|------|-----------|
| ANN | Approximate Nearest Neighbor — vector search algorithm used by sqlite-vec |
| BM25 | Best Matching 25 — probabilistic text retrieval algorithm (hybrid search with vectors) |
| BYOK | Bring Your Own Key — pricing tier where user provides their own API keys |
| CO1/CO2/CO3 | Cost Optimization tasks 1-3 (prompt caching, RAG filter, router split) |
| CM1/CM2/CM3/CM4 | Context Management tasks 1-4 (session model, assembly, epochs, counter) |
| CPAL | Cross-Platform Audio Library — Rust crate for microphone capture |
| CSP | Content Security Policy — restricts what Tauri webview can load |
| DSP | Digital Signal Processing — audio processing pipeline |
| ECAPA-TDNN | Emphasized Channel Attention, Propagation and Aggregation Time Delay Neural Network — speaker ID model |
| F1-F8 | Follow-up/Patch-mode tasks (three-lane integration through diff rendering) |
| FTS5 | Full-Text Search 5 — SQLite extension for text search |
| HTTF | Time To First Token — latency from request sent to first response token |
| IPC | Inter-Process Communication — daemon↔overlay, daemon↔dashboard, daemon↔CLI |
| L1-L6 | Latency Engineering tasks (persistent WS, HTTP/2 pool, stable-partial, speculative, socket, instrumentation) |
| LA2 | LocalAgreement-2 — streaming STT decoder algorithm (stretch goal) |
| LLM | Large Language Model — AI text generation (Cerebras, Claude, o3) |
| NDJSON | Newline-Delimited JSON — streaming format used by some providers |
| NSPanel | macOS AppKit panel type — floats above windows, non-activating |
| ONNX | Open Neural Network Exchange — model format for speaker ID |
| PCM | Pulse-Code Modulation — raw audio sample format (f32 or i16) |
| R1-R5 | Three-lane Routing tasks (classifier, parallel, override, skills, badge) |
| RAG | Retrieval-Augmented Generation — local vector search to enhance LLM context |
| rAF | requestAnimationFrame — browser API for 60Hz rendering |
| RPM | Requests Per Minute — API rate limit unit |
| RSS | Resident Set Size — process memory usage metric |
| SCK | ScreenCaptureKit — macOS framework for system audio capture |
| SLA | Service Level Agreement — provider uptime guarantee |
| SPSC | Single-Producer Single-Consumer — lock-free ring buffer pattern |
| SSE | Server-Sent Events — HTTP streaming protocol for LLM responses |
| STT | Speech-to-Text — audio transcription (Deepgram Nova-3) |
| TCC | Transparency, Consent, and Control — macOS permission framework |
| TTFT | Time To First Token — key latency metric for LLM responses |
| VAD | Voice Activity Detection — filters silence before sending to STT |
| WAL | Write-Ahead Logging — SQLite journaling mode for concurrent reads |
| WASAPI | Windows Audio Session API — Windows system audio capture |
| WS | WebSocket — persistent bidirectional connection (STT, some LLM providers) |

### 7.2 Task ID Format

| Prefix | Series | Meaning |
|--------|--------|---------|
| D0.x | Foundation | Daemon + Tauri scaffold tasks |
| C0.x | Foundation | Core data model tasks |
| N0.x | Foundation | Native overlay audit tasks |
| B1.x | Stealth | Window stealth + masquerading |
| B2.x | Audio/STT | Audio capture + speech-to-text |
| B3.x | LLM | LLM orchestration + providers |
| B4.x | Prompts | Prompt composition + skills |
| B5.x | RAG | Vector store + retrieval |
| B6.x | Dashboard | UI + UX |
| B7.x | Security | Keychain, logs, migrations |
| B8.x | Observability | Metrics, telemetry, dashboards |
| B9.x | Release | Auto-updater, build targets |
| B10.x | Dev | Documentation, codex configs |
| L1-L6 | Latency | Latency engineering (V3 new) |
| R1-R5 | Routing | Three-lane routing (V3 new) |
| F1-F8 | Follow-up | Patch-mode follow-ups (V3 new) |
| CM1-CM4 | Context | Context management (V3 new) |
| CO1-CO3 | Cost | Cost optimization (V3 new) |

### 7.3 Document Index (on uno)

All files at `/Users/uno/Downloads/cue/docs/reviews/`:

| File | Lines | Content |
|------|-------|---------|
| `CUE-BLUEY-V3-PLAN-SKELETON.md` | 734 | V3 master plan skeleton — architecture, task index, phases, timeline |
| `CUE-BLUEY-V3-PART-A-PHASES.md` | ~1500 | Detailed phase plans 0-4 (foundation through reasoning) |
| `CUE-BLUEY-V3-PART-B-PHASES.md` | ~1200 | Detailed phase plans 5-10 (memory through cost optimization) |
| `CUE-BLUEY-V3-PART-C-APPENDICES.md` | ~2800 | THIS FILE — architecture, competitive, cost, metrics, risks, providers, glossary |
| `CUE-MASTER-PORT-PLAN-V2.md` | 1895 | Previous V2 plan with 97 tasks (historical reference) |
| `CUE-CURRENT-STATE.md` | 486 | Source code audit: what's built vs planned |
| `CUE-REFERENCE-ANALYSIS.md` | 2000+ | Running pattern catalog from all reference repos |
| `CUE-PORT-PLAN.md` | — | Ranked backlog for codex (derived from reference analysis) |
| `CUE-DESIGN-01-STEALTH-WINDOWS.md` | 1300 | Stealth + Windows design synthesis (B1.x tasks) |
| `CUE-DESIGN-02-AUDIO-STT.md` | 1648 | Audio + STT design synthesis (B2.x tasks) |
| `CUE-DESIGN-03-LLM-PROMPTS-RAG.md` | 1791 | LLM + Prompts + RAG design synthesis (B3-B5 tasks) |
| `CUE-DESIGN-04-UX-OPS-DEV.md` | 1486 | UX + Ops + Security + Dev design synthesis (B6-B10 tasks) |
| `CUE-REF-01A-NATIVELY-BACKEND.md` | ~3000 | Deep analysis: natively-cluely backend (Electron main + Rust native) |
| `CUE-REF-01B-NATIVELY-FRONTEND.md` | ~1500 | Deep analysis: natively-cluely frontend (React renderer) |
| `CUE-REF-02-PLUELY.md` | ~2000 | Deep analysis: pluely (Tauri 2 — closest stack match) |
| `CUE-REF-03-SOLVEWATCHAI.md` | ~1800 | Deep analysis: solveWatchAi (Python + Electron, local Whisper) |
| `CUE-REF-04-AURA.md` | ~1200 | Deep analysis: Aura-AI (Python, multi-provider) |
| `CUE-REF-05-VYSPER.md` | ~1000 | Deep analysis: Vysper (9 skill templates, Azure) |
| `CUE-REF-06-OPENCLUELY.md` | ~800 | Deep analysis: OpenCluely (Gemini Vision, language enforcement) |
| `PINKY-ARCHITECTURE-FROM-CODE.md` | ~1500 | Pinky (Go SaaS) architecture — WebRTC, billing, teams |

### 7.4 External References

| Resource | URL | Relevance |
|----------|-----|-----------|
| Tauri 2 Docs | https://v2.tauri.app | Dashboard framework |
| tauri-nspanel | https://github.com/nicepkg/tauri-nspanel | NSPanel integration for Tauri |
| Cerebras API | https://cloud.cerebras.ai/docs | Snap lane provider |
| Anthropic API | https://docs.anthropic.com | Solve lane provider |
| Anthropic Prompt Caching | https://docs.anthropic.com/en/docs/build-with-claude/prompt-caching | CO1 implementation |
| OpenAI API | https://platform.openai.com/docs | Think lane provider |
| Deepgram Streaming | https://developers.deepgram.com/docs/streaming | STT integration |
| AssemblyAI Streaming | https://www.assemblyai.com/docs/streaming | STT fallback |
| sqlite-vec | https://github.com/asg017/sqlite-vec | RAG vector store |
| CPAL | https://docs.rs/cpal | Microphone capture |
| cidre | https://docs.rs/cidre | macOS CoreAudio/SCK bindings |
| ringbuf | https://docs.rs/ringbuf | Lock-free audio ring buffer |
| rubato | https://docs.rs/rubato | Sample rate conversion |
| governor | https://docs.rs/governor | Rate limiting |
| tokio-tungstenite | https://docs.rs/tokio-tungstenite | WebSocket client (Deepgram) |
| reqwest | https://docs.rs/reqwest | HTTP/2 client (LLM providers) |
| tracing | https://docs.rs/tracing | Structured logging |
| opentelemetry-rust | https://docs.rs/opentelemetry | Metrics + telemetry |
| criterion | https://docs.rs/criterion | Benchmarking framework |
| zeroize | https://docs.rs/zeroize | Secure key scrubbing |

---

*End of Appendices. This document is consumed by codex agents during implementation as reference material.*
*Generated: 2026-05-12 | Author: Principal Engineer review of V3 architecture*

### 6.10 Provider Migration Playbook

If a primary provider becomes unviable (pricing change, quality degradation, ToS enforcement):

| Current | Migration Target | Effort | Data Migration |
|---------|-----------------|--------|----------------|
| Cerebras → Groq | Groq Llama 4 Scout | 1 day | Config change only (OpenAI-compatible API) |
| Cerebras → Fireworks | Fireworks DeepSeek V3 | 2 days | New client config, same API shape |
| Claude Sonnet → Gemini 2.5 Pro | Google AI | 3 days | Prompt adaptation (XML→plain), cache strategy change |
| Claude Sonnet → GPT-4o | OpenAI | 2 days | Prompt adaptation, no cache benefit |
| o3 → Claude Opus | Anthropic | 1 day | Config change, enable extended-thinking |
| Deepgram → AssemblyAI | AssemblyAI | 3 days | New WS protocol, similar accuracy |
| Deepgram → Groq Whisper | Groq | 2 days | REST-batch (latency regression), cheaper |

**Key principle**: All providers are behind traits/interfaces. Migration is config + prompt adaptation, never architectural change. This is by design (CUE-DESIGN-03 provider abstraction).

### 7.5 Version History

| Version | Date | Author | Changes |
|---------|------|--------|---------|
| V3.0 | 2026-05-12 | Principal Engineer | Initial appendices (S1-S7) |

