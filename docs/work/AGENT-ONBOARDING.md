# Bluey / Cue — Agent Onboarding

Share this entire file with a new agent chat. It contains everything needed
to connect to uno and pick up the bluey/cue project work.

---

## Project

**bluey/cue** — A Rust+Tauri desktop app that listens to mic/system audio,
runs VAD, streams STT, and shows transcripts with live overlay.

- Current phase: **Phase 3 (Listening upgrade)** — streaming STT + overlay IPC + system audio

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

## Current state (as of 2026-05-14, post-R4 implementation)

- **Branch:** `feat/phase-3-round-4` — 4 commits ahead of main (pending codex review):
  ```
  6fc8682 feat(daemon): system audio capture via native helpers (macOS ScreenCaptureKit + Windows WASAPI) [P3.R4 stage 2]
  329f324 fix(windows): make native helper builds pass MSVC
  6f21864 feat(windows): render overlay with Direct2D and add Windows CI
  19ff43a fix(daemon): restructure overlay supervisor to own send_rx, fixes restart-loop race [P3.R4 stage 1]
  ```
- **Test count:** 136 passing (45 core + 81 daemon lib + 3 overlay pipe +
  1 overlay restart + 2 system audio + 4 pipeline integration + 1 ignored hardware)
- **R4 delivered:**
  - Overlay supervisor restructured: `send_rx` owned by supervisor, `run_one_child` with
    carryover semantics, restart-on-crash with exponential backoff (max 5 attempts)
  - System audio capture via native helpers: macOS ScreenCaptureKit (Swift, macOS 13+),
    Windows WASAPI loopback (C, MSVC-compatible)
  - Rust `SystemAudioCapture` launcher: spawns helper, reads 16 kHz mono i16 LE stdout,
    frames into 20 ms `AudioChunk`s, restart-on-crash
  - Windows overlay with Direct2D rendering + Windows CI
  - Opt-in via `BLUEY_SYSTEM_AUDIO_CONTINUOUS=1` env var
- **All checks green:** fmt, clippy (-D warnings), build, cargo test (136),
  dashboard npm build, swift build, git diff --check

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
```

## Standing rules

1. **Never spawn subagents** — user has standing rule against agent spawning
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
5. **Never log secrets** — API keys use `mask_api_key` helper pattern
6. **Commit messages** follow Conventional Commits, e.g.:
   `feat(daemon): system audio capture via native helpers [P3.R4]`

## Key files + locations on uno

```
/Users/uno/Downloads/cue/
├── Cargo.toml                            # workspace root
├── crates/
│   ├── cue-core/                         # types: pcm, vad, stt, overlay_ipc
│   ├── cue-daemon/
│   │   ├── src/
│   │   │   ├── audio/
│   │   │   │   ├── mod.rs
│   │   │   │   └── system_capture.rs     # Round 4 — native helper launcher
│   │   │   ├── stt/
│   │   │   │   ├── mock.rs
│   │   │   │   └── deepgram.rs           # Round 3 — Nova-3 provider
│   │   │   ├── overlay.rs                # Round 3+4 — supervisor with restart loop
│   │   │   ├── bin/
│   │   │   │   ├── overlay_stub.rs       # Round 3 — test stub (echo Pong)
│   │   │   │   ├── overlay_stub_oneshot.rs # Round 4 — crash-after-one-msg stub
│   │   │   │   └── system_audio_stub.rs  # Round 4 — 440Hz sine stub
│   │   │   └── app.rs                    # BLUEY_SYSTEM_AUDIO_CONTINUOUS wiring
│   │   └── tests/
│   │       ├── pipeline_integration.rs
│   │       ├── overlay_pipe_integration.rs
│   │       ├── overlay_restart_integration.rs  # Round 4
│   │       └── system_audio_integration.rs     # Round 4
│   └── cue-dashboard/                    # Tauri + React UI
├── native/
│   ├── macos/cue-audio/                  # Swift: ScreenCaptureKit + AVAudioEngine
│   │   ├── Package.swift
│   │   └── Sources/cue-audio/main.swift
│   └── windows/
│       ├── cue-audio/main.c              # C: WASAPI loopback
│       └── cue-overlay/main.c            # C: Direct2D overlay
└── docs/work/
    ├── TEMPLATE-REVIEW.md
    ├── TEMPLATE-FIX.md
    ├── IMPL-PHASE-3-ROUND-{1,2,3,4}.md
    ├── PHASE-3-ROUND-{1,2,3,4}-HANDOFF-FOR-CODEX-REVIEW.md
    ├── REVIEW-PHASE-3-ROUND-{1,2,3}.md   # codex's verdicts
    └── PLAN-STT-FALLBACK-CHAIN.md
```

## Type foundations (from Round 1 — use these, don't re-create)

In `crates/cue-core/src/`:

- `pcm.rs`: `AudioSource {Microphone, System}`, `SampleRate`, `AudioChunk { source, sample_rate, samples, captured_at_ms }`
- `vad.rs`: `FrameAction {Send, SendSilence, Drop}`, `VadAggressiveness`, `VadConfig`
- `stt.rs`: `SttProvider` async trait, `TranscriptEvent {Partial, Final, SpeakerLabel}`, `ConnectionState`, `SttError`, `WordTiming`, `SttConfig`
- `overlay_ipc.rs`: `OverlayMessage` (SessionSwitched/ListeningStateChanged/TranscriptPartial/TranscriptFinal/Ping), `OverlayIpcCommand {Pong, RequestSync}`, `encode_ndjson`, `decode_ndjson`

## Round 5+ scope (after R4 merges)

1. **STT routing for system audio** — `AudioChunk { source: System }` chunks
   currently reach the daemon channel but are not forwarded to STT. Needs a
   source-aware multiplexer or a second `SttProvider` instance.
2. **STT fallback chain** — design in `docs/work/PLAN-STT-FALLBACK-CHAIN.md`.
   Deepgram primary → secondary cloud provider → local whisper.cpp.
3. **Full overlay wiring with native helpers** — production spawn of the
   Swift/C overlay binaries via `NativeOverlayHandle` (currently tests use stubs).
4. **System audio device selection UX** — currently uses OS default endpoint.

## First actions for a new agent

1. SSH to uno and check current state:
   ```bash
   ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25 'cd /Users/uno/Downloads/cue
   git -P status
   git -P log --oneline -n 10
   git -P branch --show-current'
   ```
2. Read these docs on uno in order:
   - `docs/work/IMPL-PHASE-3-ROUND-4.md` (latest round's context)
   - `docs/work/PHASE-3-ROUND-4-HANDOFF-FOR-CODEX-REVIEW.md`
   - Any `REVIEW-PHASE-3-ROUND-4.md` codex has synced back
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
- **Swift build fails**: ensure Xcode CLT installed; `swift build` from `native/macos/cue-audio/`

## Contact points

- **User's preferred flow**: SSH from divii → `scp` files up → `ssh` to run commands
- **Local staging directory**: `/tmp/` on divii (not on uno)
- **Never use `rsync`** — `scp` is the convention throughout this project

---

Give this file to the new agent alongside your first prompt.
