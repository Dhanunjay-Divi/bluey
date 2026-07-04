# Bluey Release v0.1.81

Date: 2026-07-04

Branch: `codex/bluey-overlay-spacing-20260626`

## Summary

This release polishes macOS overlay input behavior for Tone and Ask.

## Changes

- Tone editor text no longer shows the I-beam cursor while hovering or editing; Bluey keeps the normal arrow cursor in that panel.
- Ask input focused state is much clearer with a stronger Bluey-blue border, subtle blue fill, and focused shadow.
- Dark and light theme composer focus colors are kept in sync.

## Verification

```bash
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.81
```

## Round Doc

- `docs/rounds/ROUND-342-TONE-CURSOR-INPUT-FOCUS.md`

