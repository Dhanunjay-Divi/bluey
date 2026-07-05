# Bluey 0.1.88

Released: 2026-07-04

## Summary

This release makes screen-context follow-ups more reliable and fixes a macOS overlay feed layout issue that could create large blank gaps between chat messages.

## Changes

- If managed vision rejects a screenshot-backed follow-up with HTTP `400`, Bluey now retries using saved screen text, recent Q&A, and any code artifact instead of failing immediately.
- Code/debug follow-ups use the managed deep lane for that fallback.
- Added daemon diagnostics for the vision-to-text fallback path.
- Mac overlay chat feed now uses compact vertical stack distribution so answers and follow-up questions stay close together.

## Verification

```bash
cargo fmt --check
cargo test -p cue-daemon managed_vision_bad_request_falls_back_to_text_deep_for_code_follow_up -- --nocapture
cargo check -p cue-cli -p cue-daemon
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cd native/macos/cue-overlay && swift build -c release
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.88
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

Round doc:

- `docs/rounds/ROUND-349-FOLLOWUP-VISION-FALLBACK-FEED-GAP.md`

Live artifact SHA256:

```text
42acefb183d8c0a762679caba7aac375ed25ad29051e5027e17281630aceaa3c
```
