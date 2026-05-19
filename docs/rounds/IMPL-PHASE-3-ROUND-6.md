# IMPL — Phase 3 Listening Upgrade (Round 6 — End-to-End Wiring After Codex Review)

**Branch**: `feat/phase-3-round-6`
**Base**: `feat/phase-3-round-5` tip (`8ad44fe`)
**Tip**: `7a9285f` (11 commits ahead of R5, 201 tests)

## Scope

**Wire R5+R6 user-facing features end-to-end after codex review caught wiring gaps.**

Round 6 shipped initial implementations of hotkey/tray daemon IPC, mic device selection, permission denial UX, and OpenAI Realtime STT. Codex's review (REVIEW-PHASE-3-ROUND-5.md + REVIEW-PHASE-3-ROUND-6.md) identified 1 R5 carryover blocker and 4 R6 blockers where features had unit tests but were not wired end-to-end. A parallel fix wave (4 subagents with worktree isolation) addressed all 5 blockers, adding 16 new tests.

**Does:**

1. Wire hotkey/tray events to daemon IPC (toggle listening, PTT, overlay toggle, dashboard show/hide).
2. Mic device selection: extend `DaemonRequest::AudioStart` with `mic_device_id: Option<String>`; dashboard reads DB before sending IPC; daemon populates `AudioCaptureConfig`.
3. Permission denial UX: classify real capture errors → `AudioCaptureStatus.permission_denied_source` → `poll_audio_permission` Tauri command → dashboard banner; platform-specific `open_privacy_settings` (macOS `open`, Windows `cmd /C start`, Linux unsupported error).
4. OpenAI Realtime STT: transcription session protocol (`?intent=transcription`, `session.update` with `input_audio_transcription`, parse `conversation.item.input_audio_transcription.{delta,completed}`, default model `gpt-4o-mini-transcribe`).
5. Fix R5 system-audio STT: single provider per session with `tokio::select!` over `send_audio` + `next_event` in one task.

**Does NOT:**

- Wire OpenAI into `SttRouter` factory chain (deferred to next round behind `BLUEY_STT_FALLBACK_OPENAI=1`).
- Implement PTT press/release (toggle only — `tauri-plugin-global-shortcut` limitation).
- Handle mic hot-swap mid-session.
- Plumb language hints to OpenAI transcription session.
- Provide word-level timing from OpenAI delta events (not available in transcription protocol).
- Wire frontend periodic poll of `poll_audio_permission` (frontend concern, backend ready).
- Support Linux `open_privacy_settings` (returns error).
- Move hotkey daemon IPC from React listener to Rust-side handler (reliability follow-up).

## Commits (11, chronological bottom → top)

| # | Hash | Title | Role |
|---|------|-------|------|
| 1 | `dd0f4de` | `feat(daemon): wire hotkey/tray events to start-stop / PTT / overlay toggle [P3.R6]` | Initial R6 feature |
| 2 | `1e55972` | `feat(daemon): respect mic device selection from app settings [P3.R6]` | Initial R6 feature |
| 3 | `5ef2098` | `feat(dashboard): permission denial UX for mic + system audio [P3.R6]` | Initial R6 feature |
| 4 | `9a7879b` | `feat(daemon): OpenAI Realtime STT provider with auth + reconnect [P3.R6]` | Initial R6 feature |
| 5 | `18f14bc` | `chore(p3r6): fix clippy items-after-test-module + result_large_err in openai` | Lint fix |
| 6 | `1574eb2` | `docs(work): comprehensive handoff to codex (R5/R6 review + all pending implementation)` | Docs (codex handoff) |
| 7 | `fb3beed` | `fix(daemon): system-audio STT must use single provider for send + drain [P3.R5 fix2]` | **Blocker fix (R5.F3)** |
| 8 | `5dbd982` | `fix(daemon): plumb mic device selection through AudioStart IPC to capture [P3.R6 fix]` | **Blocker fix (R6.2)** |
| 9 | `3f08371` | `fix(dashboard): wire permission denial from real capture errors + platform-specific Settings launchers [P3.R6 fix]` | **Blocker fix (R6.3)** |
| 10 | `b8ed87e` | `fix(daemon): OpenAI Realtime STT uses transcription session protocol [P3.R6 fix]` | **Blocker fix (R6.4)** |
| 11 | `7a9285f` | `chore(p3r6-fix): cargo fmt across cherry-picked fixes` | Formatting |

