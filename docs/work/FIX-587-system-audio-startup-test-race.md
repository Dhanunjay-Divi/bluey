# FIX-587: System-Audio Startup Test Race

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The system-audio integration test could report zero captured chunks under a
loaded full-workspace run even though the helper and production capture path
were healthy.

## Root Cause

Both receive loops in
`crates/cue-daemon/tests/system_audio_integration.rs` declared bounded outer
deadlines of three or five seconds, but their wildcard match treated the first
500-millisecond `rx.recv()` timeout as terminal. Native helper startup includes
task scheduling, child process spawn, and a validated ready handshake; the
production path deliberately allows up to three seconds for that handshake.

The stub was verified independently to emit the exact protocol-v1 ready event
followed by 64,000 bytes of 16 kHz mono PCM. The equivalent in-module test had
already been hardened against this same parallel-load scheduling condition.

## Fix Summary

The integration loops now distinguish channel closure from a quiet polling
interval. `Ok(None)` remains terminal, while a timeout continues only until the
existing bounded outer deadline. No production helper, capture, restart, or
audio-frame behavior changed.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/tests/system_audio_integration.rs` | Honor the declared outer deadlines in both helper receive loops |

## Edge Cases Handled

- A closed sender still ends the test immediately.
- A delayed helper cannot wait forever; the existing three- and five-second
  deadlines remain authoritative.
- A healthy helper under parallel build/test load is not classified as an
  audio regression after one quiet poll.

## How to Test

```bash
# Passed five consecutive focused runs.
CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo +1.98.0 test -p cue-daemon --test system_audio_integration \
  system_audio_capture_receives_chunks_from_stub -- --exact

# Passed on the final source diff.
CARGO_INCREMENTAL=0 CARGO_PROFILE_TEST_DEBUG=0 \
  cargo +1.98.0 test --workspace
```

## Known Limitations

- This regression test uses the deterministic stub. Physical packaged macOS
  and Windows audio-helper validation remains a release certification gate.
