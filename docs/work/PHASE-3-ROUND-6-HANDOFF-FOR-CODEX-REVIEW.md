# Phase 3 Round 6 — Handoff for Codex Re-Review (Post-Fix)

**Branch**: `feat/phase-3-round-6`
**Base**: `feat/phase-3-round-5` tip (`8ad44fe`)
**Authors**: kiro (4 parallel fix subagents with worktree isolation), uno (user — oversight)

## Scope

Round 6 of Phase 3. Wires R5+R6 user-facing features end-to-end after codex review caught wiring gaps. Addresses 1 R5 carryover blocker (system-audio STT dual-provider) and 4 R6 blockers (mic device selection not plumbed, permission denial not emitted from real capture, OpenAI Realtime using wrong protocol, missing docs).

### Commits (11 ahead of R5)

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

## Verification — ALL GREEN

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 201 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
swift build (native/macos/cue-overlay)               ✅ pass
git -P diff --check feat/phase-3-round-5..HEAD       ✅ clean
```

### Test count delta

| Tier | Round 5 (post-fix) | R6 first pass | R6 final (post-fix) | Δ (R6 final vs R5) |
|------|-------------------|---------------|---------------------|---------------------|
| cue-core lib | 47 | 47 | 47 | — |
| cue-daemon lib | 98 | 112 | 127 | +29 |
| Integration tests | 11 | 18 | 18 | +7 |
| Dashboard commands | 3 | 3 | 4 | +1 |
| Mic device selection | 0 | 0 | 5 | +5 |
| Ignored (hardware/keychain) | 2 | 2 | 2 | — |
| **Total running** | **164** | **185** | **201** | **+37** |

## Architecture Diagram — Round 6 End-to-End Wiring

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              cue-daemon                                      │
│                                                                             │
│  Mic ──▶ Framer ──▶ TwoStageVad ──▶ ┌─────────────────────────────────┐    │
│   ▲                                  │ SttRouter (BLUEY_STT_ROUTER=1)  │    │
│   │                                  │   providers[0]: Deepgram        │    │
│   │ mic_device_id from IPC           │   providers[1]: EchoProvider    │    │
│   │ (dashboard reads DB,             │   failover on Auth/Quota        │    │
│   │  passes via AudioStart)          └──────────────┬──────────────────┘    │
│   │                                                 │                       │
│   │                                                 ▼                       │
│   │                                      TranscriptEvent → SessionManager   │
│   │                                                 │                       │
│  SystemAudioCapture ──▶ ┌───────────────────────────┴──────────────────┐   │
│    (native helper)      │ SINGLE-TASK tokio::select! loop              │   │
│    16kHz mono i16 LE    │   sys_rx.recv() → provider.send_audio()      │   │
│                         │   provider.next_event() → session transcript │   │
│                         │   (ONE provider instance for both paths)      │   │
│                         └──────────────────────────────────────────────┘   │
│                                                                             │
│  ┌─── OpenAI Realtime STT ──────────────────────────────────────────────┐  │
│  │ wss://api.openai.com/v1/realtime?intent=transcription                 │  │
│  │ → session.update { input_audio_transcription: { model } }             │  │
│  │ → input_audio_buffer.append (base64 PCM)                              │  │
│  │ ← conversation.item.input_audio_transcription.delta → Partial         │  │
│  │ ← conversation.item.input_audio_transcription.completed → Final       │  │
│  │ Default model: gpt-4o-mini-transcribe                                 │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌─── Permission Denial Path ───────────────────────────────────────────┐  │
│  │ real_audio_loop error → is_permission_denied_message() classifier     │  │
│  │ → AudioCaptureStatus.permission_denied_source = Some("mic"|"system")  │  │
│  │ → dashboard polls via poll_audio_permission command                    │  │
│  │ → emits audio_permission_denied Tauri event → PermissionBanner.tsx    │  │
│  │ → open_privacy_settings: macOS open / Windows cmd /C start            │  │
│  └───────────────────────────────────────────────────────────────────────┘  │
│                                                                             │
│  ┌─── Hotkey/Tray → Daemon IPC ────────────────────────────────────────┐   │
│  │ Shortcut event → React HotkeyListener → Tauri command               │   │
│  │   daemon_toggle_listening → MeetingStart/MeetingEnd                  │   │
│  │   daemon_set_push_to_talk → AudioStart(mic_device_id)/AudioStop      │   │
│  │   overlay toggle → OverlayShow/OverlayHide                          │   │
│  └──────────────────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────────────────┘
```

## Per-Commit Review Checklist

### `fb3beed` — R5.F3 Fix: Single-Provider System-Audio STT

**What changed:** Replaced broken two-task wiring (provider A for send, provider B for drain) with a single-task `tokio::select!` loop using ONE provider instance.

