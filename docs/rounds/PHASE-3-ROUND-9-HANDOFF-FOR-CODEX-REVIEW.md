# Phase 3 Round 9 — Handoff for Codex Review

**Branch**: `feat/phase-3-round-9`
**Base**: `feat/phase-3-round-8` tip (`d00880a`, 224 tests)
**Authors**: kiro (parallel subagents with worktree isolation), uno (user — oversight)

## Scope

Round 9 of Phase 3. Four themes: headline AI features (the "cue" product magic), local RAG, small wins, and R7 blocker fixes. This is the **v0.1 alpha feature-complete** round — all core product functionality is implemented pending this review.

### Commits (16 ahead of R8)

```
115ac8d chore(p3r9): clippy fix manual_checked_ops in rate_limiter
8005278 chore(p3r9): reconcile parallel-subagent merges (Cargo.toml + db/mod.rs union splits)
da73b0b feat(dashboard): user-rebindable keybinds with persistence [P3.R9]
2960b4e feat(overlay): mouse passthrough toggle via IPC [P3.R9]
ea16ea2 feat(daemon): token bucket rate limiter [P3.R9]
29d8333 feat(daemon): live RAG indexing on every Final transcript [P3.R9]
b796fc2 feat(rag): cue-rag crate with chunker + vector store + OpenAI embedder [P3.R9]
e6bc1da feat(dashboard): Responses route + Settings AI provider config [P3.R9]
020d74c feat(dashboard): LLM Tauri commands + cue_response event wiring [P3.R9]
7b33b56 feat(daemon): AnswerLLM + RecapLLM + WhatToAnswerLLM + cue_responses table [P3.R9]
277f451 feat(llm): cue-llm crate with router + Anthropic/OpenAI/Ollama providers [P3.R9]
480f06c fix(daemon): live transcript dedup partial->final [P3.R7 fix]
22c8050 fix(infra): reconcile release artifact names across workflow + brew + scoop [P3.R7 fix]
ad2c3ac fix(whisper): native/windows/cue-whisper/main.c compiles + CI check [P3.R7 fix]
fbd0a86 test(daemon): deterministic factory test for fallback chain [P3.R7 fix]
4abf465 fix(daemon): wire LocalWhisper + OpenAI into production STT factory [P3.R7 fix]
```

