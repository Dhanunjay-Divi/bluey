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
| Verdict | 🟢 accept |

**Findings:**
- ✅ The daemon now calls the microphone/system permission classifiers from the real audio error path and records `permission_denied_source` on `AudioCaptureStatus` (`crates/cue-daemon/src/app.rs:1972`, `crates/cue-daemon/src/app.rs:1985`).
- ✅ The cross-process status field is present and serializable (`crates/cue-core/src/audio.rs:828`).
- ✅ `open_privacy_settings` no longer hardcodes macOS `open`; command construction is platform-specific (`crates/cue-dashboard/src/commands.rs:460`).
- ✅ The final UI bridge is now mounted. `PermissionPoller` invokes `poll_audio_permission` once on mount and every 5 seconds, which activates the daemon-status → Tauri event → `PermissionBanner` path without requiring manual `emit_permission_denied` calls (`crates/cue-dashboard/ui/src/App.tsx:58`, `crates/cue-dashboard/ui/src/App.tsx:107`).
- 🟡 Follow-up: the poller runs whenever the dashboard app is mounted, not only while audio is active. That is acceptable for alpha because the command is cheap and failure-tolerant, but Round 7 can reduce idle polling by checking audio state or starting the poll after audio start.

### P3.R6.4 Recheck — OpenAI Realtime STT Provider

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/stt/openai.rs`, `crates/cue-daemon/src/stt/router.rs` |
| Verdict | 🟢 accept |

**Findings:**
- ✅ The old `response.audio_transcript.*` parser issue is fixed. The parser now maps `conversation.item.input_audio_transcription.delta` to partials and `.completed` to finals (`crates/cue-daemon/src/stt/openai.rs:111`, `crates/cue-daemon/src/stt/openai.rs:123`).
- ✅ The connect URL includes `intent=transcription`, auth stays Bearer-based, and the old response-audio transcript events are ignored instead of being misclassified.
- ✅ The session configuration frame now uses the current official event type: `transcription_session.update` (`crates/cue-daemon/src/stt/openai.rs:151`, `crates/cue-daemon/src/stt/openai.rs:160`). Mock WebSocket tests assert that exact frame type before accepting audio/transcript traffic (`crates/cue-daemon/src/stt/openai.rs:558`, `crates/cue-daemon/src/stt/openai.rs:605`).
- 🟡 Model default note: `gpt-4o-mini-transcribe` is at least a transcription model, so this is no longer the old conversation-model blocker. However, the current realtime transcription guide recommends `gpt-realtime-whisper` for lowest-latency streaming sessions. If Bluey keeps `gpt-4o-mini-transcribe` as the default, document the latency/cost tradeoff and add a model override setting in the router-wiring round.
- 🟡 Follow-up: comments in the OpenAI test server still say "session.update" in a few places even though the assertions are correct. That is doc-comment polish only.

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
- R6.3 is now end-to-end enough for alpha: real capture failures set daemon status, dashboard polling emits the Tauri event, and the banner listens for it.
- R6.4 now matches the current OpenAI transcription-session event shape at the client-event level. Live API smoke testing can happen when keys/router wiring land.

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

🟢 **ACCEPT** — Ready to merge R5+R6 to main and start Round 7.

## Follow-ups for Next Batch

- Reduce idle permission polling or move the permission-denial bridge fully into Rust/Tauri when hardening the dashboard background lifecycle.
- Keep the OpenAI default model decision (`gpt-4o-mini-transcribe` vs `gpt-realtime-whisper`) for the router-wiring round, where it can be tested with real keys and latency/cost metrics.
- Wire OpenAI into `SttRouter` behind `BLUEY_STT_FALLBACK_OPENAI=1` and add a live-compatible smoke test once credentials are available.
- Move hotkey daemon IPC dispatch from React listeners into Rust-side handlers for better behavior during webview reloads.
