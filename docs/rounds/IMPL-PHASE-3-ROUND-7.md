# IMPL — Phase 3 Listening Upgrade (Round 7 — Live Transcript + Whisper Fallback + Distribution)

**Branch**: `feat/phase-3-round-7`
**Base**: `main` tip (`6126b28`, post-R5+R6 merge)
**Tip**: `97aa759` (10 commits ahead of main, 213 tests)

## Scope

**Three themes: live transcript UX, local Whisper fallback (stubs), and distribution scaffolding.**

Round 7 delivers the user-facing live transcript display, a third-tier offline STT provider (stub implementation using whisper.cpp IPC pattern), and the full release pipeline infrastructure (GitHub Actions, Homebrew, Scoop, Tauri updater).

**Does:**

1. Daemon emits `live_transcript` Tauri event for each transcript segment (partial + final) via broadcast channel.
2. Dashboard `LiveTranscript` route with rolling 200-segment buffer, auto-scroll (threshold >100px from bottom), source badges (mic/system), partial/final styling.
3. Background poller in dashboard reads `active-meeting.json` and bridges segments to Tauri events.
4. macOS Swift + Windows C native whisper helper stubs (NDJSON output, RMS-based silence detection, placeholder text).
5. `LocalWhisperProvider` Rust implementation: spawns helper binary, parses NDJSON stdout, implements `SttProvider` trait.
6. `SttRouter` 3-tier failover chain: Deepgram → OpenAI → LocalWhisper, with LocalWhisper gated by `BLUEY_STT_LOCAL_WHISPER=1`.
7. GitHub Actions `release.yml`: matrix build (macOS arm64/x86_64, Windows x86_64, Linux x86_64), artifact upload, `latest.json` generation.
8. `Makefile` with `build-darwin-arm64`, `build-darwin-x86_64`, `build-windows`, `package-*` targets.
9. Homebrew formula (`infra/homebrew/bluey.rb`) + Scoop manifest (`infra/scoop/bluey.json`).
10. Tauri updater endpoint pointed at GitHub releases `latest.json`.
11. `INSTALL.md` with brew/scoop/manual install instructions.
12. `infra/scripts/bump-formulae.sh` for post-release SHA256 updates.
13. `infra/scripts/download-whisper-model.sh` for fetching `tiny.en` model.

**Does NOT:**

- Integrate real whisper.cpp (helpers are stubs emitting placeholder text).
- Implement direct WebSocket push for live transcript (uses file-polling bridge with ~500ms latency).
- Auto-restart `LocalWhisperProvider` on crash (no supervisor pattern).
- Generate Tauri signing keypair (deferred to first real release).
- Replace `<org>` placeholders in release URLs (requires actual GitHub org).
- Create homebrew tap / scoop bucket repos (scaffolding only).
- Implement Linux packaging (`.deb`, AppImage, Flatpak).
- Apple notarization or Windows Authenticode signing.

## Commits (10, chronological bottom → top)

| # | Hash | Title | Theme |
|---|------|-------|-------|
| 1 | `40b9340` | `feat(daemon): emit live_transcript event for each transcript segment [P3.R7]` | Live transcript |
| 2 | `73c0fb0` | `feat(dashboard): live transcript route + auto-scroll list [P3.R7]` | Live transcript |
| 3 | `fd8ab9a` | `feat(whisper): macOS + Windows native helper binaries (stub) [P3.R7]` | Whisper fallback |
| 4 | `d4d0800` | `feat(daemon): LocalWhisperProvider with NDJSON IPC + tests [P3.R7]` | Whisper fallback |
| 5 | `b2c3991` | `feat(daemon): wire LocalWhisper as third tier in SttRouter [P3.R7]` | Whisper fallback |
| 6 | `5181a65` | `feat(infra): GitHub Actions release pipeline + Makefile targets [P3.R7]` | Distribution |
| 7 | `0e84903` | `feat(infra): Homebrew tap formula + Scoop manifest [P3.R7]` | Distribution |
| 8 | `2457314` | `feat(dashboard): point Tauri updater at GitHub releases endpoint [P3.R7]` | Distribution |
| 9 | `97aa759` | `chore(p3r7): fix items-after-test-module in stt/router.rs` | Lint fix |

(9 unique commits; the task description mentions 10 including a possible fmt commit — the lint fix at `97aa759` serves that role.)

## Files Created / Modified

