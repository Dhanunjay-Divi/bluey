# FIX-026: Continuous Audio Status

## Issue

While the visible overlay showed `listening` and an authorized `BlueyAudio`
microphone helper was streaming, `bluey audio status` reported `Idle`,
`native capture: no`, and “Native audio capture is not linked.”

## Root Cause

The overlay's continuous system and microphone paths store their authoritative
lifecycle in `daemon.system_audio` and `daemon.microphone_helper`.
`ensure_native_audio_session` added only the shared session ID to the older
aggregate `AudioPipelineStatus`. The `AudioStatus` request returned that
boot-time aggregate record without consulting the live source handles.

## Fix Summary

The daemon now snapshots each continuous source as inactive, starting, or
running and reconciles those snapshots when serving `AudioStatus`. It reports
only first-PCM sources as capturing, preserves the shared session and telemetry,
and leaves the independent chunk/REST runtime byte-for-byte unchanged.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/app.rs` | Reconcile diagnostic status with live native handles and add regression tests |
| `CHANGELOG.md` | Document the corrected diagnostic state |
| `docs/work/FIX-026-continuous-audio-status.md` | Record the bug, fix, edge cases, and verification |
| `docs/work/IMPL-OVERLAY-RELIABILITY.md` | Add the diagnostic fix to batch implementation evidence |
| `docs/work/REVIEW-MEETING-RELIABILITY.md` | Add review and verification evidence |

## Edge Cases Handled

- A spawned helper remains `Starting` until its first PCM bytes arrive.
- Microphone-only and dual-source sessions report only their active sources.
- A source-switch gap does not erase the shared session.
- Terminal cleanup marks stale native status stopped after clearing the session.
- An active handle observed before session publication does not invent an ID.
- Starting a new native session resets the previous session's counters and
  timestamps.
- A chunk runtime observed before or after native handle sampling keeps its
  authoritative aggregate status.
- Chunk/REST runtime provider labels, devices, counters, and status remain
  untouched.
- Source telemetry and permission-denied attribution survive reconciliation.

## How to Test

```bash
cargo test -p cue-daemon \
  --features parakeet-stt,local-memory,cloud-calendar \
  continuous_audio_status
cargo clippy -p cue-daemon --all-targets \
  --features parakeet-stt,local-memory,cloud-calendar -- -D warnings
```

Start the microphone from the visible overlay, confirm its helper reaches
`authorized`, and run `bluey audio status`. It should report native microphone
capture and the same live session ID.

## Known Limitations

- Idle helper installation/readiness remains intentionally unclaimed. A path
  existing on disk does not prove that its signature or current TCC grant is
  usable.
- During the brief gap between stopping one continuous source and publishing
  another, diagnostics preserve the shared session and may retain its previous
  aggregate source state until the replacement handle appears.
