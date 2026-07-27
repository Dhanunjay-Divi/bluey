# FIX-021: Independent Capture Shortcuts

## Issue

The system-audio and microphone shortcuts in the meeting overlay could leave a
source running after it was switched off, restart the other source
unnecessarily, or show the wrong listening state. Repeated starts also spawned
overlapping native microphone helpers.

## Root Cause

`OverlayEvent::RecordingStartRequested` treated every source selection as a
fresh start. It restarted system audio whenever the mic changed and restarted
the mic whenever system audio changed. The `enable_system = false` path returned
without stopping an existing system capture.

Mic-only state checked the legacy `daemon.microphone` slot, while the active
VoiceProcessingIO helper is stored in `daemon.microphone_helper`. The mic start
function used the same incomplete check, so it could replace the stored native
helper handle without first stopping the prior process.

The listening session itself was owned by system audio. Mic-only capture never
created `audio.session_id` or a meeting, so its ordered STT sink dropped every
segment. Stopping system audio also cleared that shared id before a still-live
mic (or its trailing finals) finished.

Finally, helper start was fire-and-forget and liveness meant only “an Option
contains a handle.” Permission denial or a terminal helper exit therefore
remained painted as Listening indefinitely.

## Fix Summary

- Plan source transitions from current and requested state.
- Start and stop only the source whose selection actually changed.
- Stop system audio independently without ending mic capture or archiving the
  meeting.
- Detect the active native mic helper when deriving overall listening state.
- Make mic start stop both supported mic backends and join the old STT task
  before replacement.
- Track the system shortcut independently from the overall listening state, so
  the first idle click starts capture and mic-only mode does not paint system
  audio as active.
- Preserve a listening state when one requested source works and the other
  reports an actionable warning.
- Create one source-independent listening session and meeting when either mic
  or system audio starts first; retain it until both ordered sinks drain.
- Track native helper lifecycle as starting, running, denied, failed, or
  stopped. The overlay stays Connecting until PCM arrives and clears dead
  handles on a terminal outcome.
- Push authoritative per-source state with every listening-state update, so
  all expanded and collapsed shortcuts preserve mic-only/system-only behavior.
- Seed conditionally mounted pill and legacy-tab controls from the persistent
  meeting provider, then consume subsequent per-source pushes directly.
- Make every system/mic shortcut submit the complete current source pair,
  avoiding global Stop or an implicit re-enable of the other source.
- Make the mock client publish stateful source updates so development mode
  exercises the same toggle contract as the live daemon.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/app.rs` | Correct source transition, helper lifecycle, state reporting, and add transition tests. |
| `crates/cue-daemon/src/audio/capture.rs` | Report raw microphone-thread liveness. |
| `crates/cue-daemon/src/audio/system_capture.rs` | Expose first-PCM and terminal helper lifecycle instead of treating spawn as success. |
| `crates/cue-core/src/overlay.rs` | Add authoritative system/microphone flags to listening pushes. |
| `crates/cue-core/src/overlay_ipc.rs` | Keep the native IPC source fields backward compatible and tested. |
| `crates/cue-meeting-overlay/ui/src/screens/OpenFloorScreen.tsx` | Initialize and synchronize system-source selection correctly. |
| `crates/cue-meeting-overlay/ui/src/components/floorplan/FloatingStack.tsx` | Render system shortcut state independently from mic-only listening. |
| `crates/cue-meeting-overlay/ui/src/components/FloorplanPill.tsx` | Render and toggle system audio independently in the collapsed floorplan pill. |
| `crates/cue-meeting-overlay/ui/src/components/Pill.tsx` | Preserve microphone capture from the legacy collapsed system shortcut. |
| `crates/cue-meeting-overlay/ui/src/screens/AskScreen.tsx` | Keep legacy system/mic shortcuts source-specific and source-aware. |
| `crates/cue-meeting-overlay/ui/src/lib/meetingState.tsx` | Persist both source selections across collapse and consume daemon-authoritative source state. |
| `crates/cue-meeting-overlay/ui/src/lib/mockClient.ts` | Mirror live per-source listening state in development mode. |

## Edge Cases Handled

- Re-sending the same two-source selection is a no-op.
- Turning off system audio leaves an active microphone untouched.
- Turning off the mic leaves active system audio untouched.
- Collapsing during a mic-only session shows system audio as off without
  changing the active microphone.
- A failure in one source does not report the whole overlay as stopped when the
  other source is still running.
- Full Stop still drains both STT tasks before meeting archival.
- Mic-only capture creates and persists a transcript meeting.
- A denied or exited helper no longer remains falsely active.

## How to Test

```bash
cargo test -p cue-daemon audio_source_transition --features \
  "parakeet-stt cloud-calendar"
cargo test -p cue-daemon shared_audio_session --features \
  "parakeet-stt cloud-calendar"

# Live:
# 1. Start system audio.
# 2. Toggle mic on/off repeatedly; verify the system helper PID does not change.
# 3. Keep mic on and toggle system off; verify system helper exits and mic stays.
# 4. Toggle both back on; verify one helper per source and a listening state.
```

## Known Limitations

- macOS still owns the permission dialog and can require the user to restart
  capture after changing a grant in System Settings.
