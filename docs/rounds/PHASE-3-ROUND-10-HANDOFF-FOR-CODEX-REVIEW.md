# Phase 3 Round 10 — Handoff for Codex Review

**Branch**: `feat/phase-3-round-10`
**Base**: `feat/phase-3-round-9` tip (`115ac8d`, 284 tests)
**Authors**: kiro (parallel subagents with worktree isolation), uno (user — oversight)

## Scope

Round 10 of Phase 3. Four themes: AI hookup completion (the cue feature actually firing), streaming LLM responses end-to-end, security hardening basics, and real whisper.cpp on macOS. This round delivers the first real user-visible AI experience and the first security hardening pass.

### Commits (12 ahead of R9)

```
5943e5a chore(p3r10): cargo fmt + sync codex docs after cherry-picks
010a884 chore(whisper): Windows helper documents model env + defers real impl [P3.R10]
bf7a351 feat(whisper): real whisper.cpp transcription on macOS via SwiftPM [P3.R10]
6b04f3c feat(security): obfstr for API endpoints + auth header names across providers [P3.R10]
a00d0b6 feat(stealth): anti-debug helpers (PT_DENY_ATTACH macOS / IsDebuggerPresent Windows / TracerPid Linux) [P3.R10]
849257e feat(dashboard): Responses route renders streaming cue_response_chunk events [P3.R10]
2cd7ca2 feat(llm): Anthropic/OpenAI/Ollama streaming SSE/NDJSON impls + tests [P3.R10]
0aa3962 feat(llm): trait gains complete_stream + LlmChunk; default falls back to complete [P3.R10]
4bcbeb8 feat(daemon): Cmd+Shift+A hotkey triggers AnswerLLM via question-detect [P3.R10]
32528dc feat(daemon): auto-recap on session end persists + emits cue_response [P3.R10]
94bf079 test(daemon): whisper-stub end-to-end factory test [P3.R10]
75e5df7 fix(dashboard): live transcript merges by {session_id, index} keys [P3.R10]
```

