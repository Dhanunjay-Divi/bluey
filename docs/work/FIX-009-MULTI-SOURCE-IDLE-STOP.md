# FIX-009: Quiet System Audio Can Archive an Active Microphone Session

## Issue

When both sources were enabled, a quiet system-audio channel could trigger the
idle watchdog and stop/archive the whole meeting while microphone transcription
was still active.

## Root Cause

The watchdog's `last_transcript_at` clock belongs only to the system STT task.
It could not observe microphone transcript events but treated its own silence as
silence for the shared session.

## Fix Summary

The system-source idle classifier now defers auto-stop whenever the independent
microphone capture is active. A pure classifier test pins the multi-source
invariant.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/app.rs` | Gate system idle auto-stop on microphone liveness and add coverage. |

## Edge Cases Handled

- A superseded session still exits without tearing down the replacement.
- System-only sessions retain the configured idle auto-stop.
- A pre-timeout check remains a no-op.

## How to Test

```bash
cargo test -p cue-daemon --features "parakeet-stt cloud-calendar" \
  system_idle_watchdog_never_stops_an_active_microphone --lib
```

## Known Limitations

- Microphone-only idle auto-stop is not introduced by this fix.
