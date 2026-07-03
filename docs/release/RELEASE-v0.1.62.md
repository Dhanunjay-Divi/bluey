# Release v0.1.62

Date: 2026-07-03
Round: `ROUND-323-LIVE-STT-HELPER-DISCOVERY.md`

## Summary

This release fixes a Listen/STT regression where installed Bluey could miss the bundled native audio helper and fall back to slower chunked transcription.

## Changes

- The daemon now checks the canonical executable path and `~/.bluey/bin` when locating native audio helpers.
- macOS and Windows helper discovery both include installed Bluey bin directories.
- Public and legacy installers now symlink helper binaries beside `bluey` and `bluey-daemon` when possible.
- STT fallback selection now logs warning-level breadcrumbs, so chunked fallback sessions are visible even when production logs use `RUST_LOG=warn`.

## Verification

- `cargo fmt --check`
- `cargo check -p cue-daemon`
- `bash -n scripts/install.sh && bash -n ops/install/install.sh`
- `BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64`
- Unpacked macOS release binaries reported `0.1.62`.
- Local IPC audio smoke selected `bluey-managed:deepgram/nova-3 live` with native system and microphone devices.

## Live Release

- `https://bluey.sh/latest.json` reports `0.1.62`.
- macOS artifact SHA256:
  `b05acbfc3a07a58436075cc30368b132add5a2b4b09f45044dcce7b481f88ab3`
- macOS artifact size:
  `9181487` bytes
- Live installer MIME checks passed.
- Live manifest signature verified.
- Unpacked macOS release binaries reported `0.1.62`.
