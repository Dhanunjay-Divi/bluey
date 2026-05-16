# Bluey / Cue — Agent Onboarding

Share this entire file with a new agent chat. It contains everything needed
to connect to uno and pick up the bluey/cue project work.

---

## Project

**bluey/cue** — A Rust+Tauri desktop app that listens to mic/system audio,
runs VAD, streams STT, shows transcripts with live overlay, and provides
real-time AI assistance during meetings (the "cue" feature).

- Current phase: **Phase 3 (Listening upgrade)** — streaming STT + overlay IPC + system audio + AI features + v0.1 alpha

## Remote machine (uno)

All code lives on **uno** (user's Mac Mini). The agent runs on divii and
drives uno over SSH.

```
SSH:        ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25
Repo root:  /Users/uno/Downloads/cue/
Rust PATH:  export PATH=/opt/homebrew/bin:/Users/uno/.cargo/bin:/usr/local/bin:$PATH
```

**Workflow:** write files to `/tmp/` on divii → `scp` them to uno → run
build/test commands via `ssh uno`. Never edit files directly on uno.

**Example recipe:**
```bash
# Write file locally
cat > /tmp/myfile.rs <<'EOF'
// contents
EOF

# Copy to uno
scp -i ~/.ssh/id_ed25519 /tmp/myfile.rs uno@192.168.4.25:/Users/uno/Downloads/cue/path/to/myfile.rs

# Build on uno
ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25 'cd /Users/uno/Downloads/cue
export PATH=/opt/homebrew/bin:/Users/uno/.cargo/bin:/usr/local/bin:$PATH
cargo build --all-targets 2>&1 | tail -20'
```

## Current state (as of 2026-05-16, post-R9 implementation)

- **main tip:** `6126b28` — R3-R6 merged, 201 tests passing
- **Branch `feat/phase-3-round-7`:** 10 commits ahead of main, 213 tests
- **Branch `feat/phase-3-round-8`:** 7 commits ahead of R7, 224 tests (process masquerade)
- **Branch `feat/phase-3-round-9`:** 16 commits ahead of R8, 281 tests — **awaiting codex review**

### R9 deliverables shipped:
- **AI features (headline "cue" product magic):** `cue-llm` crate with `LlmRouter` failover + 3 providers (Anthropic/OpenAI/Ollama). 3 specialized LLMs: `AnswerLlm` (question detection), `RecapLlm` (structured summary), `WhatToAnswerLlm` (1-2 bullet suggestions). `cue_responses` table + Tauri event. Responses route + Settings AI provider config.
- **Local RAG:** `cue-rag` crate with character-based chunker (sentence-boundary preference), in-memory cosine VectorStore (SQLite-backed), OpenAI embedder (text-embedding-3-small). Live indexing on every Final transcript.
- **Small wins:** Token bucket rate limiter (lock-free, unwired). Mouse passthrough toggle via `OverlayMessage::SetPassthrough` IPC + Tauri commands. User-rebindable keybinds with DB persistence (8 defaults).
- **R7 fix wave:** All 6 codex blockers resolved — STT factory wiring, deterministic factory test, Windows whisper compile, artifact name reconciliation, transcript dedup.

### v0.1 alpha status:
Feature-complete pending codex review. All core product functionality implemented. Deferred items are enhancements, not blockers for alpha.

### All checks green (R9 tip):
- `cargo fmt --all --check` ✅
- `cargo clippy --all-targets -- -D warnings` ✅
- `cargo build --all-targets` ✅
- `cargo test --all-targets` ✅ 281 pass
- `cd crates/cue-dashboard/ui && npm run build` ✅
- `git -P diff --check feat/phase-3-round-8..HEAD` ✅

## Workflow loop (kiro ↔ codex ↔ user)

This project uses a **two-agent review loop**:

1. **kiro** (this agent, on divii) — implements rounds on `feat/phase-3-round-N` branches
2. **codex** (separate agent, runs on uno directly) — reviews and writes verdicts to `docs/work/REVIEW-PHASE-N-ROUND-N.md`
3. **user** shuttles the review doc back by pasting codex's output or syncing files

**Round flow:**
- kiro implements → commits → writes IMPL + HANDOFF docs → hands to codex
- codex reviews → writes REVIEW doc with verdict (🟢 / 🟡 / 🔴)
- 🟢 → kiro merges, starts next round
- 🟡 → kiro folds nits into next round
- 🔴 → kiro writes `FIX-PHASE-N-ROUND-N.md` → re-hands to codex

Templates on uno:
```
docs/work/TEMPLATE-REVIEW.md
docs/work/TEMPLATE-FIX.md
docs/work/TEMPLATE-IMPL.md
```

## Standing rules

1. **Never spawn subagents** — user has standing rule against agent spawning (unless explicitly allowed per-task)
2. **Never `git push`** — all work stays local on uno
3. **No force push, no history rewrite** — once committed, commits are immutable
4. **Always build + test before committing** — full verification pipeline:
   ```bash
   cargo fmt --all --check
   cargo clippy --all-targets -- -D warnings
   cargo build --all-targets
   cargo test --all-targets
   cd crates/cue-dashboard/ui && npm run build
   git -P diff --check main..HEAD
   ```
5. **Never log secrets** — API keys use `mask_api_key` helper pattern; keyring returns masked values to frontend
6. **Commit messages** follow Conventional Commits, e.g.:
   `feat(daemon): system audio capture via native helpers [P3.R4]`

## Parallel subagent strategy (established in R6, continued in R7+R8+R9)

Each parallel subagent uses an isolated git worktree. Commits are cherry-picked onto the main feature branch after completion. A single lint/fmt commit normalizes formatting. Reconciliation commits resolve Cargo.toml/mod.rs conflicts from parallel work. **Continue this pattern for future parallel work.**

## Key files + locations on uno

```
/Users/uno/Downloads/cue/
├── Cargo.toml                            # workspace root
├── Makefile                              # R7 — build/package targets
├── INSTALL.md                            # R7 — user-facing install instructions
├── .github/workflows/release.yml         # R7 — release pipeline
├── crates/
│   ├── cue-core/                         # types: pcm, vad, stt, overlay_ipc, session, audio
│   ├── cue-llm/                          # R9 — LLM provider abstraction + router
│   │   └── src/{lib,router,anthropic,openai,ollama}.rs
│   ├── cue-rag/                          # R9 — RAG: chunker + vector store + embedder
│   │   └── src/{lib,chunker,store,embedder}.rs
│   ├── cue-stealth/                      # R8 — process masquerading
│   │   └── src/{lib,macos,linux,windows}.rs
│   ├── cue-daemon/
│   │   ├── src/
│   │   │   ├── audio/
│   │   │   ├── stt/
│   │   │   │   ├── deepgram.rs           # R3 — Nova-3 provider
│   │   │   │   ├── openai.rs             # R6 — OpenAI Realtime transcription
│   │   │   │   ├── router.rs             # R5+R7 — SttRouter 3-tier failover
│   │   │   │   ├── factory.rs            # R9(R7fix) — build_stt_chain()
│   │   │   │   ├── whisper/              # R7 — LocalWhisperProvider + parser
│   │   │   │   ├── echo.rs              # R5 — EchoProvider stub
│   │   │   │   └── mock.rs
│   │   │   ├── llm/                      # R9 — specialized LLMs
│   │   │   │   ├── mod.rs               # CueResponse type + ends_with_question
│   │   │   │   ├── answer.rs            # AnswerLlm
│   │   │   │   ├── recap.rs             # RecapLlm
│   │   │   │   └── suggest.rs           # WhatToAnswerLlm
│   │   │   ├── util/
│   │   │   │   ├── mod.rs
│   │   │   │   └── rate_limiter.rs      # R9 — token bucket
│   │   │   ├── db/
│   │   │   │   └── mod.rs               # migrations 009 (cue_responses), 011 (user_keybinds)
│   │   │   ├── app.rs                    # R6+R7+R9 — live transcript + RAG indexing
│   │   │   └── bin/
│   │   │       └── whisper_stub.rs       # R7 — test stub binary
│   │   └── tests/
│   │       ├── live_transcript_emit.rs   # R7 — 3 tests
│   │       ├── live_transcript_dedup.rs  # R9(R7fix) — 4 tests
│   │       ├── whisper_integration.rs    # R7 — 9 tests
│   │       └── stt_factory_integration.rs # R9(R7fix) — 4 tests
│   └── cue-dashboard/                    # Tauri + React UI
│       ├── src/
│       │   ├── lib.rs                    # R7+R8 — live transcript poller + startup disguise
│       │   └── commands.rs               # R7+R8+R9 — all Tauri commands
│       ├── ui/src/
│       │   ├── pages/Settings.tsx        # R8+R9 — settings with disguise + AI config
│       │   ├── pages/Responses.tsx       # R9 — AI responses page
│       │   ├── routes/Responses.tsx      # R9 — AI responses route component
│       │   ├── lib/disguise.ts           # R8 — invoke wrappers
│       │   ├── routes/LiveTranscript.tsx  # R7 — live transcript route
│       │   └── components/LiveTranscriptList.tsx  # R7 — auto-scroll list
│       └── icons/disguise/               # R8 — placeholder PNGs + README
├── native/
│   ├── macos/
│   │   ├── cue-audio/                    # Swift: ScreenCaptureKit + AVAudioEngine
│   │   ├── cue-overlay/                  # Swift: NSWindow overlay
│   │   └── cue-whisper/                  # R7 — Swift whisper helper stub
│   └── windows/
│       ├── cue-audio/main.c              # C: WASAPI loopback
│       ├── cue-overlay/main.c            # C: Direct2D overlay
│       └── cue-whisper/main.c            # R7+R9fix — C whisper helper stub (compiles)
├── infra/
│   ├── homebrew/bluey.rb                 # R7+R9fix — Homebrew formula (correct names)
│   ├── scoop/bluey.json                  # R7+R9fix — Scoop manifest (correct names)
│   └── scripts/
└── docs/work/
    ├── IMPL-PHASE-3-ROUND-{1..9}.md
    ├── FIX-PHASE-3-ROUND-7.md            # R9 — fix doc for R7 blockers
    ├── PHASE-3-ROUND-{1..9}-HANDOFF-FOR-CODEX-REVIEW.md
    ├── REVIEW-PHASE-3-ROUND-{1,2,3,5,6,7}.md
    ├── HANDOFF-TO-CODEX-FROM-KIRO.md
    ├── AGENT-ONBOARDING.md               # this file
    ├── PLAN-STT-FALLBACK-CHAIN.md
    └── PLAN-DISTRIBUTION.md
```

## Pending work (after R9 review passes → v0.1 alpha ships)

### Next rounds (user-prioritized order)

1. **Real whisper.cpp integration** — replace stub helpers with actual whisper.cpp inference; bundle `tiny.en` model.
2. **Streaming LLM responses** — SSE/chunked response handling for all 3 providers.
3. **Auto-recap session-lifecycle hook** — trigger RecapLlm on session end.
4. **Native overlay passthrough handlers** — macOS Swift `window.ignoresMouseEvents` + Windows C `WS_EX_TRANSPARENT`.
5. **sqlite-vec swap** — replace in-memory cosine with native ANN search in VectorStore.
6. **Logging + crash reporting** — `tracing-appender` file rotation + panic dump files.
7. **More specialized LLMs** — the other 17 from natively-cluely (AssistLLM, SummaryLLM, ActionItemsLLM, etc.).
8. **Long-session stress tests** — `#[ignore]` test running 5-minute synthetic pipeline.
9. **Screenshot + cropper window** — capture screen region for context injection into LLM prompts.
10. **Calendar / meeting-platform integration** — auto-detect meeting start/end.

## First actions for a new agent

1. SSH to uno and check current state:
   ```bash
   ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25 'cd /Users/uno/Downloads/cue
   git -P status
   git -P log --oneline -n 20
   git -P branch --show-current'
   ```
2. Read these docs on uno in order:
   - `docs/work/IMPL-PHASE-3-ROUND-9.md` (latest round's context)
   - `docs/work/PHASE-3-ROUND-9-HANDOFF-FOR-CODEX-REVIEW.md`
   - `docs/work/FIX-PHASE-3-ROUND-7.md` (R7 blocker resolution)
3. Ask the user what the current task is — don't guess. Possible states:
   - Waiting on codex review → nothing to do, just read and be ready
   - Codex gave 🔴 → need to write FIX doc and address feedback
   - Codex gave 🟢 → ready to merge + start next round
   - User scoping a new round → await explicit scope
4. Always run the full verification pipeline before committing anything.

## Troubleshooting

- **`command not found` on uno**: export the PATH first (top of this doc)
- **SSH hangs**: verify key permissions (`chmod 600 ~/.ssh/id_ed25519`)
- **Build takes forever**: release builds are ~55 s; incremental dev builds are <10 s
- **Cargo complains about workspace dep**: check root `Cargo.toml` `[workspace.dependencies]` first
- **Swift build fails**: ensure Xcode CLT installed; `swift build` from `native/macos/cue-overlay/`
- **Keyring tests fail**: `secrets::tests::roundtrip` is `#[ignore]` — requires interactive Keychain access

## Contact points

- **User's preferred flow**: SSH from divii → `scp` files up → `ssh` to run commands
- **Local staging directory**: `/tmp/` on divii (not on uno)
- **Never use `rsync`** — `scp` is the convention throughout this project

---

Give this file to the new agent alongside your first prompt.