- [ ] `drain_system_audio_stt_events` function is deleted entirely
- [ ] Single `tokio::select!` loop in system-audio task handles both `sys_rx.recv()` and `provider.next_event()`
- [ ] Non-retryable errors break the loop cleanly
- [ ] Provider is closed after loop exit
- [ ] Test `single_provider_send_and_drain_production_wiring` exercises the real pattern: inject chunks via channel → assert transcripts reach downstream
- [ ] Old test `stt_event_drain_forwards_finals_and_handles_errors` is removed (it manually seeded a separate MockStt)

### `5dbd982` — R6.2 Fix: Mic Device Selection Plumbing

**What changed:** Extended `DaemonRequest::AudioStart` with `mic_device_id: Option<String>`; dashboard reads DB before sending IPC; daemon populates `AudioCaptureConfig.microphone.device_id`.

- [ ] `cue-core/src/ipc.rs`: `mic_device_id: Option<String>` with `#[serde(default)]`
- [ ] `cue-daemon/src/app.rs`: handler destructures `mic_device_id` and sets `config.microphone.device_id`
- [ ] `cue-dashboard/src/commands.rs`: `daemon_set_push_to_talk` calls `load_mic_device_from_settings` before IPC
- [ ] `cue-cli/src/app.rs`: passes `mic_device_id: None` (system default)
- [ ] 5 tests: IPC field propagation, config population, DB round-trip, serialization, backward compat (missing field → None)
- [ ] Backward-compat: JSON without `mic_device_id` deserializes to `None`

### `3f08371` — R6.3 Fix: Permission Denial from Real Capture + Platform Launchers

**What changed:** Wired classifiers into real audio loop; added `permission_denied_source` to status; platform-specific `open_privacy_settings`.

- [ ] `cue-daemon/src/app.rs`: capture error path calls `is_permission_denied_message()` / `is_system_audio_permission_denied_message()`
- [ ] `cue-core/src/audio.rs`: `AudioCaptureStatus.permission_denied_source: Option<String>`
- [ ] `cue-dashboard/src/commands.rs`: `poll_audio_permission` checks daemon status, emits event if field set
- [ ] `open_privacy_settings`: macOS → `open x-apple.systempreferences:...`, Windows → `cmd /C start ms-settings:...`, Linux → error
- [ ] `privacy_settings_command()` extracted for testability (no process spawn in test)
- [ ] Tests cover command building for each platform and `permission_denied_source` field

### `b8ed87e` — R6.4 Fix: OpenAI Realtime Transcription Session Protocol

**What changed:** Rewrote OpenAI provider to use the current transcription session protocol instead of legacy response events.

- [ ] Connect URL includes `?intent=transcription`
- [ ] After handshake, sends `session.update` with `input_audio_transcription: { model: "gpt-4o-mini-transcribe" }`
- [ ] Parses `conversation.item.input_audio_transcription.delta` → `TranscriptEvent::Partial`
- [ ] Parses `conversation.item.input_audio_transcription.completed` → `TranscriptEvent::Final`
- [ ] Old event names (`response.audio_transcript.*`) are silently ignored (no panic, no error)
- [ ] Default model changed from `gpt-4o-realtime-preview` to `gpt-4o-mini-transcribe`
- [ ] Mock-WS tests assert: session.update frame content, delta → Partial, completed → Final, old events ignored, model default

## Parallel Fix Strategy (Worktree Isolation)

Round 6 fixes were implemented by 4 parallel subagents, each in an isolated git worktree. This **prevented** the file-stomping chaos that hit Round 5 (where 4 agents shared one working tree). Each agent committed to its own worktree branch, then commits were cherry-picked onto `feat/phase-3-round-6`. A single `cargo fmt` commit at the top normalizes formatting.

## Explicit Deferrals (NOT in Round 6)

1. **Wire OpenAI into SttRouter factory** — provider is correct but not yet in the failover chain; needs `BLUEY_STT_FALLBACK_OPENAI=1` gating + ordering tests.
2. **Hotkey daemon IPC via Rust-side handlers** — currently routed through React `HotkeyListener`; Rust-side would survive webview crashes.
3. **PTT press/release** — toggle only; `tauri-plugin-global-shortcut` limitation.
4. **Mic hot-swap mid-session** — requires stop + restart capture.
5. **OpenAI word-level timing** — delta events don't include timestamps.
6. **OpenAI language hint** — `input_audio_transcription.language` not configurable from settings yet.
7. **Linux `open_privacy_settings`** — returns error; needs DE-specific handling.
8. **Frontend `poll_audio_permission` periodic call** — backend ready, frontend wiring is a UI task.
9. **Live transcript UX** — deferred per codex's "fix correctness first" guidance.
10. **Distribution scaffolding** — deferred until runtime correctness is stable.

## Verdict Request

Codex: re-review the 11 commits (especially the 4 blocker fixes + R5.F3 carryover fix). Write `docs/work/REVIEW-PHASE-3-ROUND-6-RECHECK.md` with verdict.

- 🟢 **ACCEPT** → merge R5+R6 to main, start Round 7 (OpenAI router wiring + live transcript UX + distribution)
- 🟡 **ACCEPT WITH NITS** → fold nits into Round 7
- 🔴 **REQUEST CHANGES** → kiro writes fix doc and re-hands
