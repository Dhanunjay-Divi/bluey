# Bluey Release v0.1.83

Date: 2026-07-04

Branch: `codex/bluey-overlay-spacing-20260626`

## Summary

This release polishes the macOS Tone editor so it no longer feels like a normal document text field.

## Changes

- Tone editor keeps the normal arrow mouse cursor over the input and active field editor.
- Tone editor hides AppKit's native vertical insertion caret.
- Tone editor draws a Bluey-blue blinking underscore/block caret inside the input.
- The custom caret follows typing, mouse placement, arrow-key movement, and collapses when text is selected.

## Verification

```bash
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
cd native/macos/cue-overlay && swift build -c release
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.83
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
```

## Round Doc

- `docs/rounds/ROUND-344-TONE-UNDERSCORE-CARET.md`
