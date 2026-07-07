# Bluey 0.1.89

Released: 2026-07-07

## Summary

Production-beta desktop/server release focused on code follow-up reliability.

## Changes

- Code follow-ups now preserve the active code canvas and replace it in place.
- Requested code changes and language conversions now ask for the complete updated implementation, not only a patch, diff, or changed block.
- Server artifact validation now rejects patch-only and diff-only blocks as code artifacts.
- macOS overlay source now maps code follow-up artifacts back onto the active code canvas instead of appending a separate canvas entry.
- Daemon prompt parity updated for Code and General modes so older native paths use the same whole-code replacement rule.

## Verification

```bash
cargo test --manifest-path server/Cargo.toml --lib --quiet
cargo test --manifest-path crates/cue-daemon/Cargo.toml --lib --quiet
swift build -c release
git diff --check
```

Round doc:

- `docs/rounds/ROUND-405-CODE-FOLLOWUP-INPLACE-REPLACEMENT.md`