## Files Created / Modified

### Daemon — STT + audio wiring

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-daemon/src/app.rs` | Modified | Single-task `tokio::select!` for system-audio STT; mic device plumbing from IPC; permission classifier call sites |
| `crates/cue-daemon/src/stt/openai.rs` | Modified | Transcription session protocol: `?intent=transcription`, `session.update`, correct event names, `gpt-4o-mini-transcribe` default |
| `crates/cue-daemon/src/audio/capture.rs` | Modified | `load_mic_device_setting` helper |
| `crates/cue-daemon/tests/system_audio_integration.rs` | Modified | `single_provider_send_and_drain_production_wiring` replaces broken drain test |
| `crates/cue-daemon/tests/mic_device_selection.rs` | Created | 5 tests: IPC field propagation, config population, DB round-trip, serialization, backward compat |

### Core — IPC types

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-core/src/ipc.rs` | Modified | `DaemonRequest::AudioStart` + `mic_device_id: Option<String>` with `#[serde(default)]` |
| `crates/cue-core/src/audio.rs` | Modified | `AudioCaptureStatus.permission_denied_source: Option<String>` |

### Dashboard — commands + UI

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-dashboard/src/commands.rs` | Modified | `daemon_set_push_to_talk` reads mic device from DB; `poll_audio_permission` command; `open_privacy_settings` platform-specific; `load_mic_device_from_settings` helper; `privacy_settings_command()` testable extractor |
| `crates/cue-dashboard/src/lib.rs` | Modified | Register `poll_audio_permission` command |
| `crates/cue-dashboard/ui/src/components/PermissionBanner.tsx` | Created | Permission denial banner (listens to `audio_permission_denied` event) |

### CLI

| File | Action | Purpose |
|------|--------|---------|
| `crates/cue-cli/src/app.rs` | Modified | Pass `mic_device_id: None` in CLI `AudioStart` |

## Design Decisions

### 1. System-audio STT: single provider per session

**Problem:** Old code built provider A for `send_audio` and provider B for `next_event`. Audio went to A; transcripts were read from B (which received nothing).

**Fix:** One `tokio::select!` loop in a single task:
```rust
tokio::select! {
    chunk_opt = sys_rx.recv() => { provider.send_audio(&chunk) }
    event_opt = provider.next_event() => { forward to session }
}
```
The removed `drain_system_audio_stt_events` function (which built a second provider) is deleted entirely.

### 2. Mic device selection: IPC extension

Extended `DaemonRequest::AudioStart` with `mic_device_id: Option<String>` rather than having the daemon load settings itself. Rationale: the dashboard already has DB access and knows the user's intent; the daemon stays stateless about UI preferences. `#[serde(default)]` ensures backward-compatible deserialization from older CLI clients.

### 3. Permission UX: classifier → status field → poll command

