# Bluey Release v0.1.80

Date: 2026-07-04

Branch: `codex/bluey-overlay-spacing-20260626`

## Summary

This release fixes a stale-daemon upgrade issue and a macOS overlay feed layout gap.

## Changes

- `bluey on` now restarts the running daemon automatically when the installed daemon binary is newer than the process serving the overlay.
- Mac overlay feed rows no longer stretch to fill unused viewport height, which removes large blank gaps between cards.
- Legacy compressed inline code blobs, such as `Codecppclass...`, are hidden from chat display so old saved cards do not keep breaking the overlay.

## Verification

```bash
cargo fmt --check
cargo test -p cue-cli daemon_binary_change_check_detects_newer_installed_binary -- --nocapture
cargo check -p cue-cli -p cue-daemon
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
```

## Round Doc

- `docs/rounds/ROUND-341-STALE-DAEMON-FEED-GAP-FIX.md`

