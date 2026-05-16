# IMPL — Phase 3 Round 9 (AI Features + Local RAG + Small Wins + R7 Fix Wave)

**Branch**: `feat/phase-3-round-9`
**Base**: `feat/phase-3-round-8` tip (`d00880a`, 224 tests)
**Tip**: `115ac8d` (16 commits ahead of R8, 281 tests)

## Scope

**Four themes: headline AI cue features, local RAG, small wins, and R7 blocker fixes.**

Round 9 delivers the core "cue" product magic — the LLM router with 3 providers, 3 specialized LLMs that generate real-time AI responses during meetings, a local RAG pipeline for semantic search over transcripts, and utility features (rate limiter, mouse passthrough, user-rebindable keybinds). It also resolves all 6 blockers from codex's R7 review.

**Does:**

1. `cue-llm` crate: `LlmProvider` trait, `LlmRouter` with failover (Auth/Quota → next provider), `LlmRequest`/`LlmResponse` types, `LlmError` with `should_failover()`/`is_retryable()` classification.
2. Three providers: `AnthropicProvider` (Claude 3.5 Sonnet, Messages API), `OpenAiProvider` (GPT-4o-mini, Chat Completions), `OllamaProvider` (llama3.2, local `/api/chat`).
3. Three specialized LLMs in `cue-daemon/src/llm/`: `AnswerLlm` (question detection via `ends_with_question`), `RecapLlm` (structured markdown summary), `WhatToAnswerLlm` (1-2 short bullet suggestions).
4. `CueResponse` type with id/kind/text/ts_ms/source_session_id/source_text.
5. `cue_responses` table (migration 009) + `list_cue_responses()` DB accessor.
6. Tauri commands: `save_llm_api_key`, `list_llm_providers`, `list_responses`, `set_llm_chain`.
7. `cue_response` Tauri event emission for real-time UI updates.
8. Dashboard `Responses` route with grouped cards (answers/suggestions/recaps), copy-to-clipboard, real-time event subscription.
9. Settings page AI provider config section: per-provider API key input, chain ordering with drag-up/down.
10. `cue-rag` crate: character-based `Chunker` (800 chars max, 200 overlap, sentence-boundary preference), `VectorStore` (SQLite-backed with in-memory cosine similarity), `EmbeddingProvider` trait + `OpenAiEmbedder` (text-embedding-3-small, 1536 dims).
11. Live RAG indexing: on every Final transcript, daemon chunks + embeds + indexes into the vector store (fire-and-forget async task).
12. Token bucket `RateLimiter`: lock-free atomic implementation, `try_acquire(n)` + async `acquire(n)`, `checked_div` safety for zero refill rate.
13. `OverlayMessage::SetPassthrough { enabled: bool }` IPC variant for mouse passthrough toggle.
14. `set_mouse_passthrough` / `get_mouse_passthrough` Tauri commands with DB persistence.
15. `user_keybinds` table (migration 011) + `list_keybinds` / `set_keybind` / `reset_keybinds` Tauri commands with defaults for 8 actions.
16. R7 fix wave: STT factory wiring, deterministic factory test, Windows whisper compile fix, artifact name reconciliation, transcript dedup.

**Does NOT:**

- Stream LLM responses (`supports_streaming() == false` on all providers).
- Implement function-calling / tool use.
- Ship the other 17 specialized LLMs from natively-cluely reference.
- Wire Cmd+Shift+A hotkey to trigger AnswerLLM (event wired but daemon-side trigger logic deferred).
- Auto-recap on session end (DB+LLM ready; session-lifecycle hook deferred).
- Use sqlite-vec for ANN search (in-memory cosine fallback shipped; `EmbeddingProvider` trait stable for swap).
- Support multi-provider embedding (only OpenAI today).
- Implement native overlay passthrough (IPC variant defined; macOS Swift / Windows C overlay code not touched).
- Wire rate limiter to any caller (utility ready, no integration point yet).
- Hot-reload keybinds at runtime (persists + lists; dynamic re-registration deferred).

## Commits (16, chronological bottom → top)

