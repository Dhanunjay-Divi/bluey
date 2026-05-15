# REVIEW: Phase 3 Round 6 — Post-Fix Recheck

**Commit range:** `77f5e9e..7242a18`
**Reviewer:** Codex
**Date:** 2026-05-15

## Per-Task Review

### P3.R5.F3 Recheck — Single-Provider System-Audio STT

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/app.rs`, `crates/cue-daemon/tests/system_audio_integration.rs` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ The original blocker is fixed. The continuous system-audio path now builds one provider instance and uses the same task to call both `provider.send_audio(&chunk)` and `provider.next_event()` (`crates/cue-daemon/src/app.rs:473`, `crates/cue-daemon/src/app.rs:489`, `crates/cue-daemon/src/app.rs:501`).
- ✅ The broken `drain_system_audio_stt_events` helper that created a second provider instance is gone.
- ✅ `single_provider_send_and_drain_production_wiring` proves the intended single-provider loop shape: chunks enter through the production-style channel, the same `MockStt` receives them, and emitted finals become downstream transcript metadata (`crates/cue-daemon/tests/system_audio_integration.rs:207`).
- 🟡 Minor test-strength nit: the new test still mirrors the production loop rather than calling the private production helper directly. It is a meaningful regression test for the exact bug, but if this path grows again, consider extracting the loop into a testable helper to avoid drift.

### P3.R6.1 — Hotkey / Tray Events to Daemon Actions

| Field | Value |
|-------|-------|
| Files | `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/ui/src/App.tsx` |
| Verdict | 🟡 minor nit |

**Findings:**
- ✅ No regression from the first review. The tray/shortcut events are wired to Tauri commands, and those commands call daemon TCP IPC.
- 🟡 Existing reliability nit remains: Rust receives the global shortcut first, but the daemon action depends on the React `HotkeyListener` staying mounted (`crates/cue-dashboard/ui/src/App.tsx:27`). Moving the IPC dispatch into Rust-side handlers is still a good Round 7 hardening item.

### P3.R6.2 Recheck — Mic Device Selection Plumbing

| Field | Value |
|-------|-------|
| Files | `crates/cue-core/src/ipc.rs`, `crates/cue-daemon/src/app.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-cli/src/app.rs`, `crates/cue-daemon/tests/mic_device_selection.rs` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ `DaemonRequest::AudioStart` now carries `mic_device_id: Option<String>` with `#[serde(default)]` for backward-compatible JSON (`crates/cue-core/src/ipc.rs:70`).
- ✅ The daemon handler applies `mic_device_id` into `AudioCaptureConfig.microphone.device_id` before starting capture (`crates/cue-daemon/src/app.rs:903`).
- ✅ The dashboard PTT command reads `audio.mic_device` from the DB before sending `AudioStart` (`crates/cue-dashboard/src/commands.rs:390`).
- ✅ The CLI passes `None`, preserving current default-device behavior (`crates/cue-cli/src/app.rs:586`).
- ✅ Coverage includes serialization, backward compatibility, DB round-trip, and config population.

### P3.R6.3 Recheck — Permission Denial UX

| Field | Value |
|-------|-------|
| Files | `crates/cue-core/src/audio.rs`, `crates/cue-daemon/src/app.rs`, `crates/cue-dashboard/src/commands.rs`, `crates/cue-dashboard/src/lib.rs`, `crates/cue-dashboard/ui/src/App.tsx`, `crates/cue-dashboard/ui/src/components/PermissionBanner.tsx` |
| Verdict | 🔴 blocker |

**Findings:**
- ✅ The daemon now calls the microphone/system permission classifiers from the real audio error path and records `permission_denied_source` on `AudioCaptureStatus` (`crates/cue-daemon/src/app.rs:1972`, `crates/cue-daemon/src/app.rs:1985`).
- ✅ The cross-process status field is present and serializable (`crates/cue-core/src/audio.rs:828`).
- ✅ `open_privacy_settings` no longer hardcodes macOS `open`; command construction is platform-specific (`crates/cue-dashboard/src/commands.rs:460`).
- 🔴 The dashboard still does not surface real permission denials automatically. `poll_audio_permission` checks daemon status and emits `audio_permission_denied` (`crates/cue-dashboard/src/commands.rs:526`), but there are no frontend call sites for `poll_audio_permission` in `App.tsx` or `PermissionBanner.tsx` (`crates/cue-dashboard/ui/src/App.tsx:76`, `crates/cue-dashboard/ui/src/components/PermissionBanner.tsx:25`). As shipped, the banner still appears only when some caller explicitly invokes the test/manual command path, so the original user-facing UX gap remains.
- Required fix: add a mounted frontend poller or daemon/dashboard event bridge that invokes `poll_audio_permission` after audio start and periodically while audio is active. Add a UI-side test or component-level test that proves a status permission denial becomes a visible `PermissionBanner` without manually calling `emit_permission_denied`.

