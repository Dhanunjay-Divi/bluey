# FIX-583: Serialize Windows AppUserModelID Updates

## Issue

The Windows workspace test gate exited after the `cue-stealth` tests with
`STATUS_HEAP_CORRUPTION` even though every completed test passed.

## Root Cause

`set_app_user_model_id` changes process-global Windows Shell state. The test
harness ran disguise reassertion and direct AppUserModelID tests concurrently,
allowing multiple calls into the process-wide setter at the same time.

## Fix Summary

Guard the Windows Shell setter with one process-wide mutex. Add a regression
test that performs concurrent updates and requires every worker to complete.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-stealth/src/windows.rs` | Serialize setter calls and test concurrency. |
| `CHANGELOG.md` | Record the Windows stability fix. |

## Edge Cases Handled

- A poisoned mutex is recovered so a prior panic cannot permanently disable
  later process identity updates.
- Non-Windows platforms are unchanged.

## How to Test

```bash
cargo test -p cue-stealth --all-features
cargo test --workspace --all-targets --all-features
```

Run the workspace test command on Windows and confirm the `cue-stealth` test
binary exits normally after all tests finish.

## Known Limitations

- The regression requires a Windows runner to exercise the real Shell API.
