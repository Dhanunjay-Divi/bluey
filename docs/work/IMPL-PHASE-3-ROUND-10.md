# IMPL — Phase 3 Round 10 (AI Hookup + Streaming LLM + Hardening + Real Whisper.cpp)

**Branch**: `feat/phase-3-round-10`
**Base**: `feat/phase-3-round-9` tip (`115ac8d`, 284 tests)
**Tip**: `5943e5a` (12 commits ahead of R9, 299 tests)

## Scope

**Four themes: AI hookup completion, streaming LLM responses end-to-end, security hardening basics, and real whisper.cpp on macOS.**

Round 10 delivers the first real user-visible AI experience (Cmd+Shift+A fires AnswerLLM), streaming LLM output for responsive UX, anti-debug + string obfuscation hardening, and replaces the macOS whisper stub with actual whisper.cpp transcription. This is the last feature round before v0.1 alpha ships.

**Does:**

1. **Cmd+Shift+A → AnswerLLM**: Register CmdOrCtrl+Shift+A global shortcut, emit `hotkey_request_cue` event, `request_cue` Tauri command loads last ~10 transcript segments, dispatches to `AnswerLlm` (question detected) or `WhatToAnswerLlm` (no question), persists `CueResponse`, emits `cue_response` event.
2. **Auto-recap on session end**: `spawn_auto_recap()` from MeetingEnd IPC handler — fire-and-forget task builds LLM provider from env/keyring, runs `RecapLlm` on full transcript, persists to `cue_responses` table. Graceful skip when no provider configured.
3. **Whisper-stub e2e factory test**: Integration test using `CARGO_BIN_EXE_whisper-stub` env var to exercise the real STT factory chain with LocalWhisper as sole provider. Verifies Partial→Final ordering and expected stub output.
4. **Live transcript dedup by `{session_id, index}`**: Replace `lastSeenIndex` cursor with `Map<string, TranscriptSegment>` keyed by `${session_id}:${index}`. Both catch-up and live paths insert into same map — eliminates startup race.
5. **`LlmProvider` trait gains `complete_stream()`**: Returns `Pin<Box<dyn Stream<Item = Result<LlmChunk, LlmError>> + Send>>`. Default impl falls back to `complete()` yielding single chunk. Router forwards streaming with failover.
6. **Anthropic streaming**: SSE on `/v1/messages` with `stream:true`, parses `content_block_delta` + `message_stop` events.
7. **OpenAI streaming**: SSE on `/v1/chat/completions` with `stream:true`, parses `choices[0].delta.content` + `data: [DONE]`.
8. **Ollama streaming**: NDJSON on `/api/chat` with `stream:true`, parses line-by-line `{message:{content}, done}` objects.
9. **Dashboard Responses route streaming**: Subscribes to `cue_response_chunk` (live delta) + `cue_response` (final). Tracks in-flight responses in `Map<response_id, accumulated_text>`. Typing indicator with pulsing cursor while streaming.
10. **Anti-debug**: `install_anti_debug()` + `is_debugger_attached()` in `cue-stealth`. macOS: `ptrace(PT_DENY_ATTACH)` + sysctl `P_TRACED`. Windows: `IsDebuggerPresent` + `CheckRemoteDebuggerPresent` + 5s watchdog (log-only). Linux: `/proc/self/status` TracerPid + 5s watchdog.
11. **obfstr for API endpoints**: `obfstr!()` macro on Anthropic/OpenAI/Ollama base URLs, Deepgram/OpenAI WSS URLs, `Authorization`/`x-api-key` header names. Verified via `strings(1)` — zero matches for target URLs in release binary.
12. **Real whisper.cpp on macOS**: SwiftWhisper v1.2.0 dependency (bundles whisper.cpp source). `whisper_full()` C API for synchronous chunk processing. PCM16→float32 conversion, RMS silence gate (<0.01), per-segment confidence from token probability. Model at `~/.cache/bluey/whisper/tiny.en-q5_1.bin` (31 MB quantized).
13. **Windows whisper stub update**: Documents `BLUEY_WHISPER_MODEL` env var, exits with helpful error if model missing. Real impl deferred (CMake + MSVC).

