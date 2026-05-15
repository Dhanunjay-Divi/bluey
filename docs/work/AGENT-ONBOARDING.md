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

## Current state (as of 2026-05-15, post-R6 fix wave)

- **Branch:** `feat/phase-3-round-6` — 11 commits ahead of R5:
  ```
  7a9285f chore(p3r6-fix): cargo fmt across cherry-picked fixes
  b8ed87e fix(daemon): OpenAI Realtime STT uses transcription session protocol [P3.R6 fix]
  3f08371 fix(dashboard): wire permission denial from real capture errors + platform-specific Settings launchers [P3.R6 fix]
  5dbd982 fix(daemon): plumb mic device selection through AudioStart IPC to capture [P3.R6 fix]
  fb3beed fix(daemon): system-audio STT must use single provider for send + drain [P3.R5 fix2]
  1574eb2 docs(work): comprehensive handoff to codex (R5/R6 review + all pending implementation)
  18f14bc chore(p3r6): fix clippy items-after-test-module + result_large_err in openai
  9a7879b feat(daemon): OpenAI Realtime STT provider with auth + reconnect [P3.R6]
  5ef2098 feat(dashboard): permission denial UX for mic + system audio [P3.R6]
  1e55972 feat(daemon): respect mic device selection from app settings [P3.R6]
  dd0f4de feat(daemon): wire hotkey/tray events to start-stop / PTT / overlay toggle [P3.R6]
  ```
- **Test count:** 201 passing, 2 ignored (+37 vs R5 post-fix)
- **R6 deliverables shipped (end-to-end):**
  - Hotkey/tray events wired to daemon IPC (toggle listening, PTT, overlay toggle)
  - Mic device selection plumbed through `AudioStart` IPC to capture config
  - Permission denial UX: real capture errors classified → status field → dashboard poll → banner + platform-specific Settings launcher
  - OpenAI Realtime STT provider with transcription session protocol (correct event names, model, session.update)
  - System-audio STT fixed: single provider for send + drain via `tokio::select!`
- **All checks green:** fmt, clippy (-D warnings), build, cargo test (201),
  dashboard npm build, swift build (cue-overlay), git diff --check
- **Status:** Awaiting codex re-review (PHASE-3-ROUND-6-HANDOFF-FOR-CODEX-REVIEW.md submitted)

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
5. **Never log secrets** — API keys use `mask_api_key` helper pattern; keyring returns masked values to frontend
6. **Commit messages** follow Conventional Commits, e.g.:
   `feat(daemon): system audio capture via native helpers [P3.R4]`

## Parallel subagent strategy (learned from R5 → improved in R6)

**R5 problem:** 4 parallel subagents sharing a single working tree caused cherry-pick conflicts, duplicate implementations, and required a reconciliation commit.

**R6 solution:** 4 parallel subagents each used an isolated git worktree. Commits were cherry-picked onto the main branch after completion. A single `cargo fmt` commit normalized formatting. **Zero conflicts, zero reconciliation needed.** Recommend continuing this pattern for future parallel work.

## Key files + locations on uno

