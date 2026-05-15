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

## Current state (as of 2026-05-14, post-R5 implementation)

- **Branch:** `feat/phase-3-round-5` — 16 commits ahead of R4 (pending codex review):
  ```
  8ad44fe feat(dashboard): auto-update support via tauri-plugin-updater [P3.R5]
  863b44b feat(dashboard): system tray with listening + dashboard + overlay controls [P3.R5]
  4f1501d feat(dashboard): global hotkeys for listening/PTT/overlay-toggle [P3.R5]
  617ed16 fix(p3r5): reconcile parallel-subagent cherry-picks (deps + secrets module + commands wiring) [P3.R5]
  deb48fa feat(dashboard): first-run onboarding flow [P3.R5]
  e52767f feat(daemon): SttRouter with failover + EchoProvider stub [P3.R5]
  12991de feat(daemon): route system audio through parallel STT [P3.R5]
  1759ceb feat(overlay): stealth flags (hide-from-screenshare, hide-from-dock/alt-tab) [P3.R5]
  4143ce8 feat(overlay): render transcript content in macOS + Windows overlays [P3.R5]
  c4dc846 feat(overlay): macOS native overlay binary with NSWindow + protocol ABI [P3.R5]
  bb4c5f8 feat(dashboard): speaker name mapping with persistence [P3.R5]
  3b118c0 feat(dashboard): export sessions to markdown/text/json + clipboard/file [P3.R5]
  361c38e feat(daemon): FTS5 transcript search with backfill [P3.R5]
  3e3dada feat(dashboard): settings panel with STT provider + audio device + language [P3.R5]
  f832d3a feat(daemon): FTS5 transcript search with backfill [P3.R5]
  cbcdc08 feat(daemon): secure API key storage via OS keychain [P3.R5]
  ```
- **Test count:** 157 passing (+21 vs R4)
- **R5 delivered:**
  - macOS native overlay (Swift/AppKit) with transcript rendering + stealth (`sharingType = .none`)
  - Windows overlay transcript rendering + `WDA_EXCLUDEFROMCAPTURE` stealth
  - `SttRouter` with failover chain + `EchoProvider` stub as secondary
  - System audio → parallel STT routing (events produced, not yet consumed downstream)
  - Secure API key storage via OS keychain (`keyring` crate)
  - Settings panel Tauri commands (save/load settings, list audio devices)
  - First-run onboarding flow (API key → mic test → system audio opt-in)
  - FTS5 full-text search on transcripts with `snippet()` highlighting
  - Session export to markdown/text/JSON (clipboard + file)
  - Speaker name mapping with per-session persistence
  - Global hotkeys (toggle listening, PTT toggle, toggle overlay, toggle dashboard)
  - System tray with menu controls
  - Auto-update infrastructure (`tauri-plugin-updater`, placeholder endpoint)
  - Window-close intercept (hide to tray, not quit)
- **All checks green:** fmt, clippy (-D warnings), build, cargo test (157),
  dashboard npm build, swift build (cue-overlay), git diff --check

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

## Parallel subagent risks (learned from R5)

Round 5 used 4 parallel subagents sharing a single working tree on uno. This caused:
- Cherry-pick conflicts when subagents committed to the same files
- Duplicate implementations (two subagents both wrote FTS5 search)
- A reconciliation commit was needed to resolve conflicts

**For future rounds:** if parallel subagents are used again, assign non-overlapping file sets to each subagent, or use sequential execution. The reconciliation overhead was significant.

## Key files + locations on uno