## Verification — ALL GREEN

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 299 pass, 10 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check feat/phase-3-round-9..HEAD       ✅ clean
strings target/release/cue-dashboard | grep -c "wss://api.deepgram.com"   ✅ 0 matches
```

### Test count delta

| Tier | R9 final | R10 final | Δ |
|------|----------|-----------|---|
| Streaming LLM (3 providers × 3 tests) | 0 | 9 | +9 |
| Auto-recap integration | 0 | 2 | +2 |
| Whisper-stub e2e factory (ignored) | 0 | 2 | +2 |
| Hotkey + request_cue | 0 | 2 | +2 |
| Previous (carried forward) | 284 | 284 | — |
| **Total running** | **284** | **299** | **+15** |

## Architecture Diagram — Round 10 Additions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Streaming LLM End-to-End [NEW]                             │
│                                                                             │
│  LlmProvider::complete_stream(&LlmRequest) → LlmChunkStream                │
│       │                                                                     │
│       ├── AnthropicProvider: SSE /v1/messages                               │
│       │     event: content_block_delta → LlmChunk{text, finished:false}     │
│       │     event: message_stop → LlmChunk{text:"", finished:true}          │
│       │                                                                     │
│       ├── OpenAiProvider: SSE /v1/chat/completions                          │
│       │     data: {choices[0].delta.content} → LlmChunk                    │
│       │     data: [DONE] → LlmChunk{finished:true}                         │
│       │                                                                     │
│       └── OllamaProvider: NDJSON /api/chat                                  │
│             {message:{content}, done:false} → LlmChunk                     │
│             {done:true} → LlmChunk{finished:true}                          │
│                                                                             │
│  Default impl: complete() → single LlmChunk{text, finished:true}           │
│  Router: failover on first-chunk Auth/Quota error                           │
│                                                                             │
│  Dashboard Responses route:                                                 │
│    listen(cue_response_chunk) → Map<id, accumulated> → typing indicator    │
│    listen(cue_response) → final card render                                │
│    [NOTE: daemon→dashboard chunk emission deferred]                         │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                    Anti-Debug Install Path [NEW]                              │
│                                                                             │
│  cue-dashboard/src/lib.rs (Tauri setup):                                    │
│    cue_stealth::install_anti_debug()  ← non-fatal on error                 │
│         │                                                                   │
│         ├── macOS: ptrace(PT_DENY_ATTACH, 0, 0, 0)                         │
│         │          + sysctl CTL_KERN/KERN_PROC/KERN_PROC_PID → P_TRACED    │
│         │                                                                   │
│         ├── Windows: IsDebuggerPresent() + CheckRemoteDebuggerPresent()     │
│         │            + 5s watchdog thread (log::warn only, no kill)         │
│         │                                                                   │
│         └── Linux: read /proc/self/status → TracerPid: N                   │
│                    + 5s watchdog thread (log::warn only)                    │
│                                                                             │
│  is_debugger_attached() → bool (callable from anywhere)                    │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│              whisper.cpp + SwiftWhisper on macOS [NEW]                        │
│                                                                             │
│  native/macos/cue-whisper/Package.swift:                                    │
│    .package(url: "SwiftWhisper", exact: "1.2.0")                           │
│         │                                                                   │
│         ▼                                                                   │
│  SwiftWhisper bundles whisper.cpp C source (no external .dylib)            │
│         │                                                                   │
│         ▼                                                                   │
│  main.swift:                                                                │
│    1. Load model from BLUEY_WHISPER_MODEL or ~/.cache/bluey/whisper/       │
│    2. whisper_init_from_file(modelPath) → ctx                              │
│    3. Loop: read chunkBytes (3s PCM16 LE 16kHz mono) from stdin            │
│    4. Convert Int16 → Float32 (÷ 32768.0)                                 │
│    5. RMS gate: skip if < 0.01 (silence)                                   │
│    6. whisper_full(ctx, params, samples, count)                            │
│    7. Extract segments → emit NDJSON partial/final                         │
│    8. Confidence = avg token probability per segment                       │
│                                                                             │
│  NDJSON ABI unchanged from stub:                                            │
│    {"type":"partial","text":"..."}                                          │
│    {"type":"final","text":"...","confidence":0.95}                          │
│                                                                             │
│  Model: tiny.en-q5_1.bin (31 MB quantized ggml)                           │
│  Windows: stub only (CMake + MSVC deferred)                                │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Per-Feature-Area Review Checklist

### AI Hookup: Cmd+Shift+A + Auto-Recap (`4bcbeb8`, `32528dc`, `94bf079`, `75e5df7`)

- [ ] Hotkey registration: `CmdOrCtrl+Shift+A` in Tauri setup, emits `hotkey_request_cue`
- [ ] `request_cue` command: loads last ~10 segments from active session
- [ ] Question detection: `ends_with_question()` on concatenated text → AnswerLlm dispatch
- [ ] Non-question path: dispatches to WhatToAnswerLlm
- [ ] `build_llm_provider_from_env()`: checks `OPENAI_API_KEY` env then keyring
- [ ] CueResponse persisted to `cue_responses` table with correct kind/source fields
- [ ] `cue_response` Tauri event emitted after persist
- [ ] `spawn_auto_recap()`: fire-and-forget `tokio::spawn`, doesn't block MeetingEnd
- [ ] Auto-recap: graceful skip when no provider (log warning, no panic)
- [ ] MockLlm tests verify CueResponse shape and error propagation
- [ ] Whisper-stub e2e: `CARGO_BIN_EXE_whisper-stub` env injection
- [ ] Whisper-stub e2e: factory builds chain with LocalWhisper as sole provider
- [ ] Whisper-stub e2e: verifies Partial events precede Final
- [ ] Live transcript dedup: `Map<"${session_id}:${index}", segment>` in React state
- [ ] Dedup: both catch-up `get_live_transcripts` and live event insert into same map

### Streaming LLM (`0aa3962`, `2cd7ca2`, `849257e`)

- [ ] `LlmChunk` struct: `text: String`, `finished: bool`
- [ ] `LlmChunkStream` type alias: `Pin<Box<dyn Stream<Item = Result<LlmChunk, LlmError>> + Send>>`
- [ ] Default `complete_stream()`: wraps `complete()` in `futures_util::stream::once`
- [ ] Router `complete_stream()`: tries active provider, failover on Auth/Quota
- [ ] Anthropic: `stream: true` in request body, parses SSE `event:` lines
- [ ] Anthropic: `content_block_delta` → extracts `delta.text`
- [ ] Anthropic: `message_stop` → `LlmChunk { text: "", finished: true }`
- [ ] OpenAI: `stream: true` in request body, parses `data:` lines
- [ ] OpenAI: `choices[0].delta.content` extraction (handles null/missing)
- [ ] OpenAI: `data: [DONE]` → `LlmChunk { finished: true }`
- [ ] Ollama: `stream: true` in request body, reads line-by-line NDJSON
- [ ] Ollama: `done: true` field → `LlmChunk { finished: true }`
- [ ] All 3 providers: `supports_streaming() -> true`
- [ ] Streaming tests: `streaming_yields_multiple_chunks` (3 providers)
- [ ] Streaming tests: `streaming_propagates_auth_error` (3 providers)
- [ ] Streaming tests: `streaming_finished_chunk_terminates_stream` (3 providers)
- [ ] Dashboard: subscribes to `cue_response_chunk` event
- [ ] Dashboard: `Map<response_id, accumulated_text>` for in-flight tracking
- [ ] Dashboard: typing indicator with pulsing cursor while streaming
- [ ] Dashboard: removes from inflight map on `finished:true` or final `cue_response`

### Anti-Debug (`a00d0b6`)

- [ ] `install_anti_debug()` called in Tauri setup block
- [ ] Non-fatal: `.ok()` or `if let Err(e)` with log, doesn't crash app
- [ ] macOS: `ptrace(PT_DENY_ATTACH, 0, 0, 0)` — checks return value for error
- [ ] macOS: `sysctl` with `CTL_KERN, KERN_PROC, KERN_PROC_PID, getpid()` → checks `kp_proc.p_flag & P_TRACED`
- [ ] Windows: `IsDebuggerPresent()` initial check
- [ ] Windows: `CheckRemoteDebuggerPresent()` for remote debuggers
- [ ] Windows: 5s watchdog thread spawned (daemon thread, won't prevent exit)
- [ ] Windows: watchdog logs `log::warn!` only — no process termination
- [ ] Linux: reads `/proc/self/status`, parses `TracerPid:\t{N}` line
- [ ] Linux: handles missing/malformed file gracefully (returns false, no panic)
- [ ] Linux: 5s watchdog thread spawned
- [ ] `is_debugger_attached()` public API returns `bool`

### obfstr (`6b04f3c`)

- [ ] `obfstr!()` applied to: Anthropic base URL, OpenAI base URL, Ollama base URL
- [ ] `obfstr!()` applied to: Deepgram WSS URL, OpenAI Realtime WSS URL
- [ ] `obfstr!()` applied to: `"Authorization"` header name, `"x-api-key"` header name
- [ ] `strings(1)` verification: 0 matches for `wss://api.deepgram.com`
- [ ] `strings(1)` verification: 0 matches for `wss://api.openai.com`
- [ ] `strings(1)` verification: 0 matches for `http://localhost:11434`
- [ ] `strings(1)` verification: 0 matches for `api.anthropic.com`
- [ ] Intentionally excluded: `cue-rag` module (documented, out of scope)
- [ ] API key VALUES not obfstr-wrapped (runtime-loaded, not static literals)