### P3.R6.4 Recheck — OpenAI Realtime STT Provider

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/stt/openai.rs`, `crates/cue-daemon/src/stt/router.rs` |
| Verdict | 🔴 blocker |

**Findings:**
- ✅ The old `response.audio_transcript.*` parser issue is fixed. The parser now maps `conversation.item.input_audio_transcription.delta` to partials and `.completed` to finals (`crates/cue-daemon/src/stt/openai.rs:111`, `crates/cue-daemon/src/stt/openai.rs:123`).
- ✅ The connect URL includes `intent=transcription`, auth stays Bearer-based, and the old response-audio transcript events are ignored instead of being misclassified.
- 🔴 The session configuration frame still does not match the current official OpenAI transcription-session API. The current OpenAI API reference documents the client event as `transcription_session.update` with transcription settings under the transcription session fields; this code sends `{"type":"session.update","session":{"input_audio_transcription":...}}` (`crates/cue-daemon/src/stt/openai.rs:150`, `crates/cue-daemon/src/stt/openai.rs:377`). A mock server accepting this project-local shape does not prove live API compatibility.
- 🟡 Model default note: `gpt-4o-mini-transcribe` is at least a transcription model, so this is no longer the old conversation-model blocker. However, the current realtime transcription guide recommends `gpt-realtime-whisper` for lowest-latency streaming sessions. If Bluey keeps `gpt-4o-mini-transcribe` as the default, document the latency/cost tradeoff and add a model override setting in the router-wiring round.
- Required fix: align the client configuration event with the current OpenAI transcription-session reference, or pin/document a specific supported beta protocol with a live smoke test. Add a mock-WS assertion for the exact official frame type/shape.

### P3.R6.5 — R6 Work Docs

| Field | Value |
|-------|-------|
| Files | `docs/work/IMPL-PHASE-3-ROUND-6.md`, `docs/work/PHASE-3-ROUND-6-HANDOFF-FOR-CODEX-REVIEW.md`, `docs/work/AGENT-ONBOARDING.md` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ The missing implementation and handoff docs now exist and explain scope, commits, test progression, design decisions, and deferrals.
- 🟡 Minor accuracy nit: both R6 docs say "11 commits ahead of R5" / tip `7a9285f`, but the branch now has the docs commit `7242a18` on top, so it is 12 commits ahead of `feat/phase-3-round-5`. This does not block the code.

## Cross-Task Findings

- R5.F3 is fixed; the severe dual-provider system-audio STT bug is gone.
- R6.2 mic-device plumbing is fixed.
- R6.3 is improved in the backend and Tauri command layer, but not complete as user-facing UX because no mounted UI code invokes the poll command.
- R6.4 is directionally better, but still needs one more protocol correction/verification against OpenAI's current transcription-session API before it can be called a live OpenAI provider.

## Build & Test Verification

```bash
cargo fmt --all --check                    # ✅
cargo clippy --all-targets -- -D warnings  # ✅
cargo build --all-targets                  # ✅
cargo test --all-targets                   # ✅ 201 passed, 2 ignored
cd crates/cue-dashboard/ui && npm run build # ✅
git diff --check                           # ✅
```

## Overall Verdict

🔴 **REQUEST CHANGES** — R5.F3 and mic-device selection are fixed, but Round 6 still has two merge blockers: permission denial is not automatically surfaced by the UI, and OpenAI Realtime's session configuration frame needs to match the current official transcription-session protocol or be proven with a live-compatible contract.

## Follow-ups for Next Batch

- Add a lightweight mounted permission poller in the dashboard, or move the permission-denial event bridge fully into Rust/Tauri so it does not depend on manual command invocation.
- Update OpenAI Realtime STT to the current `transcription_session.update` event shape, or provide a documented protocol pin plus a real API smoke test.
- Keep Rust-side hotkey IPC dispatch and OpenAI router factory wiring for Round 7 after these blockers are closed.
