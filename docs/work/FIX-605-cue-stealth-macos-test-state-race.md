# FIX-605: Serialize macOS Disguise Test State

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

The root `cargo test --all-targets` gate intermittently failed in
`tests::apply_disguise_terminal_macos`: after applying the Terminal disguise,
the test observed `CFBundleName=System Settings` instead of `Terminal`.

## Root Cause

`apply_disguise` mutates process-global disguise state on macOS, including the
`CFBundleName` environment variable and `argv[0]`. Rust runs unit tests in the
same test process concurrently by default. `crates/cue-stealth/src/lib.rs`
already had `TEST_ENV_LOCK`, but two tests that call `apply_disguise` did not
acquire it:

- `tests::apply_disguise_none_does_not_panic` could overwrite the value with
  `Bluey`.
- `reassertion_tests::reassertion_reads_current_mode_not_stale` could overwrite
  the value with `System Settings`.

The Terminal test held the mutex across its write and assertion, but an
unlocked writer could still race it. A focused parallel stress run reproduced
the exact `System Settings` observation on iteration 18.

## Fix Summary

Acquire the existing process-wide test mutex in every remaining test that calls
`apply_disguise`. The guard spans the complete sequence of disguise writes, so
all tests that mutate macOS process identity now participate in the same
serialization discipline.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-stealth/src/lib.rs` | Lock the two previously unsynchronized disguise tests. |
| `docs/work/FIX-605-cue-stealth-macos-test-state-race.md` | Record the failure, cause, fix, and verification. |

## Edge Cases Handled

- Serializes both the `None` (`Bluey`) and reasserted `Settings`
  (`System Settings`) writers against the Terminal assertion.
- Recovers a poisoned test mutex so one failed test cannot strand later tests.
- Uses the same harmless test-only guard on non-macOS targets, keeping the test
  structure consistent without changing production behavior.

## How to Test

```bash
cargo test -p cue-stealth --all-targets --all-features
cargo clippy -p cue-stealth --all-targets --all-features -- -D warnings
cargo fmt --all -- --check

# Stress the default parallel test harness repeatedly.
repeat 1000 {
  cargo test -p cue-stealth --lib --quiet -- --test-threads=16 || exit 1
}
```

The focused crate test passed, followed by 1,000 complete parallel test-binary
runs and 1,000 additional runs isolating the original Terminal-versus-Settings
competitors.

## Known Limitations

- The mutex is intentionally test-only. `apply_disguise` currently documents
  its production macOS use as single-threaded startup work; any future
  concurrent production callers must introduce an explicit process-identity
  synchronization contract.