### whisper.cpp macOS (`bf7a351`, `010a884`)

- [ ] `Package.swift`: SwiftWhisper pinned to exact version `1.2.0`
- [ ] `Package.resolved`: lockfile committed (reproducible builds)
- [ ] Model path: `BLUEY_WHISPER_MODEL` env var → fallback `~/.cache/bluey/whisper/tiny.en-q5_1.bin`
- [ ] Exits with error + helpful message if model file missing
- [ ] `whisper_init_from_file()` → exits on failure
- [ ] PCM16 LE → Float32 conversion: `Float(sample) / 32768.0`
- [ ] RMS silence gate: threshold 0.01, skips `whisper_full()` on silence
- [ ] `whisper_full()` called with default params (language: "en", no translate)
- [ ] Segment extraction: iterates `whisper_full_n_segments()`
- [ ] Confidence: average of `whisper_full_get_token_p()` across segment tokens
- [ ] NDJSON output format unchanged: `{"type":"partial",...}` / `{"type":"final",...,"confidence":N}`
- [ ] Windows stub: checks `BLUEY_WHISPER_MODEL` / `%USERPROFILE%\.cache\bluey\whisper\`
- [ ] Windows stub: exits with helpful error if model missing
- [ ] Windows stub: still emits RMS-based placeholder NDJSON (real impl deferred)

### Reconciliation (`5943e5a`)

- [ ] Only formatting changes + doc sync — no logic changes
- [ ] Artifact of parallel subagent workflow

## Explicit Deferrals (NOT in Round 10)

1. **Daemon→dashboard `cue_response_chunk` Tauri event emission** — provider streaming wired; daemon→dashboard push deferred (TCP IPC vs Tauri event boundary).
2. **Multi-provider AnswerLLM** — only OpenAI via env/keyring today; Anthropic/Ollama deferred to LLM chain settings UI.
3. **Auto-recap Tauri event from daemon** — no `AppHandle` in daemon context; dashboard command path does emit.
4. **Windows real whisper.cpp** — CMake + MSVC toolchain.
5. **Windows anti-debug process termination** — watchdog logs only in v0.1.
6. **mlock for API key memory** — R12 hardening round.
7. **SQLCipher migration** — R12 hardening round.
8. **Tauri signing keypair** — R12 hardening round.
9. **obfstr for cue-rag** — out of scope this round.
10. **Word-level whisper.cpp streaming** — `new_segment_callback` deferred.
11. **sqlite-vec swap** — R11.
12. **Native overlay passthrough handlers** — R11.

## Verdict Request

Codex: review the 12 commits (4 themes: AI hookup, streaming LLM, hardening, whisper.cpp). Write `docs/work/REVIEW-PHASE-3-ROUND-10.md` with verdict.

- 🟢 **ACCEPT** → merge R7+R8+R9+R10 chain to main, ship v0.1 alpha
- 🟡 **ACCEPT WITH NITS** → fold nits into Round 11
- 🔴 **REQUEST CHANGES** → kiro writes fix doc and re-hands
