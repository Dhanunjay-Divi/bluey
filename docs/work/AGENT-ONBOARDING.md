# Bluey / Cue — Agent Onboarding

Share this entire file with a new agent chat. It contains everything needed
to connect to uno and pick up the bluey/cue project work.

---

## Project

**bluey/cue** — A Rust+Tauri desktop app that listens to mic/system audio,
runs VAD, streams STT, and shows transcripts with live overlay.

- Current phase: **Phase 3 (Listening upgrade)** — streaming STT + overlay IPC + system audio + user-facing alpha features

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

## Current state (as of 2026-05-15, post-R6 merge + R7 implementation)

- **main tip:** `6126b28` — R3-R6 merged, 201 tests passing
- **Branch:** `feat/phase-3-round-7` — 10 commits ahead of main:
  ```
  97aa759 chore(p3r7): fix items-after-test-module in stt/router.rs
  2457314 feat(dashboard): point Tauri updater at GitHub releases endpoint [P3.R7]
  0e84903 feat(infra): Homebrew tap formula + Scoop manifest [P3.R7]
  5181a65 feat(infra): GitHub Actions release pipeline + Makefile targets [P3.R7]
  b2c3991 feat(daemon): wire LocalWhisper as third tier in SttRouter [P3.R7]
  d4d0800 feat(daemon): LocalWhisperProvider with NDJSON IPC + tests [P3.R7]
  fd8ab9a feat(whisper): macOS + Windows native helper binaries (stub) [P3.R7]
  73c0fb0 feat(dashboard): live transcript route + auto-scroll list [P3.R7]
  40b9340 feat(daemon): emit live_transcript event for each transcript segment [P3.R7]
  ```
- **Test count:** 213 passing, 2 ignored (+12 vs main)
- **Status:** Awaiting codex review (PHASE-3-ROUND-7-HANDOFF-FOR-CODEX-REVIEW.md submitted)

### R7 deliverables shipped:
- **Live transcript UX:** daemon broadcast channel → file-polling bridge → Tauri event → LiveTranscript route with rolling 200-segment buffer + auto-scroll
- **Local Whisper fallback (stub):** macOS Swift + Windows C helper stubs (NDJSON IPC); `LocalWhisperProvider` Rust impl; SttRouter 3-tier chain (Deepgram → OpenAI → LocalWhisper) gated by `BLUEY_STT_LOCAL_WHISPER=1`
- **Distribution scaffolding:** GitHub Actions release pipeline (matrix build), Makefile targets, Homebrew formula, Scoop manifest, Tauri updater endpoint, INSTALL.md, bump-formulae.sh

### All checks green:
- `cargo fmt --all --check` ✅
- `cargo clippy --all-targets -- -D warnings` ✅
- `cargo build --all-targets` ✅
- `cargo test --all-targets` ✅ 213 pass
- `cd crates/cue-dashboard/ui && npm run build` ✅
- `git -P diff --check main..HEAD` ✅

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

## Parallel subagent strategy (established in R6, continued in R7)

Each parallel subagent uses an isolated git worktree. Commits are cherry-picked onto the main feature branch after completion. A single lint/fmt commit normalizes formatting. **Zero conflicts, zero reconciliation needed.** Continue this pattern for future parallel work.

## Key files + locations on uno