```
/Users/uno/Downloads/cue/
├── Cargo.toml                            # workspace root
├── crates/
│   ├── cue-core/                         # types: pcm, vad, stt, overlay_ipc, session
│   ├── cue-daemon/
│   │   ├── src/
│   │   │   ├── audio/
│   │   │   │   ├── mod.rs
│   │   │   │   └── system_capture.rs     # R4 — native helper launcher + R5 STT routing
│   │   │   ├── stt/
│   │   │   │   ├── mock.rs
│   │   │   │   ├── deepgram.rs           # R3 — Nova-3 provider
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
│   │   │   ├── app.rs                    # Wiring: system audio, STT router
│   │   │   └── bin/
│   │   │       ├── overlay_stub.rs
│   │   │       ├── overlay_stub_oneshot.rs
│   │   │       └── system_audio_stub.rs
│   │   └── tests/
│   │       ├── pipeline_integration.rs
│   │       ├── overlay_pipe_integration.rs
│   │       ├── overlay_restart_integration.rs
│   │       └── system_audio_integration.rs
│   └── cue-dashboard/                    # Tauri + React UI
│       ├── src/
│       │   ├── lib.rs                    # R5 — tray, hotkeys, updater, window intercept
│       │   └── commands.rs               # R5 — 15+ new Tauri commands
│       └── ui/src/
│           ├── pages/Onboarding.tsx       # R5 — first-run flow
│           ├── routes/Search.tsx          # R5 — FTS5 search UI
│           └── components/UpdateToast.tsx  # R5 — update notification
├── native/
│   ├── macos/
│   │   ├── cue-audio/                    # Swift: ScreenCaptureKit + AVAudioEngine
│   │   └── cue-overlay/                  # R5 — Swift: NSWindow overlay with stealth
│   │       ├── Package.swift
│   │       └── Sources/cue-overlay/main.swift
│   └── windows/
│       ├── cue-audio/main.c              # C: WASAPI loopback
│       └── cue-overlay/main.c            # R4+R5: Direct2D overlay + transcript rendering
├── infra/migrations/
│   ├── 005_settings.sql                  # R5 — app_settings KV table
│   ├── 006_transcript_fts.sql            # R5 — transcripts + FTS5 + triggers
│   └── 007_speakers.sql                  # R5 — speakers table
└── docs/work/
    ├── TEMPLATE-REVIEW.md
    ├── TEMPLATE-FIX.md
    ├── IMPL-PHASE-3-ROUND-{1,2,3,4,5}.md
    ├── PHASE-3-ROUND-{1,2,3,4,5}-HANDOFF-FOR-CODEX-REVIEW.md
    ├── REVIEW-PHASE-3-ROUND-{1,2,3}.md   # codex's verdicts
    └── PLAN-STT-FALLBACK-CHAIN.md
```

## Type foundations (from Round 1 — use these, don't re-create)

In `crates/cue-core/src/`:

- `pcm.rs`: `AudioSource {Microphone, System}`, `SampleRate`, `AudioChunk { source, sample_rate, samples, captured_at_ms }`
- `vad.rs`: `FrameAction {Send, SendSilence, Drop}`, `VadAggressiveness`, `VadConfig`
- `stt.rs`: `SttProvider` async trait, `TranscriptEvent {Partial, Final, SpeakerLabel}`, `ConnectionState`, `SttError`, `WordTiming`, `SttConfig`
- `overlay_ipc.rs`: `OverlayMessage` (SessionSwitched/ListeningStateChanged/TranscriptPartial/TranscriptFinal/Ping), `OverlayIpcCommand {Pong, RequestSync}`, `encode_ndjson`, `decode_ndjson`

## Post-R5 next-round candidates

1. **Real secondary STT provider** — replace `EchoProvider` with OpenAI Realtime / AssemblyAI / Groq Whisper (tier 2 in fallback chain plan).
2. **System audio STT event consumption** — wire parallel STT events into session manager + overlay.
3. **Real auto-update endpoint** — GitHub Releases or custom server; replace placeholder pubkey.
4. **AI features** — meeting summary, action items, cues (the "cue" in the app name).
5. **Advanced search filters** — date range, source filter, speaker filter.
6. **Full settings page UI** — replace Placeholder with real component.
7. **Local whisper.cpp provider** — tier 3 offline fallback.
8. **App signing** — Apple Developer + Authenticode for non-terminal distribution.

## First actions for a new agent

1. SSH to uno and check current state:
   ```bash
   ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25 'cd /Users/uno/Downloads/cue
   git -P status
   git -P log --oneline -n 20
   git -P branch --show-current'
   ```
2. Read these docs on uno in order:
   - `docs/work/IMPL-PHASE-3-ROUND-5.md` (latest round's context)
   - `docs/work/PHASE-3-ROUND-5-HANDOFF-FOR-CODEX-REVIEW.md`
   - Any `REVIEW-PHASE-3-ROUND-5.md` codex has synced back
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
