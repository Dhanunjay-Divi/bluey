# Bluey / Cue — Agent Onboarding

Share this entire file with a new agent chat. It contains everything needed
to connect to uno and pick up the bluey/cue project work.

---

## Project

**bluey/cue** — A Rust+Tauri desktop app that listens to mic/system audio,
runs VAD, streams STT, and shows transcripts with live overlay.

- Planning doc (on uno): `/Users/uno/Downloads/cue/docs/work/CUE-PORT-PLAN.md`
- Current phase: **Phase 3 (Listening upgrade)** — streaming STT + overlay IPC

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

## Current state (as of 2026-05-13)

- **Main tip:** includes Round 1 + Round 2 merged
- **Active branch:** `feat/phase-3-round-3` — 2 commits, awaiting codex review
- **Test count:** 123 passing (45 core + 71 daemon lib + 3 overlay pipe + 4 pipeline integration + 1 ignored hardware)
- **All checks green:** fmt, clippy, build release, cargo test, dashboard npm build, git diff --check

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
   `feat(daemon): Deepgram Nova-3 STT + native overlay IPC [P3.R3]`

## Key files + locations on uno

```
/Users/uno/Downloads/cue/
├── Cargo.toml                            # workspace root
├── crates/
│   ├── cue-core/                         # types: pcm, vad, stt, overlay_ipc
│   ├── cue-daemon/
│   │   ├── src/
│   │   │   ├── audio/                    # capture, framer
│   │   │   ├── stt/
│   │   │   │   ├── mock.rs               # MockStt (test pattern template)
│   │   │   │   └── deepgram.rs           # Round 3 — Nova-3 provider
│   │   │   ├── overlay.rs                # Round 3 — NativeOverlayHandle
│   │   │   └── bin/overlay_stub.rs       # Round 3 — test stub binary
│   │   └── tests/
│   │       ├── pipeline_integration.rs
│   │       └── overlay_pipe_integration.rs
│   └── cue-dashboard/                    # Tauri + React UI
└── docs/work/
    ├── CUE-PORT-PLAN.md                  # master plan
    ├── TEMPLATE-REVIEW.md
    ├── TEMPLATE-FIX.md
    ├── IMPL-PHASE-3-ROUND-{1,2,3}.md
    ├── PHASE-3-ROUND-{1,2,3}-HANDOFF-FOR-CODEX-REVIEW.md
    └── REVIEW-PHASE-3-ROUND-{1,2}.md     # codex's verdicts
```

## Type foundations (from Round 1 — use these, don't re-create)

In `crates/cue-core/src/`:

- `pcm.rs`: `AudioSource {Microphone, System}`, `SampleRate`, `AudioChunk { source, sample_rate, samples, captured_at_ms }`
- `vad.rs`: `FrameAction {Send, SendSilence, Drop}`, `VadAggressiveness`, `VadConfig`
- `stt.rs`: `SttProvider` async trait, `TranscriptEvent {Partial, Final, SpeakerLabel}`, `ConnectionState`, `SttError`, `WordTiming`, `SttConfig`
- `overlay_ipc.rs`: `OverlayMessage` (SessionSwitched/ListeningStateChanged/TranscriptPartial/TranscriptFinal/Ping), `OverlayIpcCommand {Pong, RequestSync}`, `encode_ndjson`, `decode_ndjson`

## Pending — Round 4 scope (deferred from Round 3)

1. **Deepgram live `connect()`** — bind `connect_async` to the `from_channels` seam; all other pieces (URL, auth, parser, backoff, error map) are tested and ready
2. **Overlay restart-on-crash loop** — helpers ready (`restart_delay`, `Restarting { attempt }`); watcher currently single-shot
3. **System audio capture** — ScreenCaptureKit (macOS), WASAPI loopback (Windows)
4. **Real Swift/C overlay code updates** — consume `OverlayMessage::SessionSwitched`

Do **NOT** start Round 4 until codex has reviewed Round 3 and user has
acknowledged the verdict.

## First actions for a new agent

1. SSH to uno and check current state:
   ```bash
   ssh -i ~/.ssh/id_ed25519 uno@192.168.4.25 'cd /Users/uno/Downloads/cue
   git -P status
   git -P log --oneline -n 10
   git -P branch --show-current'
   ```
2. Read these docs on uno in order:
   - `docs/work/CUE-PORT-PLAN.md` (master plan)
   - The most recent `IMPL-PHASE-*-ROUND-*.md` (latest round's context)
   - The most recent `PHASE-*-ROUND-*-HANDOFF-FOR-CODEX-REVIEW.md`
   - Any `REVIEW-PHASE-*-ROUND-*.md` codex has synced back
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

## Contact points

- **User's preferred flow**: SSH from divii → `scp` files up → `ssh` to run commands
- **Local staging directory**: `/tmp/` on divii (not on uno)
- **Never use `rsync`** — `scp` is the convention throughout this project

---

Give this file to the new agent alongside your first prompt.
