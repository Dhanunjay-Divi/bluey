# FIX-016: Async-safe environment lock in memory integration tests

## Issue

The full daemon Clippy pass failed because the ignored real-memory integration
tests held a blocking `std::sync::MutexGuard` across asynchronous model and
search operations.

## Root Cause

The tests intentionally serialize access to the process-global
`BLUEY_DATA_DIR`, but used a synchronous mutex even though the guard must remain
held for each async test's complete lifetime.

## Fix Summary

Replace the blocking mutex with `tokio::sync::Mutex` and await its guard. The
tests retain the required process-wide serialization without blocking an async
runtime worker or triggering `clippy::await_holding_lock`.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/tests/facts_memory_real.rs` | Use an async-aware mutex for the environment guard. |
| `docs/work/FIX-016-async-memory-test-lock.md` | Document the validation fix. |

## Edge Cases Handled

- Poison recovery is no longer needed because Tokio mutexes do not poison.
- The guard remains live for the full test, so concurrent ignored tests cannot
  cross-contaminate their data directories.

## How to Test

```bash
cargo clippy -p cue-daemon --all-targets \
  --features parakeet-stt,local-memory,cloud-calendar -- -D warnings
```

## Known Limitations

- The real-memory tests remain ignored by default because they require a model
  download and, for one case, an external agent CLI.