| # | Hash | Title | Theme |
|---|------|-------|-------|
| 1 | `4abf465` | `fix(daemon): wire LocalWhisper + OpenAI into production STT factory [P3.R7 fix]` | R7 fix |
| 2 | `fbd0a86` | `test(daemon): deterministic factory test for fallback chain [P3.R7 fix]` | R7 fix |
| 3 | `ad2c3ac` | `fix(whisper): native/windows/cue-whisper/main.c compiles + CI check [P3.R7 fix]` | R7 fix |
| 4 | `22c8050` | `fix(infra): reconcile release artifact names across workflow + brew + scoop [P3.R7 fix]` | R7 fix |
| 5 | `480f06c` | `fix(daemon): live transcript dedup partial->final [P3.R7 fix]` | R7 fix |
| 6 | `277f451` | `feat(llm): cue-llm crate with router + Anthropic/OpenAI/Ollama providers [P3.R9]` | AI features |
| 7 | `7b33b56` | `feat(daemon): AnswerLLM + RecapLLM + WhatToAnswerLLM + cue_responses table [P3.R9]` | AI features |
| 8 | `020d74c` | `feat(dashboard): LLM Tauri commands + cue_response event wiring [P3.R9]` | AI features |
| 9 | `e6bc1da` | `feat(dashboard): Responses route + Settings AI provider config [P3.R9]` | AI features |
| 10 | `b796fc2` | `feat(rag): cue-rag crate with chunker + vector store + OpenAI embedder [P3.R9]` | RAG |
| 11 | `29d8333` | `feat(daemon): live RAG indexing on every Final transcript [P3.R9]` | RAG |
| 12 | `ea16ea2` | `feat(daemon): token bucket rate limiter [P3.R9]` | Small wins |
| 13 | `2960b4e` | `feat(overlay): mouse passthrough toggle via IPC [P3.R9]` | Small wins |
| 14 | `da73b0b` | `feat(dashboard): user-rebindable keybinds with persistence [P3.R9]` | Small wins |
| 15 | `8005278` | `chore(p3r9): reconcile parallel-subagent merges (Cargo.toml + db/mod.rs union splits)` | Reconciliation |
| 16 | `115ac8d` | `chore(p3r9): clippy fix manual_checked_ops in rate_limiter` | Lint fix |

## Files Created / Modified

### cue-llm crate (new)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-llm/Cargo.toml` | Created | Crate manifest: async-trait, reqwest, serde, thiserror, wiremock (dev) |
| `crates/cue-llm/src/lib.rs` | Created | `LlmProvider` trait, `LlmRequest`/`LlmResponse`/`LlmError` types |
| `crates/cue-llm/src/router.rs` | Created | `LlmRouter`: failover on Auth/Quota, AtomicUsize active index, 7 tests |
| `crates/cue-llm/src/anthropic.rs` | Created | `AnthropicProvider`: Messages API, wiremock tests (success/auth/rate/server/network) |
| `crates/cue-llm/src/openai.rs` | Created | `OpenAiProvider`: Chat Completions, wiremock tests (5 tests) |
| `crates/cue-llm/src/ollama.rs` | Created | `OllamaProvider`: local /api/chat, wiremock tests (5 tests) |

### cue-rag crate (new)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-rag/Cargo.toml` | Created | Crate manifest: rusqlite, reqwest, async-trait, anyhow, serde_json |
| `crates/cue-rag/src/lib.rs` | Created | Re-exports: Chunk, Chunker, VectorStore, RagHit, EmbeddingProvider |
| `crates/cue-rag/src/chunker.rs` | Created | Character-based chunker with sentence-boundary preference, 4 tests |
| `crates/cue-rag/src/store.rs` | Created | SQLite-backed VectorStore with in-memory cosine similarity, 4 tests |
| `crates/cue-rag/src/embedder.rs` | Created | `EmbeddingProvider` trait + `OpenAiEmbedder` (text-embedding-3-small) |

### cue-daemon LLM module

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/src/llm/mod.rs` | Created | Module root: `CueResponse` type, `ends_with_question()` helper |
| `crates/cue-daemon/src/llm/answer.rs` | Created | `AnswerLlm`: concise meeting question answers, 1 test |
| `crates/cue-daemon/src/llm/recap.rs` | Created | `RecapLlm`: structured markdown summary, 1 test |
| `crates/cue-daemon/src/llm/suggest.rs` | Created | `WhatToAnswerLlm`: 1-2 bullet suggestions, 2 tests |

### cue-daemon DB + RAG integration

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/src/db/mod.rs` | Modified | Added `pub mod rag;`, `cue_responses` table migration, `list_cue_responses()` |
| `crates/cue-daemon/src/app.rs` | Modified | RAG pipeline init, live indexing on Final transcripts, factory call |
| `crates/cue-daemon/src/stt/factory.rs` | Created | `build_stt_chain()` — R7 fix |
| `crates/cue-daemon/src/stt/mod.rs` | Modified | Added `pub mod factory;` |
| `crates/cue-daemon/src/stt/router.rs` | Modified | Added `provider_names()` |
| `crates/cue-daemon/src/util/mod.rs` | Created | `pub mod rate_limiter;` |
| `crates/cue-daemon/src/util/rate_limiter.rs` | Created | Lock-free token bucket, 7 tests |
| `crates/cue-daemon/tests/stt_factory_integration.rs` | Created | 4 factory tests — R7 fix |
| `crates/cue-daemon/tests/live_transcript_dedup.rs` | Created | 4 dedup tests — R7 fix |

