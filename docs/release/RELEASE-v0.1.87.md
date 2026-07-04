# Bluey 0.1.87

Released: 2026-07-04

## Summary

This release keeps Bluey's overlay responsive while an answer is streaming, so users can capture screen context or attach documents for the next answer without waiting for the current stream to finish.

## Changes

- Overlay answer streaming now runs in a background task instead of blocking the daemon overlay event loop.
- Attach, screen, history, and other overlay events can be processed while an answer is in progress.
- A daemon-side guard prevents starting a second simultaneous overlay answer.
- Newly added macOS context is staged for the next answer when it arrives during an active stream.

## Verification

```bash
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cd native/macos/cue-overlay && swift build -c release
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.87
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

Live artifact SHA256:

```text
ecd0c9eb00c3f40259d9a9ab9f351f4ae7b88e579b21dcd733656c8da0bcbb1e
```

Round doc:

- `docs/rounds/ROUND-348-STREAMING-ATTACHMENT-EVENTS.md`
