# FIX-028: Linux integration and observability policy gates

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and
> verify its memory against the current repository state.

## Issue

After the Rust 1.97 Clippy failures were corrected, the full Ubuntu workflow
still failed four system-audio integration tests and the observability policy
gate.

## Root Cause

Unsupported platforms unconditionally returned no native audio helper, so the
Linux debug integration build could not honor its explicit executable-stub
override. Separately, the Bluey Jobs rate-limit warning logged a raw
`account_id` field instead of the repository's canonical hashed support join
key.

## Fix Summary

Allow unsupported debug builds to resolve only an explicit, non-empty audio
helper override after canonical file and executable validation. Unsupported
release builds remain compile-time fail-closed. Replace the rate-limit log's
raw account ID with `account_id_hash` produced by the shared observability hash
helper.

## Files Modified

| File | Change |
|------|--------|
| `crates/cue-daemon/src/audio/system_capture.rs` | Restore validated debug-only override resolution on unsupported platforms. |
| `server/src/rate_limit.rs` | Log the canonical hashed account identifier. |
| `CHANGELOG.md` | Record the Linux CI and observability corrections. |

## Edge Cases Handled

- Linux release builds still cannot discover or launch a system-audio helper.
- Empty, missing, nonexistent, non-file, and non-executable overrides remain
  rejected.
- macOS and Windows development and packaged-helper discovery are unchanged.
- Rate-limit keys and behavior are unchanged; only the diagnostic field is
  privacy-safe.

## How to Test

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo test -p cue-daemon --test system_audio_integration
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
python3 scripts/analyze-tracing-calls.py --check-only
git diff --check
```

## Known Limitations

- Unsupported-platform release builds intentionally do not run the external
  system-audio stub tests because production capture is unavailable there.