### cue-dashboard commands

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-dashboard/src/commands.rs` | Modified | Added: `save_llm_api_key`, `list_llm_providers`, `list_responses`, `set_llm_chain`, `set_mouse_passthrough`, `get_mouse_passthrough`, `list_keybinds`, `set_keybind`, `reset_keybinds` |

### cue-dashboard UI

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-dashboard/ui/src/routes/Responses.tsx` | Created | AI responses route: grouped cards, real-time event subscription, copy |
| `crates/cue-dashboard/ui/src/pages/Responses.tsx` | Created | Page wrapper (re-export) |
| `crates/cue-dashboard/ui/src/pages/Settings.tsx` | Modified | Added AI provider config section: API keys + chain ordering |

### cue-core (overlay IPC)

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-core/src/overlay_ipc.rs` | Modified | Added `OverlayMessage::SetPassthrough { enabled: bool }` variant + test |

### Infra (R7 fixes)

| File | Action | Purpose |
|------|--------|---------|
| `.github/workflows/release.yml` | Modified | Fixed binary names + versioned artifacts |
| `.github/workflows/ci.yml` | Modified | Added cue-whisper to Windows compile-check |
| `Makefile` | Modified | Fixed package targets |
| `infra/homebrew/bluey.rb` | Modified | Fixed install stanza |
| `infra/scoop/bluey.json` | Modified | Fixed URL pattern |
| `INSTALL.md` | Modified | Updated binary references |
| `native/windows/cue-whisper/main.c` | Modified | Fixed printf C syntax |

## Design Decisions

### 1. LlmRouter failover semantics

The router tries providers in order. On `Auth` or `Quota` errors (classified by `should_failover()`), it advances to the next provider. On `Network` errors (retryable but not failover-worthy), it returns immediately — the caller can retry. On `Provider` errors (4xx/5xx non-auth), it also returns immediately. The `active` AtomicUsize remembers the last successful provider to avoid re-trying known-bad providers on subsequent calls.

### 2. Specialized LLMs as thin wrappers

Each specialized LLM (Answer, Recap, WhatToAnswer) is a zero-state struct with a single `run()` method that constructs an `LlmRequest` with a hardcoded system prompt and delegates to any `&dyn LlmProvider`. This makes them trivially testable with fake providers and composable with the router.

### 3. RAG: in-memory cosine over sqlite-vec

The `VectorStore` stores embeddings as BLOBs in SQLite and computes cosine similarity in Rust at query time. This is O(n) per query but avoids the sqlite-vec dependency (which requires a custom SQLite build). The `EmbeddingProvider` trait is stable — swapping to sqlite-vec virtual tables is a follow-up that doesn't change the public API.

### 4. Live RAG indexing (fire-and-forget)

On every Final transcript, the daemon spawns a fire-and-forget async task that chunks the text, embeds each chunk via OpenAI, and indexes into the vector store. If the OpenAI key is missing, the RAG pipeline is `None` and indexing is silently skipped.

### 5. Rate limiter: lock-free atomics

Uses fixed-point scaled integers (`tokens * 1_000_000`) stored in `AtomicU64` to avoid floating-point atomics. The `checked_div` in `acquire()` prevents division-by-zero when `refill_per_sec` is 0 (bucket that never refills — useful for testing).

### 6. Mouse passthrough: IPC variant only

`OverlayMessage::SetPassthrough { enabled: bool }` is defined and serialization-tested, but the native overlay binaries (Swift/C) don't handle it yet. The Tauri commands persist the state to DB so it survives restarts. Actual passthrough implementation requires platform-specific window attribute changes.

### 7. Keybinds: DB-backed with defaults

8 default keybinds are defined in code. On first `list_keybinds` call, the table is created (migration 011). Users can override any keybind via `set_keybind`. `reset_keybinds` drops all overrides. Hot-reload (re-registering global shortcuts at runtime) is deferred.

## Test Count Progression

| Stage | Running tests | Δ |
|-------|---------------|---|
| R8 final | 224 | — |
| R9 final | 281 | +57 |

### Tests added in R9 (+57)

| Area | Tests | Type |
|------|-------|------|
| cue-llm router | 7 | Unit (mock providers) |
| cue-llm Anthropic | 5 | Integration (wiremock) |
| cue-llm OpenAI | 5 | Integration (wiremock) |
| cue-llm Ollama | 5 | Integration (wiremock) |
| Specialized LLMs (answer + recap + suggest) | 4 | Unit (fake provider) |
| cue-rag chunker | 4 | Unit |
| cue-rag vector store | 4 | Unit |
| Rate limiter | 7 | Unit + async |
| STT factory integration (R7 fix) | 4 | Integration |
| Live transcript dedup (R7 fix) | 4 | Integration |
| Overlay SetPassthrough serialization | 1 | Unit |
| Keybinds DB operations | 3 | Unit |
| LLM Tauri commands | 4 | Unit |
| **Total** | **57** | |

## Build & Test

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 281 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check feat/phase-3-round-8..HEAD       ✅ clean
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| 3 providers instead of 7 | Anthropic/OpenAI/Ollama cover cloud+local; Gemini/Groq/Together/Fireworks deferred to avoid scope creep |
| 3 specialized LLMs instead of 20+ | AnswerLLM/RecapLLM/WhatToAnswerLLM are the core "cue" experience; remaining 17 are follow-ups |
| In-memory cosine instead of sqlite-vec | Avoids custom SQLite build dependency; trait-stable for future swap |
| Rate limiter unwired | Utility ready; wiring requires deciding which subsystems need rate-gating |
| Two reconciliation commits | Artifact of parallel subagent workflow; no logic changes, only Cargo.toml + mod.rs merge resolution |

## Known Follow-ups

1. **Streaming LLM responses** — `supports_streaming()` returns false; need SSE/chunked response handling.
2. **Function-calling / tool use** — extend `LlmRequest` with tool definitions.
3. **Remaining 17 specialized LLMs** — from natively-cluely reference (AssistLLM, SummaryLLM, ActionItemsLLM, etc.).
4. **Cmd+Shift+A hotkey → AnswerLLM trigger** — event wired but daemon-side trigger logic deferred.
5. **Auto-recap on session end** — DB+LLM ready; session-lifecycle hook deferred.
6. **sqlite-vec swap** — replace in-memory cosine with native ANN search.
7. **Multi-provider embedding** — Gemini, Ollama, local ONNX models.
8. **Native overlay passthrough** — macOS Swift `window.ignoresMouseEvents` + Windows `WS_EX_TRANSPARENT`.
9. **Rate limiter integration** — wire to LLM provider calls.
10. **Keybind hot-reload** — dynamic global shortcut re-registration at runtime.
11. **Real whisper.cpp integration** — replace stub helpers.

## Review Checklist (for reviewer)

- [ ] cue-llm: `LlmProvider` trait shape is clean (name, complete, supports_streaming)
- [ ] cue-llm: Router failover only on Auth/Quota, not Network/Provider
- [ ] cue-llm: All 3 providers handle HTTP status codes correctly (401→Auth, 429→Quota, 5xx→Provider)
- [ ] cue-llm: All providers have `with_base_url` test seam
- [ ] Specialized LLMs: system prompts are concise and task-appropriate
- [ ] Specialized LLMs: `CueResponse` includes all required fields (id, kind, text, ts_ms, source)
- [ ] cue-rag: Chunker respects max_chars and produces overlapping chunks
- [ ] cue-rag: VectorStore cascade-deletes embeddings when chunks are deleted
- [ ] cue-rag: Cosine similarity handles zero-norm vectors (returns 0.0)
- [ ] cue-rag: Dimension mismatch rejected at index time
- [ ] RAG indexing: fire-and-forget doesn't block transcript processing
- [ ] RAG indexing: gracefully disabled when no OpenAI key
- [ ] Rate limiter: `checked_div` prevents panic on zero refill rate
- [ ] Rate limiter: concurrent test proves exactly `capacity` tokens granted
- [ ] Overlay: `SetPassthrough` serializes/deserializes correctly (serde rename)
- [ ] Keybinds: defaults cover all 8 actions
- [ ] Keybinds: `reset_keybinds` drops overrides without dropping defaults
- [ ] R7 fixes: factory produces documented 3-tier chain
- [ ] R7 fixes: Windows whisper helper compiles
- [ ] R7 fixes: artifact names consistent across all infra files
- [ ] No secrets logged, no PII in stdout
- [ ] Code style matches CLAUDE.md rules

---

## Update: STT factory scope clarification (R7-recheck-2)

The `build_stt_chain` factory applies **only** to streaming providers used
by the continuous system-audio path. The default mic + chunked-REST path
(`real_audio_loop` → `transcribe_audio_file`) is **not** routed through
the factory: it posts WAV chunks to `runtime.stt_endpoint` and bypasses
the streaming `SttProvider` trait entirely.

Therefore: `BLUEY_STT_FALLBACK_OPENAI=1` and `BLUEY_STT_LOCAL_WHISPER=1`
do NOT affect the chunked-REST mic path today.

Unifying the two paths (streaming providers everywhere) is a deferred
follow-up. The factory module doc-comment in
`crates/cue-daemon/src/stt/factory.rs` carries this caveat inline.