## Verification — ALL GREEN

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 281 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check feat/phase-3-round-8..HEAD       ✅ clean
```

### Test count delta

| Tier | R8 final | R9 final | Δ |
|------|----------|----------|---|
| cue-llm (router + 3 providers) | 0 | 22 | +22 |
| Specialized LLMs (answer + recap + suggest) | 0 | 4 | +4 |
| cue-rag (chunker + store) | 0 | 8 | +8 |
| Rate limiter | 0 | 7 | +7 |
| STT factory integration (R7 fix) | 0 | 4 | +4 |
| Live transcript dedup (R7 fix) | 0 | 4 | +4 |
| Overlay SetPassthrough + keybinds + LLM commands | 0 | 8 | +8 |
| Previous (carried forward) | 224 | 224 | — |
| **Total running** | **224** | **281** | **+57** |

## Architecture Diagram — Round 9 Additions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                           cue-llm crate [NEW]                                │
│                                                                             │
│  LlmRequest ──▶ LlmRouter (failover: Auth/Quota → next)                   │
│                      │                                                      │
│         ┌────────────┼────────────────────┐                                │
│         ▼            ▼                    ▼                                 │
│   Anthropic      OpenAI (GPT-4o-mini)   Ollama (local)                     │
│   (Claude 3.5)   /v1/chat/completions   /api/chat                          │
│   /v1/messages                                                              │
│                                                                             │
│  Error classification:                                                      │
│    Auth (401) → failover    Network → return (retryable)                   │
│    Quota (429) → failover   Provider (5xx/4xx) → return                    │
└─────────────────────────────────────────────────────────────────────────────┘
         │
         ▼ (used by)
┌─────────────────────────────────────────────────────────────────────────────┐
│                    cue-daemon LLM module [NEW]                                │
│                                                                             │
│  Specialized LLMs (thin wrappers over LlmProvider):                         │
│    AnswerLlm ──── "question detected (ends_with_question)" → concise answer │
│    RecapLlm ───── "summarize transcript" → structured markdown              │
│    WhatToAnswerLlm ── "suggest next thing to say" → 1-2 bullets            │
│                                                                             │
│  CueResponse { id, kind, text, ts_ms, source_session_id, source_text }     │
│         │                                                                   │
│         ▼                                                                   │
│  cue_responses table (migration 009) + Tauri event "cue_response"          │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                           cue-rag crate [NEW]                                │
│                                                                             │
│  Final transcript ──▶ Chunker (800 chars, 200 overlap, sentence boundary)  │
│         │                                                                   │
│         ▼                                                                   │
│  OpenAiEmbedder (text-embedding-3-small, 1536 dims)                        │
│         │                                                                   │
│         ▼                                                                   │
│  VectorStore (SQLite + in-memory cosine)                                   │
│    - rag_chunks table (session_id, text, offsets, ts_ms)                   │
│    - rag_embeddings table (chunk_id FK → CASCADE DELETE, BLOB)             │
│    - query(embedding, limit, session_id?) → Vec<RagHit>                    │
│                                                                             │
│  Live indexing: daemon spawns fire-and-forget task on every Final           │
│  Disabled gracefully when no OpenAI API key configured                      │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                         Small Wins [NEW]                                      │
│                                                                             │
│  RateLimiter (cue-daemon/src/util/rate_limiter.rs):                        │
│    Lock-free token bucket (AtomicU64 fixed-point)                          │
│    try_acquire(n) / acquire(n).await / available()                         │
│    checked_div safety for zero refill rate                                  │
│    NOT YET WIRED to any caller                                              │
│                                                                             │
│  OverlayMessage::SetPassthrough { enabled: bool }:                          │
│    IPC variant defined in cue-core/overlay_ipc.rs                          │
│    Tauri commands: set_mouse_passthrough / get_mouse_passthrough            │
│    Persisted to DB (overlay_passthrough setting)                            │
│    Native overlay handlers NOT YET implemented                              │
│                                                                             │
│  User Keybinds (migration 011):                                             │
│    user_keybinds table (action TEXT PK, accelerator TEXT)                   │
│    8 default actions (toggle_overlay, push_to_talk, etc.)                  │
│    list_keybinds / set_keybind / reset_keybinds Tauri commands             │
│    Hot-reload at runtime DEFERRED                                           │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Per-Feature-Area Review Checklist

### R7 Fixes (5 commits: `4abf465..480f06c`)

- [ ] `factory.rs`: `build_stt_chain()` constructs Deepgram → OpenAI → LocalWhisper based on env vars
- [ ] `factory.rs`: graceful degradation when no API keys configured (returns EchoProvider)
- [ ] Factory test: 4 tests cover all chain compositions with env var save/restore
- [ ] Windows whisper: `main.c` printf strings properly quoted and escaped
- [ ] CI: cue-whisper added to Windows compile-check step
- [ ] Artifact names: consistent `bluey-{version}-{os}-{arch}.{ext}` across release.yml, Makefile, formula, manifest, INSTALL.md
- [ ] Dedup: `dedup_partial_on_final()` removes superseded partial (prefix match, case-insensitive)
- [ ] Dedup: 4 tests cover match/no-match/case-insensitive/most-recent-only

### cue-llm Router + Providers (`277f451`)

- [ ] `LlmRouter`: failover only on `should_failover()` (Auth/Quota), not Network/Provider
- [ ] `LlmRouter`: `active` AtomicUsize remembers last successful provider
- [ ] `LlmRouter`: empty provider list returns `Provider("no providers configured")`
- [ ] Anthropic: correct headers (`x-api-key`, `anthropic-version: 2023-06-01`)
- [ ] Anthropic: concatenates multiple content blocks
- [ ] OpenAI: Bearer auth, Chat Completions format, handles null content
- [ ] Ollama: no auth required, `stream: false`, options for temperature/num_predict
- [ ] All providers: `with_base_url` test seam (cfg(test) only)
- [ ] All providers: wiremock tests cover success + error paths

### Specialized LLMs (`7b33b56`)

- [ ] `AnswerLlm`: system prompt mentions "meeting" + "concise"
- [ ] `AnswerLlm`: max_tokens=256, temperature=0.3 (conservative)
- [ ] `RecapLlm`: system prompt requests "key decisions, action items, unresolved questions"
- [ ] `RecapLlm`: max_tokens=1024 (longer output for summaries)
- [ ] `WhatToAnswerLlm`: system prompt requests "1-2 short bullet points"
- [ ] `ends_with_question()`: simple `trim().ends_with('?')` — intentionally naive
- [ ] `CueResponse::new()`: generates UUID, captures current timestamp

### cue-rag (`b796fc2`, `29d8333`)

- [ ] Chunker: max_chars=800 (~200 tokens), overlap_chars=200 (~50 tokens)
- [ ] Chunker: sentence boundary search in last 20% of chunk
- [ ] Chunker: empty/whitespace-only input returns empty vec
- [ ] Chunker: text shorter than max_chars returns single chunk
- [ ] VectorStore: `PRAGMA journal_mode=WAL` + `foreign_keys=ON`
- [ ] VectorStore: cascade delete (rag_embeddings FK → rag_chunks ON DELETE CASCADE)
- [ ] VectorStore: dimension mismatch rejected at index time with clear error
- [ ] VectorStore: cosine similarity handles zero-norm (returns 0.0, no NaN/panic)
- [ ] VectorStore: session_id filter works correctly
- [ ] OpenAiEmbedder: validates response dimension matches expected 1536
- [ ] Live indexing: fire-and-forget (doesn't block transcript processing)
- [ ] Live indexing: `None` RAG pipeline when no API key (graceful skip)

### Small Wins (`ea16ea2`, `2960b4e`, `da73b0b`)

- [ ] RateLimiter: `checked_div` in `acquire()` prevents panic when refill_per_sec=0
- [ ] RateLimiter: concurrent test proves exactly `capacity` tokens granted (no over-grant)
- [ ] RateLimiter: refill caps at capacity (no overflow)
- [ ] `OverlayMessage::SetPassthrough`: serde rename to `set_passthrough` (snake_case)
- [ ] `OverlayMessage::SetPassthrough`: round-trip encode/decode test passes
- [ ] Passthrough commands: persist to DB, return bool
- [ ] Keybinds: `ensure_keybinds_table()` is idempotent (CREATE IF NOT EXISTS)
- [ ] Keybinds: 8 default actions defined in code
- [ ] Keybinds: `set_keybind` validates accelerator string is non-empty
- [ ] Keybinds: `reset_keybinds` drops all rows (returns to defaults)

### Reconciliation Commits (`8005278`, `115ac8d`)

- [ ] `8005278`: only Cargo.toml dependency union + db/mod.rs module declarations — no logic changes
- [ ] `115ac8d`: only replaces `/` with `checked_div` in rate_limiter.rs — no logic changes
- [ ] Both are artifacts of parallel subagent workflow, not bugs

## Explicit Deferrals (NOT in Round 9)

1. **LLM streaming responses** — `supports_streaming() == false`; need SSE/chunked handling.
2. **Function-calling / tool use** — extend `LlmRequest` with tool definitions.
3. **Other 17 specialized LLMs** — from natively-cluely reference (AssistLLM, SummaryLLM, etc.).
4. **Cmd+Shift+A hotkey → AnswerLLM** — event wired but daemon-side trigger deferred.
5. **Auto-recap on session end** — DB+LLM ready; session-lifecycle hook deferred.
6. **sqlite-vec swap** — in-memory cosine fallback shipped; trait stable for swap.
7. **Multi-provider embedding** — only OpenAI today; trait supports others.
8. **Native overlay passthrough** — IPC variant defined; Swift/C handlers not touched.
9. **Rate limiter callers** — utility ready; no integration point yet.
10. **Keybind hot-reload** — persists + lists; dynamic re-registration deferred.
11. **Two reconciliation commits** — parallel-subagent merge artifacts, not bugs.

## Verdict Request

Codex: review the 16 commits (4 themes: AI features, RAG, small wins, R7 fixes). Write `docs/work/REVIEW-PHASE-3-ROUND-9.md` with verdict.

- 🟢 **ACCEPT** → merge R7+R8+R9 to main, ship v0.1 alpha
- 🟡 **ACCEPT WITH NITS** → fold nits into Round 10
- 🔴 **REQUEST CHANGES** → kiro writes fix doc and re-hands
