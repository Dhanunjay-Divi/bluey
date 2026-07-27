# FIX-010: Capture Can Say Listening When STT Failed to Initialize

## Issue

A native helper could continue producing PCM after STT construction failed,
while the overlay remained in `Listening` state and no transcript was possible.

## Root Cause

System audio converted provider-construction failure into `None` and drained
PCM forever. The microphone task returned on the same failure but left its
capture helper alive.

## Fix Summary

Both source-specific STT providers are now constructed while the source is
still `Connecting`, before launching the native capture helper. Construction
failure returns through the normal start-error path, so the overlay receives a
terminal failure and no unconsumed helper remains.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/app.rs` | Build system/microphone STT before native capture and propagate errors. |

## Edge Cases Handled

- Explicitly disabled system STT can still run the intentional PCM-only path.
- Shared Parakeet weights remain single-flight; each source retains independent
  decoder state.
- First-click model initialization cannot drop already-captured audio.

## How to Test

```bash
cargo check -p cue-daemon --features "parakeet-stt cloud-calendar"
```

With an invalid STT configuration, start either source and confirm the overlay
reports failure instead of `Listening`.

## Known Limitations

- Provider failures that occur after a successful connection retain their
  provider-specific retry policy.