```
/Users/uno/Downloads/cue/
├── Cargo.toml                            # workspace root
├── crates/
│   ├── cue-core/                         # types: pcm, vad, stt, overlay_ipc, session, audio
│   ├── cue-daemon/
│   │   ├── src/
│   │   │   ├── audio/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── capture.rs            # R6 — load_mic_device_setting helper
│   │   │   │   └── system_capture.rs     # R4 — native helper launcher
│   │   │   ├── stt/
│   │   │   │   ├── mock.rs
│   │   │   │   ├── deepgram.rs           # R3 — Nova-3 provider
│   │   │   │   ├── openai.rs             # R6 — OpenAI Realtime transcription session
│   │   │   │   ├── router.rs             # R5 — SttRouter failover chain
│   │   │   │   └── echo.rs              # R5 — EchoProvider stub
│   │   │   ├── db/
│   │   │   │   ├── mod.rs               # R5 — settings KV + migrations
│   │   │   │   ├── search.rs            # R5 — FTS5 search + export
│   │   │   │   └── speakers.rs          # R5 — speaker name mapping
│   │   │   ├── secrets/
│   │   │   │   └── mod.rs               # R5 — keyring API key store
│   │   │   ├── export/
│   │   │   │   └── mod.rs               # R5 — ExportOptions re-export
│   │   │   ├── overlay.rs                # R3+4 — supervisor with restart loop
│   │   │   ├── app.rs                    # R6 — single-task select! for sys STT; mic device plumbing; permission classifier
│   │   │   └── bin/
│   │   │       ├── overlay_stub.rs
│   │   │       ├── overlay_stub_oneshot.rs
│   │   │       └── system_audio_stub.rs
│   │   └── tests/
│   │       ├── pipeline_integration.rs
│   │       ├── overlay_pipe_integration.rs
│   │       ├── overlay_restart_integration.rs
│   │       ├── system_audio_integration.rs  # R6 — single_provider_send_and_drain test
│   │       └── mic_device_selection.rs      # R6 — 5 IPC/config/DB tests
│   └── cue-dashboard/                    # Tauri + React UI
│       ├── src/
│       │   ├── lib.rs                    # R5+R6 — tray, hotkeys, updater, window intercept, poll_audio_permission
│       │   └── commands.rs               # R6 — mic device from DB, permission poll, platform launchers
│       └── ui/src/
│           ├── pages/Onboarding.tsx       # R5 — first-run flow
│           ├── routes/Search.tsx          # R5 — FTS5 search UI
│           ├── components/UpdateToast.tsx  # R5 — update notification
│           └── components/PermissionBanner.tsx  # R6 — permission denial banner
├── native/
│   ├── macos/
│   │   ├── cue-audio/                    # Swift: ScreenCaptureKit + AVAudioEngine
│   │   └── cue-overlay/                  # R5 — Swift: NSWindow overlay with stealth
│   └── windows/
│       ├── cue-audio/main.c              # C: WASAPI loopback
│       └── cue-overlay/main.c            # R4+R5: Direct2D overlay + transcript rendering
├── infra/migrations/
│   ├── 005_settings.sql                  # R5 — app_settings KV table
│   ├── 006_transcript_fts.sql            # R5 — transcripts + FTS5 + triggers
│   ├── 007_speakers.sql                  # R5 — speakers table
│   └── 008_fts_cascade_fix.sql           # R5 fix — FTS delete consistency
└── docs/work/
    ├── TEMPLATE-REVIEW.md
    ├── TEMPLATE-FIX.md
    ├── TEMPLATE-IMPL.md
    ├── IMPL-PHASE-3-ROUND-{1,2,3,4,5,6}.md
    ├── PHASE-3-ROUND-{1,2,3,4,5,6}-HANDOFF-FOR-CODEX-REVIEW.md
    ├── REVIEW-PHASE-3-ROUND-{1,2,3,5,6}.md   # codex's verdicts
    ├── HANDOFF-FROM-CODEX-TO-KIRO.md          # codex's R5/R6 handoff
    ├── AGENT-ONBOARDING.md
    └── PLAN-STT-FALLBACK-CHAIN.md
```

## Type foundations (from Round 1 — use these, don't re-create)

In `crates/cue-core/src/`:

- `pcm.rs`: `AudioSource {Microphone, System}`, `SampleRate`, `AudioChunk { source, sample_rate, samples, captured_at_ms }`
- `vad.rs`: `FrameAction {Send, SendSilence, Drop}`, `VadAggressiveness`, `VadConfig`
- `stt.rs`: `SttProvider` async trait, `TranscriptEvent {Partial, Final, SpeakerLabel}`, `ConnectionState`, `SttError`, `WordTiming`, `SttConfig`
- `overlay_ipc.rs`: `OverlayMessage` (SessionSwitched/ListeningStateChanged/TranscriptPartial/TranscriptFinal/Ping), `OverlayIpcCommand {Pong, RequestSync}`, `encode_ndjson`, `decode_ndjson`
- `audio.rs`: `AudioCaptureStatus { permission_denied_source: Option<String>, ... }`, `AudioCaptureConfig`
- `ipc.rs`: `DaemonRequest::AudioStart { enable_system, enable_microphone, mic_device_id }`

## Pending work (after R6 re-review passes)

1. **Wire OpenAI into SttRouter** — behind `BLUEY_STT_FALLBACK_OPENAI=1` with failover tests.
2. **Live transcript UX** — overlay + dashboard real-time display of system-audio transcripts.
3. **Distribution scaffolding** — GitHub Releases, auto-update endpoint, app signing.
4. **AI features** — meeting summary, action items, cues.
5. **Local whisper.cpp provider** — tier 3 offline fallback.
6. **Advanced search filters** — date range, source filter, speaker filter.
7. **Full settings page UI** — replace Placeholder with real component.
8. **Hotkey daemon IPC via Rust** — move from React listener to Rust-side for reliability.

## First actions for a new agent

1. SSH to uno and check current state:
   ```bash
   ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25 'cd /Users/uno/Downloads/cue
   git -P status
   git -P log --oneline -n 20
   git -P branch --show-current'
   ```
2. Read these docs on uno in order:
   - `docs/work/IMPL-PHASE-3-ROUND-6.md` (latest round's context)
   - `docs/work/PHASE-3-ROUND-6-HANDOFF-FOR-CODEX-REVIEW.md`
   - Any `REVIEW-PHASE-3-ROUND-6-RECHECK.md` codex has synced back
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
