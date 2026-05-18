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

All code lives on **uno** (user's Mac Mini). Some agent sessions run directly
inside `/Users/uno/Downloads/cue`; older Kiro sessions ran from divii and drove
uno over SSH. If you are already on uno, work locally and skip the SSH/scp
recipe.

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

## Current state (updated 2026-05-18, post-R12/R13 Codex pass)

- **Active branch:** `feat/phase-3-round-12`
- **Release target:** v0.1.0 macOS arm64, terminal-distributed, local-first.
- **Verification:** 363 cargo tests passing, 14 ignored; 13 Vitest tests passing.
- **Latest Codex worktree:** compact pill-first startup, overlay modal-state reset,
  fallible session-token generation, macOS arm64 packaging/helper discovery,
  and `scripts/install.sh`.
- **Current source of truth:** read `docs/PRODUCTION-READINESS.md` and
  `docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md` before older round docs.

Older R7-R11 notes below are historical and useful for code archaeology, but
they are no longer the current release state.

### R11 deliverables shipped:
- **R10 codex fixes (4):** End-to-end UI streaming via `run_streaming(callback)` + `cue_response_chunk` events; obfstr on streaming auth header names; SwiftWhisper exact 1.2.0 pin; alignment-safe PCM16 decode with `loadUnaligned`.
- **Overlay injection hardening (7):** Production overlay-bin override gate (`BLUEY_DEV_OVERLAY=1` required); IPC session token handshake (64-hex-char via env var); native overlay token implementation (macOS Swift + Windows C); safe JSON type extractor replacing `strstr`; event state machine with UI-state allowlist (`OverlayUiState`); field length limits (question 4KB, instructions 16KB, etc.); 8 prompt-injection security tests.
- **Reconciliation:** Parallel subagent C+D merge with 2 clippy allows for readability.

### v0.1 status:
MacOS arm64 local-first GA candidate. Shipping decision is now gated by
clean-machine install validation and the user's platform-scope decision, not
by the older R7→R11 review chain.

### All checks green (R11 tip):
- `cargo fmt --all --check` ✅
- `cargo clippy --all-targets -- -D warnings` ✅
- `cargo build --all-targets` ✅
- `cargo test --all-targets` ✅ 331 pass, 10 ignored
- `cd crates/cue-dashboard/ui && npm run build` ✅
- `git -P diff --check feat/phase-3-round-10..HEAD` ✅

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

## Parallel subagent strategy (established in R6, continued in R7-R11)

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
│   │   └── src/overlay_ipc.rs            # R11 — token validation, state machine, length limits
│   ├── cue-llm/                          # R9+R10+R11 — LLM provider abstraction + streaming
│   │   └── src/{lib,router,anthropic,openai,ollama}.rs
│   ├── cue-rag/                          # R9 — RAG: chunker + vector store + embedder
│   │   └── src/{lib,chunker,store,embedder}.rs
│   ├── cue-stealth/                      # R8+R10 — process masquerading + anti-debug
│   │   └── src/{lib,macos,linux,windows}.rs
│   ├── cue-daemon/
│   │   ├── src/
│   │   │   ├── audio/
│   │   │   ├── stt/
│   │   │   │   ├── deepgram.rs           # R3+R10 — Nova-3 provider + obfstr
│   │   │   │   ├── openai.rs             # R6+R10 — OpenAI Realtime + obfstr
│   │   │   │   ├── router.rs             # R5+R7 — SttRouter 3-tier failover
│   │   │   │   ├── factory.rs            # R9(R7fix) — build_stt_chain()
│   │   │   │   ├── whisper/              # R7 — LocalWhisperProvider + parser
│   │   │   │   ├── echo.rs              # R5 — EchoProvider stub
│   │   │   │   └── mock.rs
│   │   │   ├── llm/                      # R9+R10+R11 — specialized LLMs + streaming
│   │   │   │   ├── mod.rs               # CueResponse type + ends_with_question
│   │   │   │   ├── answer.rs            # AnswerLlm (run_streaming)
│   │   │   │   ├── recap.rs             # RecapLlm (run_streaming)
│   │   │   │   └── suggest.rs           # WhatToAnswerLlm (run_streaming)
│   │   │   ├── util/
│   │   │   │   ├── mod.rs
│   │   │   │   └── rate_limiter.rs      # R9 — token bucket
│   │   │   ├── db/
│   │   │   │   └── mod.rs               # migrations 009-011
│   │   │   ├── app.rs                    # R6-R11 — live transcript + RAG + auto-recap + hotkey + overlay token
│   │   │   └── bin/
│   │   │       └── whisper_stub.rs       # R7 — test stub binary
│   │   └── tests/
│   │       ├── live_transcript_emit.rs   # R7 — 3 tests
│   │       ├── live_transcript_dedup.rs  # R9(R7fix) — 4 tests
│   │       ├── whisper_integration.rs    # R7 — 9 tests
│   │       ├── stt_factory_integration.rs # R9(R7fix) — 4 tests
│   │       ├── whisper_stub_e2e.rs       # R10 — 2 tests (ignored)
│   │       ├── auto_recap_integration.rs # R10 — 2 tests
│   │       └── streaming_chunk_emit.rs   # R11 — 5 tests
│   └── cue-dashboard/                    # Tauri + React UI
│       ├── src/
│       │   ├── lib.rs                    # R7-R10 — poller + disguise + hotkey + anti-debug
│       │   └── commands.rs               # R7-R11 — all Tauri commands (streaming chunks)
│       ├── ui/src/
│       │   ├── pages/Settings.tsx        # R8+R9 — settings with disguise + AI config
│       │   ├── pages/Responses.tsx       # R9 — AI responses page
│       │   ├── routes/Responses.tsx      # R9+R10 — streaming chunk subscription
│       │   ├── routes/LiveTranscript.tsx  # R7+R10 — Map-based dedup
│       │   ├── App.tsx                   # R10 — HotkeyListener
│       │   └── components/LiveTranscriptList.tsx  # R7 — auto-scroll list
│       └── icons/disguise/               # R8 — placeholder PNGs + README
├── native/
│   ├── macos/
│   │   ├── cue-audio/                    # Swift: ScreenCaptureKit + AVAudioEngine
│   │   ├── cue-overlay/                  # Swift: NSWindow overlay + R11 token handshake
│   │   └── cue-whisper/                  # R7+R10+R11 — real whisper.cpp (exact 1.2.0 pin)
│   └── windows/
│       ├── cue-audio/main.c              # C: WASAPI loopback
│       ├── cue-overlay/main.c            # C: Direct2D overlay + R11 token + safe JSON
│       ├── cue-overlay/json_type_extract.h  # R11 — safe type field extractor
│       └── cue-whisper/main.c            # R7+R10 — stub with model env check
├── infra/
│   ├── homebrew/bluey.rb                 # R7+R9fix — Homebrew formula
│   ├── scoop/bluey.json                  # R7+R9fix — Scoop manifest
│   └── scripts/
└── docs/work/
    ├── IMPL-PHASE-3-ROUND-{1..11}.md
    ├── FIX-PHASE-3-ROUND-{7,10}.md
    ├── PHASE-3-ROUND-{1..11}-HANDOFF-FOR-CODEX-REVIEW.md
    ├── REVIEW-PHASE-3-ROUND-{1,2,3,5,6,7,8}.md
    ├── HANDOFF-TO-CODEX-FROM-KIRO.md
    ├── AGENT-ONBOARDING.md               # this file
    ├── PLAN-STT-FALLBACK-CHAIN.md
    └── PLAN-DISTRIBUTION.md
```

## Pending work (current)

### Next rounds (user-prioritized order)

1. **R13 clean-machine validation** — install from `scripts/install.sh`, run `bluey on/off`, confirm helper discovery and pill-first overlay on a fresh Apple Silicon Mac.
2. **R13.3 sqlite-vec / ANN RAG** — replace local linear cosine scan, preserve small-dataset fallback.
3. **R13.4 Windows whisper.cpp + QA** — replace the Windows whisper stub and validate overlay/audio/page capture on Windows 10/11.
4. **R13.5 platform expansion** — macOS x86_64 first if v0.1.0 must support Intel; Linux/Windows after real QA.
5. **R14 Observability** — structured log rotation, crash reports/support bundles, long-session stress, mic hot-swap, telemetry opt-in.
6. **Cloud/commercial** — auth/device registration, sync queue, managed provider router, tenant RAG, billing/admin.

### Remaining feature backlog (post-v0.1)

- More specialized LLMs (the other 17 from natively-cluely reference)
- Function-calling / tool use in LLM requests
- Screenshot + cropper window for context injection
- Calendar / meeting-platform integration
- Bookmarks / highlights
- Session metadata (title, tags, participants)
- Multi-language whisper support
- CoreML acceleration for whisper.cpp

## First actions for a new agent

1. SSH to uno and check current state:
   ```bash
   ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25 'cd /Users/uno/Downloads/cue
   git -P status
   git -P log --oneline -n 20
   git -P branch --show-current'
   ```
2. Read these docs on uno in order:
   - `docs/PRODUCTION-READINESS.md`
   - `docs/work/HANDOFF-FROM-CODEX-TO-KIRO.md`
   - `docs/work/PHASE-3-ROUND-13-PLAN.md`
   - `docs/work/AGENT-ONBOARDING.md` (this file)
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
- **Swift build fails**: ensure Xcode CLT installed; `swift build` from `native/macos/cue-whisper/`
- **Keyring tests fail**: `secrets::tests::roundtrip` is `#[ignore]` — requires interactive Keychain access
- **Whisper model missing**: run `infra/scripts/download-whisper-model.sh` to fetch tiny.en-q5_1.bin

## Contact points

- **User's preferred flow**: SSH from divii → `scp` files up → `ssh` to run commands
- **Local staging directory**: `/tmp/` on divii (not on uno)
- **Never use `rsync`** — `scp` is the convention throughout this project

---

Give this file to the new agent alongside your first prompt.
