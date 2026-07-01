# Bluey v0.1.36

## Summary

This release improves the desktop sign-in flow by making the one-time connect code visible inside the Bluey overlay.

- The daemon now sends a structured `Code: XXXX-XXXX` line with login cards.
- macOS overlay sign-in cards show a dedicated `Connect code` pill.
- The raw login URL remains hidden from the card body.
- The sign-in CTA now says `Open browser`.
- Windows receives the same structured login card body, so the code is visible in its existing renderer.

## Verification

- `cargo fmt --all`
- `cargo check -p cue-daemon --quiet`
- `cargo check -p cue-cli --quiet`
- `swift build -c debug --package-path native/macos/cue-overlay`
- `/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- live `latest.json.sig` verified successfully
- live artifact checksum verified against `SHA256SUMS.txt`
- temp-root installer smoke verified `bluey 0.1.36`

## Notes

This builds on the `0.1.35` production-update recovery cleanup. Very old installed builds may still need one production reinstall before they can receive signed updates normally.
