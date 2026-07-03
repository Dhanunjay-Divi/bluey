# Release v0.1.60

Date: 2026-07-03
Round: `ROUND-321-DEEPGRAM-REALTIME-STT-HARDENING.md`

## Summary

This release hardens realtime STT for Deepgram managed live captions.

## Changes

- Deepgram realtime defaults:
  - `endpointing=300`
  - `utterance_end_ms=1000`
  - `interim_results=true`
  - `vad_events=true`
  - `no_delay=true`
  - `language=en-US` by default
- Added `BLUEY_DEEPGRAM_LANGUAGE=auto|detect|none|off` escape hatch to omit the default language.
- Added privacy-safe provider-frame timing telemetry for first provider frame, transcript, partial, and final.
- Added desktop live STT tail finalization: send `CloseStream`, drain up to `850 ms`, then close.

## Verification

- `cargo fmt --all`
- `cargo test --manifest-path server/Cargo.toml deepgram_url -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml deepgram_frame_inspection -- --nocapture`
- `cargo test --manifest-path server/Cargo.toml stt::tests -- --nocapture`
- `cargo test -p cue-daemon live_stt_ -- --nocapture`
- `cargo test -p cue-daemon parse_frame -- --nocapture`
- `cargo check -p cue-daemon`
- `cargo check --manifest-path server/Cargo.toml`
- `git diff --check`
- `BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64`
- `BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh`

## Live Release

- `https://bluey.sh/latest.json` reports `0.1.60`.
- macOS artifact SHA256:
  `16164a213784b2d02e7293e5980bde5f8d328f37b1313ae87da52932d7cdcf2f`
- Live installer MIME checks passed.
- Live manifest signature verified.
- Production API relay was rebuilt and restarted active.