**Does NOT:**

- Emit `cue_response_chunk` Tauri events from daemon side (provider streaming wired; daemon→dashboard event push deferred — TCP IPC vs Tauri event boundary).
- Support Anthropic/Ollama in `build_llm_provider_from_env` (only OpenAI today).
- Emit Tauri event from daemon-side auto-recap (no `AppHandle` in daemon context; dashboard `auto_recap` command does emit).
- Implement Windows real whisper.cpp (CMake + MSVC toolchain deferred).
- Terminate process on debug detection (Windows watchdog logs only).
- Apply `mlock` to API key memory or SQLCipher migration.
- Cover `cue-rag` with obfstr (out of scope — grep still finds api.openai.com/api.anthropic.com there).
- Emit word-level streaming partials from whisper.cpp (`new_segment_callback` deferred).

## Commits (12, chronological bottom → top)

| # | Hash | Title | Theme |
|---|------|-------|-------|
| 1 | `75e5df7` | `fix(dashboard): live transcript merges by {session_id, index} keys [P3.R10]` | AI hookup |
| 2 | `94bf079` | `test(daemon): whisper-stub end-to-end factory test [P3.R10]` | AI hookup |
| 3 | `32528dc` | `feat(daemon): auto-recap on session end persists + emits cue_response [P3.R10]` | AI hookup |
| 4 | `4bcbeb8` | `feat(daemon): Cmd+Shift+A hotkey triggers AnswerLLM via question-detect [P3.R10]` | AI hookup |
| 5 | `0aa3962` | `feat(llm): trait gains complete_stream + LlmChunk; default falls back to complete [P3.R10]` | Streaming LLM |
| 6 | `2cd7ca2` | `feat(llm): Anthropic/OpenAI/Ollama streaming SSE/NDJSON impls + tests [P3.R10]` | Streaming LLM |
| 7 | `849257e` | `feat(dashboard): Responses route renders streaming cue_response_chunk events [P3.R10]` | Streaming LLM |
| 8 | `a00d0b6` | `feat(stealth): anti-debug helpers (PT_DENY_ATTACH macOS / IsDebuggerPresent Windows / TracerPid Linux) [P3.R10]` | Hardening |
| 9 | `6b04f3c` | `feat(security): obfstr for API endpoints + auth header names across providers [P3.R10]` | Hardening |
| 10 | `bf7a351` | `feat(whisper): real whisper.cpp transcription on macOS via SwiftPM [P3.R10]` | Whisper.cpp |
| 11 | `010a884` | `chore(whisper): Windows helper documents model env + defers real impl [P3.R10]` | Whisper.cpp |
| 12 | `5943e5a` | `chore(p3r10): cargo fmt + sync codex docs after cherry-picks` | Reconciliation |

## Files Created / Modified

### cue-llm (streaming additions)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-llm/Cargo.toml` | Modified | Added `futures-util` dep |
| `crates/cue-llm/src/lib.rs` | Modified | `LlmChunk`, `LlmChunkStream` type, `complete_stream()` default impl |
| `crates/cue-llm/src/router.rs` | Modified | Router `complete_stream()` with failover semantics |
| `crates/cue-llm/src/anthropic.rs` | Modified | SSE streaming impl + 3 tests |
| `crates/cue-llm/src/openai.rs` | Modified | SSE streaming impl + 3 tests |
| `crates/cue-llm/src/ollama.rs` | Modified | NDJSON streaming impl + 3 tests |

