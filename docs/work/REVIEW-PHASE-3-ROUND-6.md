# REVIEW: Phase 3 Round 6 — Hotkeys, Mic Selection, Permission UX, OpenAI Realtime STT

**Commit range:** `77f5e9e..1574eb2`
**Reviewer:** Codex
**Date:** 2026-05-15

## Per-Task Review

### P3.R6.1 — Hotkey / Tray Events to Daemon Actions

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/ui/src/App.tsx` |
| Verdict | 🟡 minor nit |

**Findings:**
- ✅ Tray and shortcut events are now forwarded to Tauri commands, and those commands use daemon TCP IPC instead of being pure UI events.
- ✅ Dashboard show/hide and overlay toggle paths are straightforward and low risk.
- 🟡 The hotkey action path depends on the React `HotkeyListener` being mounted (`crates/cue-dashboard/ui/src/App.tsx:27`). Since Tauri already receives the shortcut in Rust, direct Rust-side daemon IPC would be more robust when the webview is hidden, crashed, or still booting. This is not a blocker for the current tray-window lifecycle, but it is a reliability follow-up.
- 🟡 `daemon_toggle_listening` toggles meeting start/end, while `daemon_set_push_to_talk` toggles audio start/stop. That distinction should be surfaced in UX/docs so "listening" does not imply microphone capture has started.

### P3.R6.2 — Mic Device Selection Persistence and Capture Plumbing

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/audio/capture.rs`, `crates/cue-daemon/src/app.rs`, `crates/cue-dashboard/src/commands.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The setting is not actually plumbed into daemon capture. `load_mic_device_setting` is defined and tested (`crates/cue-daemon/src/audio/capture.rs:233`), but there are no production call sites. `DaemonRequest::AudioStart` still only carries booleans (`crates/cue-core/src/ipc.rs:70`), and `start_audio_capture` uses the request config it is given without loading `audio.mic_device` from the settings DB.
- 🔴 The dashboard can save arbitrary settings, but pressing the tray/hotkey audio path does not pass the selected device into `AudioCaptureConfig.microphone.device_id`.
- Required fix: either extend `AudioStart` with device ids/provider settings, or have the daemon load persisted audio settings before building `AudioCaptureConfig`. Add a test that saves `audio.mic_device`, starts capture config resolution, and asserts the selected device is used or matched.

### P3.R6.3 — Permission Denial UX

| Field | Value |
|-------|-------|
| Files | `crates/cue-core/src/audio.rs`, `crates/cue-daemon/src/audio/capture.rs`, `crates/cue-daemon/src/audio/system_capture.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/ui/src/components/PermissionBanner.tsx` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The dashboard banner is only driven by the test command `emit_permission_denied`; the daemon capture paths never emit `audio_permission_denied` to the dashboard. The real audio loop records drops and pushes an overlay warning on errors (`crates/cue-daemon/src/app.rs:1941`), but it does not classify permission denial into `AudioEvent::PermissionDenied` or route it to Tauri.
- 🔴 The classifier helpers are not used in production. `is_permission_denied_error`, `is_permission_denied_message`, and `is_system_audio_permission_denied_message` have unit coverage, but no call sites from actual capture failure handling.
- 🔴 `open_privacy_settings` always launches the `open` command (`crates/cue-dashboard/src/commands.rs:460`), which is macOS-only. On Windows it builds a `ms-settings:` URI but still tries to execute `open`, so the button fails.
- Required fix: wire daemon permission failures into a dashboard-visible event or status poll, call the classifiers at the capture failure boundary, and use platform-specific launch commands (`open` on macOS, `cmd /C start` or `explorer.exe ms-settings:...` on Windows).

### P3.R6.4 — OpenAI Realtime STT Provider

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/stt/openai.rs`, `crates/cue-daemon/src/stt/router.rs`, `crates/cue-daemon/src/stt/mod.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 The provider does not configure a realtime transcription session. It connects, then immediately starts appending/committing audio, but never sends the required `session.update` payload with transcription settings. The current OpenAI Realtime transcription guide describes transcription sessions as `type: "transcription"` with `audio.input.transcription` configured before listening for transcript events: https://platform.openai.com/docs/guides/realtime-transcription
- 🔴 The parsed event names are wrong for input STT. The provider listens for `response.audio_transcript.delta` and `response.audio_transcript.done` (`crates/cue-daemon/src/stt/openai.rs:108`), but current transcription events are `conversation.item.input_audio_transcription.delta` and `conversation.item.input_audio_transcription.completed`. A real socket can connect and still never produce `TranscriptEvent`s through this parser.
- 🔴 The default model is `gpt-4o-realtime-preview` (`crates/cue-daemon/src/stt/openai.rs:37`), which is a realtime conversation model name, not a transcription model from the current transcription-session list (`gpt-4o-transcribe`, `gpt-4o-transcribe-latest`, `gpt-4o-mini-transcribe`, or `whisper-1`).
- 🟡 The provider is not yet wired into any factory/router chain. The handoff explicitly defers this, so I am not counting it as a blocker for this commit, but it means Round 6 does not deliver a usable fallback until the above provider correctness issues and factory wiring land.
- Required fix: send a transcription `session.update` after connect, parse `conversation.item.input_audio_transcription.*`, use a transcription model default, and add mock-WS tests that assert the initial session config frame and actual transcription event names.

### P3.R6.5 — R6 Work Docs

| Field | Value |
|-------|-------|
| Files | `docs/work/HANDOFF-TO-CODEX-FROM-KIRO.md` |
| Verdict | 🔴 blocker |

**Findings:**
- 🔴 `IMPL-PHASE-3-ROUND-6.md` and `PHASE-3-ROUND-6-HANDOFF-FOR-CODEX-REVIEW.md` are still missing. The handoff marks them as P0 housekeeping, and without them the review trail is incomplete.

## Cross-Task Findings

- R6 is not ready to merge on top of the R5 fix branch because the R5 continuous system-audio STT blocker is still open.
- Several R6 items have useful local pieces and unit tests, but the user-facing paths are not end-to-end: mic setting is not consumed, permission denial is not emitted from real capture, and OpenAI Realtime STT does not follow the current transcription protocol.

## Build & Test Verification

```bash
cargo fmt --all --check                    # ✅
cargo clippy --all-targets -- -D warnings  # ✅
cargo build --all-targets                  # ✅
cargo test --all-targets                   # ✅ 185 passed, 2 ignored
cd crates/cue-dashboard/ui && npm run build # ✅
git diff --check                           # ✅
```

## Overall Verdict

🔴 **REQUEST CHANGES** — Blockers must be resolved before Round 6 can merge.

## Follow-ups for Next Batch

- Fix R5 system-audio STT provider ownership first; R6 live audio work depends on it.
- Complete R6 docs after the fix round so future reviewers can compare intended scope to final behavior.
- After provider correctness is fixed, wire OpenAI into `SttRouter` behind `BLUEY_STT_FALLBACK_OPENAI=1` with deterministic ordering and failover tests.
- Consider moving daemon IPC for hotkeys/tray from React listeners into Rust-side handlers for better background reliability.