Rather than emitting a one-shot Tauri event from deep in the audio loop (which could be missed if the frontend isn't mounted), the fix adds `permission_denied_source` to `AudioCaptureStatus`. The dashboard polls via `poll_audio_permission` and emits the event when the field is set. This survives webview reloads.

Platform-specific launchers:
- macOS: `open x-apple.systempreferences:com.apple.preference.security?Privacy_Microphone`
- Windows: `cmd /C start ms-settings:privacy-microphone`
- Linux: returns `Err("not supported on Linux")`

### 4. OpenAI Realtime: transcription session protocol

**Problem:** Provider used legacy `response.audio_transcript.*` events and a conversation model.

**Fix:**
- Connect URL includes `?intent=transcription`
- After WebSocket handshake, send `session.update` with `input_audio_transcription: { model: "gpt-4o-mini-transcribe" }`
- Parse `conversation.item.input_audio_transcription.delta` → `TranscriptEvent::Partial`
- Parse `conversation.item.input_audio_transcription.completed` → `TranscriptEvent::Final`
- Old event names are silently ignored (forward-compatible)
- Default model changed from `gpt-4o-realtime-preview` to `gpt-4o-mini-transcribe`

## Test Count Progression

| Stage | Running tests | Δ |
|-------|---------------|---|
| R5 final (post-fix) | 164 | — |
| R6 first pass (pre-review) | 185 | +21 |
| R6 final (post-fix wave) | 201 | +16 |

### Tests added in fix wave (+16)

| Area | Tests | Type |
|------|-------|------|
| System-audio single-provider wiring | 1 (replaces broken test) | Integration |
| Mic device selection (IPC, config, DB, serde, backward compat) | 5 | Integration |
| Permission denial (classifier call sites, status field, command builder) | 4 | Unit |
| OpenAI transcription session (session.update, delta, completed, old-events-ignored, model default) | 6 | Unit |

## Build & Test

```
cargo fmt --all --check                              ✅ pass
cargo clippy --all-targets -- -D warnings            ✅ pass
cargo build --all-targets                            ✅ pass
cargo test --all-targets                             ✅ 201 pass, 2 ignored
cd crates/cue-dashboard/ui && npm run build          ✅ pass
swift build (native/macos/cue-overlay)               ✅ pass
git -P diff --check feat/phase-3-round-5..HEAD       ✅ clean
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| OpenAI not wired into SttRouter factory | Codex recommended fixing provider correctness first, then wiring behind feature flag in next round |
| Parallel fix agents used worktrees instead of shared working tree | Learned from R5 file-stomping chaos; worktree isolation prevented conflicts entirely |
| `drain_system_audio_stt_events` deleted entirely | The function was architecturally wrong (built a second provider); replaced with inline `select!` loop |

## Known Follow-ups

1. **Hotkey daemon IPC via Rust-side handlers** — currently routed through React `HotkeyListener`; Rust-side would survive webview crashes/reloads.
2. **PTT press/release** — toggle only today; `tauri-plugin-global-shortcut` doesn't expose key-up.
3. **Mic hot-swap mid-session** — changing device requires stopping and restarting capture.
4. **OpenAI word-level timing** — transcription delta events don't include word timestamps.
5. **OpenAI language hint plumbing** — `input_audio_transcription.language` not yet configurable from settings.
6. **Linux `open_privacy_settings`** — returns error; needs desktop-environment-specific handling.
7. **Frontend `poll_audio_permission` periodic call** — backend ready, frontend wiring is a UI task.
8. **Wire OpenAI into SttRouter** — behind `BLUEY_STT_FALLBACK_OPENAI=1` with failover tests.

## Review Checklist (for reviewer)

- [ ] R5.F3 fix: single provider instance for both send and drain in system-audio task
- [ ] R5.F3 fix: production-wiring test exercises the real `select!` pattern
- [ ] R6.2 fix: `DaemonRequest::AudioStart.mic_device_id` flows to `AudioCaptureConfig`
- [ ] R6.2 fix: dashboard reads DB before sending IPC
- [ ] R6.2 fix: backward-compat deserialization (missing field → None)
- [ ] R6.3 fix: classifiers called from real audio loop error paths
- [ ] R6.3 fix: `permission_denied_source` in `AudioCaptureStatus`
- [ ] R6.3 fix: platform-specific launcher commands (macOS/Windows/Linux)
- [ ] R6.4 fix: `?intent=transcription` in connect URL
- [ ] R6.4 fix: `session.update` sent after handshake
- [ ] R6.4 fix: correct event names (`conversation.item.input_audio_transcription.*`)
- [ ] R6.4 fix: default model is `gpt-4o-mini-transcribe`
- [ ] R6.4 fix: old event names silently ignored
- [ ] No secrets logged, no PII in stdout
- [ ] Code style matches CLAUDE.md rules
