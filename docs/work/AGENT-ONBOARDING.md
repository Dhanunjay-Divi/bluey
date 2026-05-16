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

## Current state (as of 2026-05-16, post-R10 implementation)

- **main tip:** `6126b28` — R3-R6 merged, 201 tests passing
- **Branch `feat/phase-3-round-7`:** 10 commits ahead of main, 213 tests — awaiting codex final verdict (R7-fix-3 recheck: `a5991b2`)
- **Branch `feat/phase-3-round-8`:** 7 commits ahead of R7, 224 tests — 🟡 accepted with nits
- **Branch `feat/phase-3-round-9`:** 16 commits ahead of R8, 284 tests — awaiting codex review
- **Branch `feat/phase-3-round-10`:** 12 commits ahead of R9, **299 tests** — current, awaiting codex review

### R10 deliverables shipped:
- **AI hookup completion:** Cmd+Shift+A global shortcut → `request_cue` command → question-detect → AnswerLlm/WhatToAnswerLlm dispatch → persist + emit. Auto-recap on session end via `spawn_auto_recap()`. Whisper-stub e2e factory test. Live transcript dedup by `{session_id, index}` map keys.
- **Streaming LLM responses:** `LlmProvider` trait gains `complete_stream()` returning `LlmChunkStream`. Anthropic SSE (`content_block_delta`), OpenAI SSE (`chat.completion.chunk`), Ollama NDJSON. Default fallback wraps `complete()`. Dashboard Responses route subscribes to `cue_response_chunk` with typing indicator.
- **Hardening basics:** Anti-debug via PT_DENY_ATTACH (macOS), IsDebuggerPresent + watchdog (Windows), TracerPid (Linux). obfstr for API endpoint URLs + auth header names — `strings` grep verified zero matches in release binary.
- **Real whisper.cpp on macOS:** SwiftWhisper v1.2.0 (bundles whisper.cpp source), `whisper_full()` C API, RMS silence gate, NDJSON ABI preserved. Model: `tiny.en-q5_1.bin` (31 MB). Windows kept as stub.

### v0.1 alpha status:
Feature-complete. All core product functionality implemented across R7-R10. Shipping over the weekend pending codex review chain acceptance.

### All checks green (R10 tip):
- `cargo fmt --all --check` ✅
- `cargo clippy --all-targets -- -D warnings` ✅
- `cargo build --all-targets` ✅
- `cargo test --all-targets` ✅ 299 pass, 10 ignored
- `cd crates/cue-dashboard/ui && npm run build` ✅
- `git -P diff --check feat/phase-3-round-9..HEAD` ✅

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

## Parallel subagent strategy (established in R6, continued in R7-R10)

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
│   ├── cue-llm/                          # R9+R10 — LLM provider abstraction + router + streaming
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
│   │   │   ├── llm/                      # R9+R10 — specialized LLMs + streaming
│   │   │   │   ├── mod.rs               # CueResponse type + ends_with_question
│   │   │   │   ├── answer.rs            # AnswerLlm (uses complete_stream)
│   │   │   │   ├── recap.rs             # RecapLlm (uses complete_stream)
│   │   │   │   └── suggest.rs           # WhatToAnswerLlm (uses complete_stream)
│   │   │   ├── util/
│   │   │   │   ├── mod.rs
│   │   │   │   └── rate_limiter.rs      # R9 — token bucket
│   │   │   ├── db/
│   │   │   │   └── mod.rs               # migrations 009-011
│   │   │   ├── app.rs                    # R6-R10 — live transcript + RAG + auto-recap + hotkey
│   │   │   └── bin/
│   │   │       └── whisper_stub.rs       # R7 — test stub binary
│   │   └── tests/
│   │       ├── live_transcript_emit.rs   # R7 — 3 tests
│   │       ├── live_transcript_dedup.rs  # R9(R7fix) — 4 tests
│   │       ├── whisper_integration.rs    # R7 — 9 tests
│   │       ├── stt_factory_integration.rs # R9(R7fix) — 4 tests
│   │       ├── whisper_stub_e2e.rs       # R10 — 2 tests (ignored)
│   │       └── auto_recap_integration.rs # R10 — 2 tests
│   └── cue-dashboard/                    # Tauri + React UI
│       ├── src/
│       │   ├── lib.rs                    # R7-R10 — poller + disguise + hotkey + anti-debug
│       │   └── commands.rs               # R7-R10 — all Tauri commands
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
│   │   ├── cue-overlay/                  # Swift: NSWindow overlay
│   │   └── cue-whisper/                  # R7+R10 — real whisper.cpp via SwiftWhisper
│   └── windows/
│       ├── cue-audio/main.c              # C: WASAPI loopback
│       ├── cue-overlay/main.c            # C: Direct2D overlay
│       └── cue-whisper/main.c            # R7+R10 — stub with model env check
├── infra/
│   ├── homebrew/bluey.rb                 # R7+R9fix — Homebrew formula
│   ├── scoop/bluey.json                  # R7+R9fix — Scoop manifest
│   └── scripts/
└── docs/work/
    ├── IMPL-PHASE-3-ROUND-{1..10}.md
    ├── FIX-PHASE-3-ROUND-7.md
    ├── PHASE-3-ROUND-{1..10}-HANDOFF-FOR-CODEX-REVIEW.md
    ├── REVIEW-PHASE-3-ROUND-{1,2,3,5,6,7,8}.md
    ├── HANDOFF-TO-CODEX-FROM-KIRO.md
    ├── AGENT-ONBOARDING.md               # this file
    ├── PLAN-STT-FALLBACK-CHAIN.md
    └── PLAN-DISTRIBUTION.md
```

## Pending work (after R7-R10 review chain passes → v0.1 alpha ships)

### Next rounds (user-prioritized order)

1. **R11: sqlite-vec swap + native overlay passthrough + multi-provider embedding** — replace in-memory cosine with native ANN; macOS Swift `window.ignoresMouseEvents` + Windows C `WS_EX_TRANSPARENT`; Ollama/local ONNX embedding support.
2. **R12: Hardening deep** — Tauri signing keypair, mlock for API keys, SQLCipher migration, anti-RE deeper measures, Windows real anti-debug with process termination.
3. **R13: Observability** — structured logging (`tracing-appender` file rotation), crash reporting (panic dump files), long-session stress tests, mic hot-swap mid-session.
4. **R14: Distribution publishing** — homebrew tap repo, scoop bucket repo, first tagged release, GitHub release automation end-to-end.
5. **R15: Polish** — production icons, README overhaul, privacy policy, onboarding videos, telemetry opt-in.

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
   - `docs/work/IMPL-PHASE-3-ROUND-10.md` (latest round's context)
   - `docs/work/PHASE-3-ROUND-10-HANDOFF-FOR-CODEX-REVIEW.md`
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
