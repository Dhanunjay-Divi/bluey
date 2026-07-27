# FIX-008: Audio Stop Can Steal a Newly Retried Task

## Issue

Rapidly toggling a native audio source off and back on could detach the new
source task, leaving future Stop actions unable to drain it.

## Root Cause

The stop and terminal-cleanup paths removed a capture handle, awaited its
multi-second shutdown, and only then removed the global task handle. A retry
during that await could install a new task in the same slot, which the stale
cleanup then removed and awaited.

## Fix Summary

Each stop path now takes the matching capture/helper and task handle together,
before its first await. Terminal monitors also verify the capture-status identity
before taking that pair, so an old monitor cannot clean up a replacement.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/app.rs` | Pair capture/task removal before asynchronous shutdown. |

## Edge Cases Handled

- System-audio and microphone source toggles use the same ordering.
- A terminal monitor for an older helper leaves a newer helper and task intact.
- Full Stop still drains both source tasks before meeting archival.

## How to Test

```bash
cargo test -p cue-daemon --features "parakeet-stt cloud-calendar" --lib
```

In a visible run, toggle either source off/on rapidly and confirm the replacement
source remains stoppable and its state continues updating.

## Known Limitations

- Native permission prompts are controlled by macOS and cannot be automated by
  the daemon.
