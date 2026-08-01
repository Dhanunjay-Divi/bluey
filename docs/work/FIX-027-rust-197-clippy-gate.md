# FIX-027: Rust 1.97 Clippy gate failures

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

GitHub Actions failed `cargo clippy --all-targets -- -D warnings` after the
hosted toolchain advanced to Rust 1.97.

## Root Cause

Platform-specific process helpers were imported or defined on targets that
could not call them, and two `format!` arguments borrowed temporary `String`
values unnecessarily.

## Fix Summary

Gate platform-only imports and helper definitions with the same target
conditions as their call sites, retain the packaged-helper resolver in tests,
and remove the redundant formatting borrows.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/app.rs` | Align command import with macOS/Windows and remove redundant borrows. |
| `crates/cue-daemon/src/audio/system_capture.rs` | Gate native-helper override names to supported targets. |
| `crates/cue-daemon/src/stt/whisper/mod.rs` | Compile packaged resolution only where used or tested. |
| `CHANGELOG.md` | Record restored CI compatibility. |

## Edge Cases Handled

- Linux tests retain packaged-helper symlink-escape coverage.
- macOS and Windows production helper resolution remains unchanged.

## How to Test

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test --all-targets
git diff --check
```

## Known Limitations

- None.
