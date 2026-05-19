# Phase 3 Round 7 — Handoff for Codex Review

**Branch**: `feat/phase-3-round-7`
**Base**: `main` tip (`6126b28`, post-R5+R6 merge)
**Authors**: kiro (3 parallel subagents with worktree isolation), uno (user — oversight)

## Scope

Round 7 of Phase 3. Three themes: live transcript UX, local Whisper fallback (stub implementation), and distribution scaffolding. All three were implemented by parallel subagents in isolated worktrees, then cherry-picked onto the feature branch. Pipeline green at 213 tests.

### Commits (10 ahead of main)

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

(Plus the lint fix `97aa759` which reorders items-after-test-module in router.rs.)

## Verification — ALL GREEN

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 213 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check main..HEAD                       ✅ clean
```

### Test count delta

| Tier | Main (post-R6 merge) | R7 final | Δ |
|------|---------------------|----------|---|
| Live transcript (daemon emit) | 0 | 3 | +3 |
| LocalWhisperProvider + integration | 0 | 9 | +9 |
| Previous (carried forward) | 201 | 201 | — |
| **Total running** | **201** | **213** | **+12** |

## Architecture Diagram — Round 7 Additions

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              cue-daemon                                      │
│                                                                             │
│  Mic/System Audio ──▶ Framer ──▶ TwoStageVad ──▶ SttRouter (3-tier)        │
│                                                                             │
│  ┌─── SttRouter ────────────────────────────────────────────────────────┐  │
│  │ Tier 1: Deepgram Nova-3        (failover on Auth/Quota)              │  │
│  │ Tier 2: OpenAI Realtime        (failover on Auth/Quota)              │  │
│  │ Tier 3: LocalWhisper [NEW]     (gated: BLUEY_STT_LOCAL_WHISPER=1)    │  │
│  │          └── spawns native helper (NDJSON IPC on stdin/stdout)        │  │
│  └──────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  TranscriptEvent ──▶ SessionManager                                         │
│         │                                                                   │
│         ▼ [NEW]                                                             │
│  broadcast::Sender<LiveTranscriptEvent>                                     │
│         │                                                                   │
│         ▼                                                                   │
│  active-meeting.json (file sink)                                            │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
         │
         ▼ (file polling ~500ms)
┌─────────────────────────────────────────────────────────────────────────────┐
│                           cue-dashboard                                      │
│                                                                             │
│  Background poller ──▶ Tauri event: "live_transcript"                       │
│                              │                                              │
│                              ▼                                              │
│  LiveTranscript.tsx ──▶ LiveTranscriptList.tsx                              │
│    - subscribes to event      - rolling 200-segment buffer                  │
│    - route: /live-transcript  - auto-scroll (>100px threshold)              │
│                               - source badges (mic/system)                  │
│                               - partial: italic/dim; final: normal          │
└─────────────────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────────────────┐
│                     Distribution Pipeline [NEW]                              │
│                                                                             │
│  Tag push (v*) ──▶ .github/workflows/release.yml                           │
│    ├── macOS arm64   → tar.gz + sha256                                     │
│    ├── macOS x86_64  → tar.gz + sha256                                     │
│    ├── Windows x86_64 → zip + sha256                                       │
│    └── Linux x86_64  → tar.gz + sha256                                     │
│                                                                             │
│  Release job → latest.json (Tauri updater manifest)                         │
│                                                                             │
│  Post-release: bump-formulae.sh → update Homebrew/Scoop SHA256             │
│                                                                             │
│  Install paths:                                                             │
│    brew install <org>/bluey/bluey                                           │
│    scoop bucket add bluey <org>/scoop-bluey && scoop install bluey          │
│    Manual: download from GitHub Releases                                    │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Per-Commit Review Checklist

### `40b9340` — Daemon: Live Transcript Event Emission

**What changed:** Added `tokio::broadcast` channel to daemon app state; `add_audio_transcript_segment` now emits `LiveTranscriptEvent` with source, text, is_final, and timestamp.

- [ ] Broadcast channel created with reasonable capacity (won't OOM on slow consumers)
- [ ] Both partial and final transcript events are emitted
- [ ] Event payload includes `source` (mic/system), `text`, `is_final`, `ts_ms`
- [ ] No blocking in the hot path (broadcast send is non-blocking)
- [ ] 3 integration tests cover: partial emission, final emission, multi-subscriber fan-out

### `73c0fb0` — Dashboard: Live Transcript Route + Auto-Scroll

**What changed:** New `/live-transcript` route with `LiveTranscriptList` component; background poller in `lib.rs` bridges file → Tauri events; `get_live_transcripts` command in `commands.rs`.

- [ ] Route registered in `App.tsx`
- [ ] Sidebar link added
- [ ] `LiveTranscriptList` maintains rolling buffer (max 200 segments)
- [ ] Auto-scroll engages only when user is within 100px of bottom
- [ ] Partials rendered in italic/dim; finals in normal weight
- [ ] Source badge (mic/system) displayed per segment
- [ ] Background poller reads `active-meeting.json` at ~500ms interval
- [ ] Catch-up behavior: new route mount fetches existing segments

### `fd8ab9a` — Native Whisper Helper Stubs (macOS + Windows)

**What changed:** Swift and C stub binaries that read PCM16 LE stdin, compute RMS for silence detection, and emit NDJSON placeholder transcripts on stdout.

- [ ] macOS: `Package.swift` is valid SPM; `main.swift` compiles with `swift build`
- [ ] macOS: `build.sh` produces executable
- [ ] Windows: `main.c` compiles (inspection — no Windows CI)
- [ ] Windows: `build.ps1` invokes cl.exe correctly
- [ ] Both emit NDJSON: `{"type":"partial"|"final","text":"...","ts_ms":N}`
- [ ] RMS-based silence detection (not real transcription — this is a STUB)
- [ ] `download-whisper-model.sh` fetches from Hugging Face with checksum verification

### `d4d0800` — LocalWhisperProvider with NDJSON IPC

**What changed:** New `stt/whisper/` module: `LocalWhisperProvider` spawns helper binary, pipes PCM to stdin, parses NDJSON from stdout. Includes `whisper_stub.rs` test binary.

- [ ] Implements `SttProvider` trait: `connect()`, `send_audio()`, `next_event()`, `close()`
- [ ] Spawns helper as child process with stdin/stdout pipes
- [ ] `BLUEY_LOCAL_WHISPER_BINARY` env var overrides helper path (test seam)
- [ ] NDJSON parser handles partial, final, and malformed lines gracefully
- [ ] `close()` kills child process and waits
- [ ] `whisper_stub.rs` binary mirrors the native helper protocol for testing
- [ ] 9 tests: spawn lifecycle, parse partial, parse final, malformed line, env override, trait contract, error on missing binary, close cleanup, concurrent send+receive

### `b2c3991` — Wire LocalWhisper as Third Tier in SttRouter

**What changed:** Router factory now includes `LocalWhisperProvider` as tier 3 when `BLUEY_STT_LOCAL_WHISPER=1` is set.

- [ ] Router `new()` checks env var and conditionally adds LocalWhisper
- [ ] Failover test: Deepgram returns `Auth` → OpenAI returns `Quota` → LocalWhisper succeeds
- [ ] LocalWhisper is last in chain (never fails over further)
- [ ] When env var is unset, router remains 2-tier (no behavior change for existing users)

### `5181a65` — GitHub Actions Release Pipeline + Makefile

**What changed:** New `release.yml` workflow triggered on `v*` tags; `Makefile` with build/package targets.

- [ ] Workflow triggers on `push: tags: ['v*']`
- [ ] Matrix: `macos-14` (arm64), `macos-13` (x86_64), `windows-latest`, `ubuntu-latest`
- [ ] Each job: checkout → install Rust → build → package → upload artifact
- [ ] Release job: download all artifacts → generate `latest.json` → create GitHub Release
- [ ] `latest.json` format matches Tauri updater expectations
- [ ] Makefile targets: `build-darwin-arm64`, `build-darwin-x86_64`, `build-windows`, `build-linux`, `package-darwin-arm64`, `package-darwin-x86_64`, `package-windows`, `package-linux`
- [ ] `bump-formulae.sh` accepts version + SHA256 args, updates both formula and manifest

### `0e84903` — Homebrew Formula + Scoop Manifest

**What changed:** `infra/homebrew/bluey.rb` formula and `infra/scoop/bluey.json` manifest; `INSTALL.md` with user-facing instructions.

- [ ] `bluey.rb` is valid Ruby (Homebrew formula syntax)
- [ ] Formula references correct GitHub release URL pattern
- [ ] Formula includes SHA256 placeholder (updated by `bump-formulae.sh`)
- [ ] `bluey.json` is valid JSON matching Scoop manifest schema
- [ ] Manifest references correct GitHub release URL pattern
- [ ] `INSTALL.md` covers: brew install, scoop install, manual download, building from source

### `2457314` — Tauri Updater Endpoint

**What changed:** `tauri.conf.json` updater endpoint changed from placeholder to GitHub releases URL.

- [ ] Endpoint URL pattern: `https://github.com/<org>/bluey/releases/latest/download/latest.json`
- [ ] `<org>` placeholder is documented (to be replaced before first release)
- [ ] No other `tauri.conf.json` fields changed unintentionally