### cue-daemon (AI hookup + auto-recap)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/Cargo.toml` | Modified | Added `obfstr` dep |
| `crates/cue-daemon/src/app.rs` | Modified | `spawn_auto_recap()`, MeetingEnd hook, `build_llm_provider_from_env()` |
| `crates/cue-daemon/src/llm/answer.rs` | Modified | Uses `complete_stream` when supported |
| `crates/cue-daemon/src/llm/recap.rs` | Modified | Uses `complete_stream` when supported |
| `crates/cue-daemon/src/llm/suggest.rs` | Modified | Uses `complete_stream` when supported |
| `crates/cue-daemon/src/stt/deepgram.rs` | Modified | obfstr for WSS URL |
| `crates/cue-daemon/src/stt/openai.rs` | Modified | obfstr for WSS URL |
| `crates/cue-daemon/tests/auto_recap_integration.rs` | Created | MockLlm integration tests for auto-recap |
| `crates/cue-daemon/tests/whisper_stub_e2e.rs` | Created | Factory e2e with CARGO_BIN_EXE_whisper-stub |

### cue-dashboard (hotkey + streaming UI)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-dashboard/Cargo.toml` | Modified | Added `obfstr` dep |
| `crates/cue-dashboard/src/lib.rs` | Modified | Cmd+Shift+A registration, `install_anti_debug()` call |
| `crates/cue-dashboard/src/commands.rs` | Modified | `request_cue`, `auto_recap` commands, `build_llm_provider_from_env` |
| `crates/cue-dashboard/ui/src/App.tsx` | Modified | HotkeyListener for `hotkey_request_cue` event |
| `crates/cue-dashboard/ui/src/routes/LiveTranscript.tsx` | Modified | Map-based dedup by `{session_id, index}` |
| `crates/cue-dashboard/ui/src/routes/Responses.tsx` | Modified | `cue_response_chunk` subscription, typing indicator |

### cue-stealth (anti-debug + obfstr)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-stealth/Cargo.toml` | Modified | Added `obfstr`, platform deps (libc, winapi, nix) |
| `crates/cue-stealth/src/lib.rs` | Modified | `install_anti_debug()`, `is_debugger_attached()` public API |
| `crates/cue-stealth/src/macos.rs` | Modified | PT_DENY_ATTACH + sysctl P_TRACED |
| `crates/cue-stealth/src/windows.rs` | Modified | IsDebuggerPresent + CheckRemoteDebuggerPresent + watchdog |
| `crates/cue-stealth/src/linux.rs` | Modified | /proc/self/status TracerPid + watchdog |

### native/macos/cue-whisper (real whisper.cpp)

| File | Action | Purpose |
|------|--------|---------|
| `native/macos/cue-whisper/Package.swift` | Modified | SwiftWhisper v1.2.0 dependency |
| `native/macos/cue-whisper/Package.resolved` | Created | SPM lockfile |
| `native/macos/cue-whisper/Sources/CueWhisper/main.swift` | Modified | Real whisper_full() transcription |
| `native/macos/cue-whisper/build.sh` | Modified | Updated build flags |
| `native/macos/cue-whisper/SMOKE-TEST.md` | Created | Manual verification steps |

### native/windows/cue-whisper (stub update)

| File | Action | Purpose |
|------|--------|---------|
| `native/windows/cue-whisper/main.c` | Modified | Model env check, helpful error, deferred note |

### Workspace root

| File | Action | Purpose |
|------|--------|---------|
| `Cargo.toml` | Modified | `futures-util` workspace dep |
| `Cargo.lock` | Modified | Lock updates |

### Docs (codex review sync)

| File | Action | Purpose |
|------|--------|---------|
| `docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md` | Modified | Synced after cherry-picks |
| `docs/work/REVIEW-PHASE-3-ROUND-7.md` | Modified | Synced after cherry-picks |

## Test Count Progression

| Stage | Running tests | Δ |
|-------|---------------|---|
| R9 final | 284 | — |
| R10 final | 299 | +15 |

### Tests added in R10 (+15)

| Area | Tests | Type |
|------|-------|------|
| Anthropic streaming | 3 | Integration (wiremock) |
| OpenAI streaming | 3 | Integration (wiremock) |
| Ollama streaming | 3 | Integration (wiremock) |
| Auto-recap integration | 2 | Integration (MockLlm) |
| Whisper-stub e2e factory | 2 | Integration (ignored, CARGO_BIN_EXE) |
| Hotkey + request_cue | 2 | Unit |
| **Total** | **15** | |