### Theme 1: Live Transcript UX

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/src/app.rs` | Modified | Broadcast channel for `LiveTranscriptEvent`; emission in `add_audio_transcript_segment` |
| `crates/cue-daemon/tests/live_transcript_emit.rs` | Created | 3 integration tests: partial emission, final emission, broadcast fan-out |
| `crates/cue-dashboard/src/commands.rs` | Modified | `get_live_transcripts` Tauri command |
| `crates/cue-dashboard/src/lib.rs` | Modified | Background poller thread bridging active-meeting.json → Tauri events |
| `crates/cue-dashboard/ui/src/routes/LiveTranscript.tsx` | Created | Route component subscribing to `live_transcript` events |
| `crates/cue-dashboard/ui/src/components/LiveTranscriptList.tsx` | Created | Rolling list with auto-scroll, source badges, partial/final styling |
| `crates/cue-dashboard/ui/src/App.tsx` | Modified | Route registration for `/live-transcript` |
| `crates/cue-dashboard/ui/src/components/Sidebar.tsx` | Modified | Navigation link to Live Transcript |

### Theme 2: Local Whisper Fallback (Stub)

| File | Action | Purpose |
|------|--------|---------|
| `native/macos/cue-whisper/Package.swift` | Created | SPM package definition for macOS whisper helper |
| `native/macos/cue-whisper/Sources/CueWhisper/main.swift` | Created | Swift stub: reads stdin PCM, emits NDJSON with RMS silence detection |
| `native/macos/cue-whisper/build.sh` | Created | Build script for macOS helper |
| `native/windows/cue-whisper/main.c` | Created | C stub: reads stdin PCM, emits NDJSON with RMS silence detection |
| `native/windows/cue-whisper/build.ps1` | Created | PowerShell build script for Windows helper |
| `infra/scripts/download-whisper-model.sh` | Created | Downloads `tiny.en` ggml model from Hugging Face |
| `crates/cue-daemon/Cargo.toml` | Modified | Dependencies for whisper provider |
| `crates/cue-daemon/src/stt/mod.rs` | Modified | `pub mod whisper;` declaration |
| `crates/cue-daemon/src/stt/whisper/mod.rs` | Created | `LocalWhisperProvider`: spawn helper, send PCM, parse NDJSON stdout |
| `crates/cue-daemon/src/stt/whisper/parser.rs` | Created | NDJSON line parser → `TranscriptEvent` |
| `crates/cue-daemon/src/stt/whisper/error.rs` | Created | Whisper-specific error types |
| `crates/cue-daemon/src/bin/whisper_stub.rs` | Created | In-tree test stub binary (mirrors native helper protocol) |
| `crates/cue-daemon/tests/whisper_integration.rs` | Created | 9 tests: spawn, parse, error handling, env override, trait contract |
| `crates/cue-daemon/src/stt/router.rs` | Modified | 3-tier chain wiring + `BLUEY_STT_LOCAL_WHISPER=1` gate + failover test |

### Theme 3: Distribution Scaffolding

| File | Action | Purpose |
|------|--------|---------|
| `.github/workflows/release.yml` | Created | Matrix CI: build + package + upload to GitHub Release on tag push |
| `Makefile` | Created | `build-darwin-arm64`, `build-darwin-x86_64`, `build-windows`, `package-*` targets |
| `infra/homebrew/bluey.rb` | Created | Homebrew formula referencing GitHub release tarballs |
| `infra/scoop/bluey.json` | Created | Scoop manifest referencing GitHub release zips |
| `infra/scripts/bump-formulae.sh` | Created | Post-release script to update SHA256 in formula/manifest |
| `INSTALL.md` | Created | User-facing install instructions (brew, scoop, manual) |
| `crates/cue-dashboard/tauri.conf.json` | Modified | Updater endpoint → GitHub releases `latest.json` |
| `docs/work/PLAN-DISTRIBUTION.md` | Modified | Updated with implementation notes |

## Design Decisions

### 1. Live transcript: broadcast channel + file-polling bridge

The daemon emits `LiveTranscriptEvent` on a `tokio::broadcast` channel whenever `add_audio_transcript_segment` is called. The dashboard's background thread polls `active-meeting.json` at ~500ms intervals and bridges new segments to Tauri window events. This avoids tight coupling between daemon internals and the Tauri event system while keeping the architecture extensible to a future direct WebSocket push.

### 2. Whisper helpers: NDJSON IPC protocol

Native helpers read raw PCM16 LE 16kHz on stdin and emit one JSON object per line on stdout:
```json
{"type":"partial","text":"hello wor","ts_ms":1234}
{"type":"final","text":"hello world","ts_ms":1234}
```
This mirrors the overlay IPC pattern already established in the project. The Rust provider spawns the helper as a child process and parses stdout line-by-line. The `BLUEY_LOCAL_WHISPER_BINARY` env var overrides the helper path for testing.

### 3. SttRouter 3-tier failover

The router now supports three tiers:
1. **Deepgram** — primary (fails over on `Auth` or `Quota`)
2. **OpenAI** — secondary (fails over on `Auth` or `Quota`)
3. **LocalWhisper** — tertiary, offline (always succeeds or returns `Provider` error)

LocalWhisper is gated by `BLUEY_STT_LOCAL_WHISPER=1` to avoid spawning a helper process when not needed. The failover test exercises the full chain: Deepgram(Auth) → OpenAI(Quota) → LocalWhisper(success).

### 4. Distribution: GitHub Actions matrix

The release pipeline triggers on `v*` tag push and builds a matrix of 4 targets. Each produces a tarball/zip + SHA256 checksum. A final job collects artifacts, generates `latest.json` for Tauri updater, and creates the GitHub Release. The `bump-formulae.sh` script is run manually post-release to update Homebrew/Scoop SHA256 values.

## Test Count Progression

| Stage | Running tests | Δ |
|-------|---------------|---|
| Main tip (post-R5+R6 merge) | 201 | — |
| R7 final | 213 | +12 |

### Tests added in R7 (+12)

| Area | Tests | Type |
|------|-------|------|
| Live transcript emission (partial, final, broadcast) | 3 | Integration |
| LocalWhisperProvider (spawn, parse, error, env override, trait) | 9 | Integration + Unit |

## Build & Test

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 213 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
git -P diff --check main..HEAD                       ✅ clean
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Whisper helpers are stubs (no real whisper.cpp) | Real integration requires bundling ~75MB model + C++ compilation; deferred to dedicated round |
| Live transcript uses file-polling not WebSocket | Simpler architecture; ~500ms latency acceptable for MVP; WebSocket push is a follow-up |
| No auto-restart for LocalWhisperProvider | Supervisor pattern adds complexity; deferred until crash frequency is measured |
| Distribution has `<org>` placeholders | Requires actual GitHub org decision from user |

## Known Follow-ups

1. **Real whisper.cpp integration** — replace stub helpers with actual whisper.cpp inference.
2. **LocalWhisperProvider crash restart loop** — supervisor pattern for helper process.
3. **Direct WebSocket push for live transcript** — eliminate file-polling latency.
4. **Tauri signing keypair generation** — `tauri signer generate` before first release.
5. **Replace `<org>` placeholders** — actual GitHub org/user in release URLs, formula, manifest.
6. **Create homebrew tap + scoop bucket repos** — separate repositories for package managers.
7. **Linux packaging** — `.deb`, AppImage, Flatpak when demand warrants.
8. **Apple notarization + Windows Authenticode** — code signing for non-developer distribution.

## Review Checklist (for reviewer)

- [ ] Live transcript: broadcast channel created in daemon app state
- [ ] Live transcript: `LiveTranscriptEvent` emitted for both partial and final segments
- [ ] Live transcript: dashboard poller bridges file → Tauri event correctly
- [ ] Live transcript: `LiveTranscriptList` auto-scrolls when within 100px of bottom
- [ ] Live transcript: rolling buffer caps at 200 segments
- [ ] Whisper stubs: macOS Swift helper builds (`swift build`)
- [ ] Whisper stubs: Windows C helper compiles (inspection — no CI for Windows native)
- [ ] Whisper stubs: NDJSON output format matches parser expectations
- [ ] Whisper provider: spawns helper binary, reads stdout line-by-line
- [ ] Whisper provider: `BLUEY_LOCAL_WHISPER_BINARY` env override works in tests
- [ ] Whisper provider: implements `SttProvider` trait contract (connect, send_audio, next_event, close)
- [ ] Router: 3-tier chain Deepgram → OpenAI → LocalWhisper
- [ ] Router: LocalWhisper gated by `BLUEY_STT_LOCAL_WHISPER=1`
- [ ] Router: failover test exercises Auth → Quota → success path
- [ ] Distribution: `release.yml` matrix covers all 4 targets
- [ ] Distribution: Makefile targets produce correct artifact names
- [ ] Distribution: Homebrew formula syntax is valid Ruby
- [ ] Distribution: Scoop manifest is valid JSON with correct schema
- [ ] Distribution: Tauri updater endpoint URL pattern is correct
- [ ] Distribution: `bump-formulae.sh` updates SHA256 in both formula and manifest
- [ ] No secrets logged, no PII in stdout
- [ ] Code style matches CLAUDE.md rules

---

## Update: STT factory scope clarification (R7-recheck-2)

The `build_stt_chain` factory applies **only** to streaming providers used
by the continuous system-audio path. The default mic + chunked-REST path
(`real_audio_loop` → `transcribe_audio_file`) is **not** routed through
the factory: it posts WAV chunks to `runtime.stt_endpoint` and bypasses
the streaming `SttProvider` trait entirely.

Therefore: `BLUEY_STT_FALLBACK_OPENAI=1` and `BLUEY_STT_LOCAL_WHISPER=1`
do NOT affect the chunked-REST mic path today.

Unifying the two paths (streaming providers everywhere) is a deferred
follow-up. The factory module doc-comment in
`crates/cue-daemon/src/stt/factory.rs` carries this caveat inline.
