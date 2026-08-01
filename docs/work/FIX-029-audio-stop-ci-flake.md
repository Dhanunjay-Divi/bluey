# FIX-029: System-audio stop cancellation CI flake

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

Two latest-SHA Linux workspace jobs intermittently failed
`stalled_helper_read_is_interrupted_by_stop_notification`, while an independent
duplicate run of the same SHA passed.

## Root Cause

The intentional-stop path killed and boundedly reaped the helper but then still
waited up to one second for its diagnostic reader before returning the already
decided `Clean` result. The test allowed only 250 milliseconds despite the
production helper-reap budget being 500 milliseconds, and its shell-wrapped
`sleep` fixture could leave a child holding inherited pipes.

## Fix Summary

Abort the diagnostic reader immediately when the stop flag or notification
wins, boundedly terminate the helper, await diagnostic-task cancellation, and
return `Clean`. Use a direct `sleep` process in the Unix fixture and assert
against Bluey's public one-second stop deadline.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/audio/system_capture.rs` | Short-circuit intentional-stop cleanup and harden the stalled-helper test fixture. |
| `CHANGELOG.md` | Record the bounded shutdown correction. |

## Edge Cases Handled

- A stop racing with stdout closure still takes the clean intentional path.
- Non-intentional exits still drain and validate bounded terminal diagnostics.
- Helper termination remains bounded and does not switch to unbounded
  `Child::kill().await`.
- The Unix test no longer leaves a shell child holding helper pipes.

## How to Test

```bash
cargo fmt --all --check
cargo test -p cue-daemon stalled_helper_read_is_interrupted_by_stop_notification
cargo test -p cue-daemon --all-targets
cargo clippy --all-targets -- -D warnings
git diff --check
```

## Known Limitations

- Hosted-runner linker bus errors are infrastructure failures and remain
  suitable for a clean-job retry; they are not masked by this code change.