```
/Users/uno/Downloads/cue/
├── Cargo.toml                            # workspace root
├── Makefile                              # R7 — build/package targets
├── INSTALL.md                            # R7 — user-facing install instructions
├── .github/workflows/release.yml         # R7 — release pipeline
├── crates/
│   ├── cue-core/                         # types: pcm, vad, stt, overlay_ipc, session, audio
│   ├── cue-daemon/
│   │   ├── src/
│   │   │   ├── audio/
│   │   │   ├── stt/
│   │   │   │   ├── deepgram.rs           # R3 — Nova-3 provider
│   │   │   │   ├── openai.rs             # R6 — OpenAI Realtime transcription
│   │   │   │   ├── router.rs             # R5+R7 — SttRouter 3-tier failover
│   │   │   │   ├── whisper/              # R7 — LocalWhisperProvider + parser
│   │   │   │   ├── echo.rs              # R5 — EchoProvider stub
│   │   │   │   └── mock.rs
│   │   │   ├── app.rs                    # R6+R7 — live transcript broadcast
│   │   │   └── bin/
│   │   │       └── whisper_stub.rs       # R7 — test stub binary
│   │   └── tests/
│   │       ├── live_transcript_emit.rs   # R7 — 3 tests
│   │       └── whisper_integration.rs    # R7 — 9 tests
│   └── cue-dashboard/                    # Tauri + React UI
│       ├── src/
│       │   ├── lib.rs                    # R7 — background poller for live transcript
│       │   └── commands.rs               # R7 — get_live_transcripts command
│       └── ui/src/
│           ├── routes/LiveTranscript.tsx  # R7 — live transcript route
│           └── components/LiveTranscriptList.tsx  # R7 — auto-scroll list
├── native/
│   ├── macos/
│   │   ├── cue-audio/                    # Swift: ScreenCaptureKit + AVAudioEngine
│   │   ├── cue-overlay/                  # Swift: NSWindow overlay
│   │   └── cue-whisper/                  # R7 — Swift whisper helper stub
│   └── windows/
│       ├── cue-audio/main.c              # C: WASAPI loopback
│       ├── cue-overlay/main.c            # C: Direct2D overlay
│       └── cue-whisper/main.c            # R7 — C whisper helper stub
├── infra/
│   ├── homebrew/bluey.rb                 # R7 — Homebrew formula
│   ├── scoop/bluey.json                  # R7 — Scoop manifest
│   ├── scripts/
│   │   ├── bump-formulae.sh             # R7 — post-release SHA256 updater
│   │   └── download-whisper-model.sh    # R7 — model downloader
│   └── migrations/
└── docs/work/
    ├── IMPL-PHASE-3-ROUND-{1..7}.md
    ├── PHASE-3-ROUND-{1..7}-HANDOFF-FOR-CODEX-REVIEW.md
    ├── REVIEW-PHASE-3-ROUND-{1,2,3,5,6}.md
    ├── HANDOFF-TO-CODEX-FROM-KIRO.md     # R7 review + pending work
    ├── HANDOFF-FROM-CODEX-TO-KIRO.md     # (stale — codex will overwrite)
    ├── AGENT-ONBOARDING.md               # this file
    ├── PLAN-STT-FALLBACK-CHAIN.md
    └── PLAN-DISTRIBUTION.md
```

## Pending work (after R7 review passes)

### Next rounds (priority order)

1. **Real whisper.cpp integration** — replace stub helpers with actual whisper.cpp inference; bundle `tiny.en` model (~75MB)
2. **LocalWhisperProvider crash restart loop** — supervisor pattern for helper process
3. **AI features (the "cue" differentiator)** — LLM-powered meeting summary, action items, live suggestions, custom cues. NOT YET STARTED.
4. **Structured logging + crash reporting** — `tracing-appender` file rotation + panic dump files
5. **Long-session stress tests** — `#[ignore]` test running 5-minute synthetic pipeline
6. **Bookmarks / highlights** — SQLite migration + keyboard shortcut + transcript markers
7. **Session metadata** — title, tags, participants, notes
8. **Mic device hot-swap mid-session** — detect disappearance, pause, emit UI event
9. **Auto-update endpoint signing keys** — `tauri signer generate` + GitHub Secrets setup

## First actions for a new agent

1. SSH to uno and check current state:
   ```bash
   ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25 'cd /Users/uno/Downloads/cue
   git -P status
   git -P log --oneline -n 20
   git -P branch --show-current'
   ```
2. Read these docs on uno in order:
   - `docs/work/IMPL-PHASE-3-ROUND-7.md` (latest round's context)
   - `docs/work/PHASE-3-ROUND-7-HANDOFF-FOR-CODEX-REVIEW.md`
   - `docs/work/HANDOFF-TO-CODEX-FROM-KIRO.md` (full task assignment)
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
