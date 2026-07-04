# Bluey 0.1.86

Released: 2026-07-04

## Summary

This release makes click-through mode more usable by routing mouse-wheel and trackpad scroll events to Bluey's own scrollable panes while preserving blank click-through behavior.

## Changes

- Click-through mode now routes scroll-wheel events to the Bluey pane under the pointer.
- Supported panes include the answer feed, history drawer, live transcript preview, composer area, and right-side canvas.
- Blank clicks still pass through to the underlying app.
- Shortcut/help copy now explains that Bluey panes can still scroll in click-through mode.

## Verification

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cd native/macos/cue-overlay && swift build -c release
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.86
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

Live artifact SHA256:

```text
f7232a5aa03ea5b8aae329194462f5aaf7f004da7ad994dfbc617fade7de488a
```

Round doc:

- `docs/rounds/ROUND-347-CLICKTHROUGH-SCROLL-ROUTING.md`
