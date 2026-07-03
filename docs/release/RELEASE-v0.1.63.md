# Release v0.1.63

Released: 2026-07-03

## Summary

This release keeps useful partial code answers when a provider stream drops with an open Markdown code fence, and nudges coding answers to start with a short approach sentence before the full code.

## Changes

- Repaired recoverable `unclosed_code_fence` answer streams instead of replacing them with a generic retry card.
- Preserved code canvas detection for repaired partial code answers.
- Added a short visible note when Bluey keeps a partial answer after a dropped stream.
- Updated the coding answer prompt so the overlay gets useful explanation text before large code blocks start streaming.
- Added regression tests for partial code repair and the coding prompt contract.

## Verification

- `cargo fmt --check`
- `cargo test -p cue-daemon incomplete_code_answer_repair -- --nocapture`
- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape -- --nocapture`
- `cargo check -p cue-daemon`
- `BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64`
- `BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.63`

## Artifact

- Darwin arm64:
  `https://bluey.sh/releases/v0.1.63/bluey-0.1.63-darwin-arm64.tar.gz`
- SHA256:
  `36aee8d6c5934b9bf55c122fab90ea8ca4614b672bbe8982cdaace5e01371d93`