### `97aa759` — Clippy Fix: items-after-test-module

**What changed:** Reordered items in `stt/router.rs` to satisfy clippy's `items-after-test-module` lint.

- [ ] Only reordering, no logic changes
- [ ] All existing router tests still pass

## Explicit Deferrals (NOT in Round 7)

1. **Real whisper.cpp integration** — helpers are stubs; actual inference requires bundling ~75MB model + C++ compilation pipeline.
2. **LocalWhisperProvider crash restart** — no supervisor pattern; if helper dies, provider returns error and router doesn't retry.
3. **Direct WebSocket push for live transcript** — file-polling bridge has ~500ms latency; acceptable for MVP.
4. **Tauri signing keypair** — `tauri signer generate` must be run before first release; private key stored as GitHub Secret.
5. **`<org>` placeholder replacement** — requires user decision on GitHub org/username.
6. **Homebrew tap + Scoop bucket repos** — formula/manifest are scaffolded but need separate repos.
7. **Linux packaging** — `.deb`, AppImage, Flatpak deferred to demand.
8. **Apple notarization + Windows Authenticode** — terminal-distributed app, not needed for developer audience.

## Verdict Request

Codex: review the 10 commits (3 themes: live transcript, whisper fallback, distribution). Write `docs/work/REVIEW-PHASE-3-ROUND-7.md` with verdict.

- 🟢 **ACCEPT** → merge R7 to main, start Round 8 (AI features: meeting summary + action items + cues)
- 🟡 **ACCEPT WITH NITS** → fold nits into Round 8
- 🔴 **REQUEST CHANGES** → kiro writes fix doc and re-hands