Note: R9 final was 284 (not 281 as originally reported — 3 additional tests from the R7-fix-3 recheck wave landed on R9 tip before R10 branched).

## Build & Test

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 299 pass, 10 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check feat/phase-3-round-9..HEAD       ✅ clean
```

### Hardening verification (manual)

```
# strings grep on release binary
cargo build --release -p cue-dashboard 2>/dev/null
strings target/release/cue-dashboard | grep -c "wss://api.deepgram.com"    # 0
strings target/release/cue-dashboard | grep -c "wss://api.openai.com"      # 0
strings target/release/cue-dashboard | grep -c "http://localhost:11434"     # 0
strings target/release/cue-dashboard | grep -c "api.anthropic.com"         # 0

# Anti-debug (macOS)
# lldb -p $(pgrep cue-dashboard) → fails to attach (PT_DENY_ATTACH)
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Daemon doesn't emit `cue_response_chunk` Tauri events | TCP IPC vs Tauri event boundary requires more wiring; dashboard subscriber ready when this lands |
| Only OpenAI in `build_llm_provider_from_env` | Anthropic/Ollama support deferred to LLM chain settings UI round |
| Windows whisper stays stub | CMake + MSVC toolchain setup is a separate effort |
| Windows anti-debug logs only (no termination) | Too aggressive for v0.1 alpha |
| obfstr excludes cue-rag | Out of scope; grep still finds api.openai.com there |

## Known Follow-ups

1. **Daemon→dashboard `cue_response_chunk` event emission** — wire streaming chunks through Tauri event system.
2. **Multi-provider `build_llm_provider_from_env`** — Anthropic/Ollama support via LLM chain settings.
3. **Windows real whisper.cpp** — CMake + MSVC build of libwhisper.
4. **mlock for API key memory** — prevent key material from being paged to disk.
5. **SQLCipher migration** — encrypt sessions.db at rest.
6. **Tauri signing keypair** — code signing for update integrity.
7. **obfstr coverage for cue-rag** — remaining plaintext URLs.
8. **Word-level streaming from whisper.cpp** — `new_segment_callback` for partial transcripts.
9. **Process termination on debug attach** — Windows watchdog escalation.
10. **sqlite-vec swap** — replace in-memory cosine with native ANN.
11. **Native overlay passthrough handlers** — Swift + C implementations.

## Review Checklist (for reviewer)

- [ ] `complete_stream()` default impl correctly wraps `complete()` in single-chunk stream
- [ ] Router `complete_stream()` applies failover on Auth/Quota errors
- [ ] Anthropic SSE parser handles `content_block_delta` + `message_stop` correctly
- [ ] OpenAI SSE parser handles `choices[0].delta.content` + `data: [DONE]`
- [ ] Ollama NDJSON parser handles `{message:{content}, done:true}` termination
- [ ] `request_cue` loads last ~10 segments and dispatches correctly
- [ ] `spawn_auto_recap` is fire-and-forget (doesn't block MeetingEnd handler)
- [ ] Auto-recap gracefully skips when no LLM provider configured
- [ ] Whisper-stub e2e test uses CARGO_BIN_EXE env var correctly
- [ ] LiveTranscript Map dedup eliminates both catch-up and live duplicates
- [ ] `install_anti_debug()` is non-fatal on failure
- [ ] PT_DENY_ATTACH return value checked (macOS)
- [ ] Windows watchdog thread is daemon (won't prevent process exit)
- [ ] Linux TracerPid parsing handles malformed /proc/self/status gracefully
- [ ] obfstr applied to all documented strings (no plaintext leaks)
- [ ] SwiftWhisper version pinned in Package.swift
- [ ] whisper.cpp model path fallback logic correct
- [ ] RMS silence gate prevents unnecessary whisper_full() calls
- [ ] NDJSON output ABI unchanged from stub (partial/final format)
- [ ] No secrets logged, no PII in stdout
- [ ] Code style matches CLAUDE.md rules
