# Round 347 - Click-Through Scroll Routing

Date: 2026-07-04
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner reported that when click-through is on, scrolling inside the Bluey window still did not work. The desired behavior is that blank clicks pass through to the app behind Bluey, while the user can still scroll Bluey's own feed, canvas, history, live captions, and composer areas with a mouse wheel or trackpad.

## Product Rule

Click-through mode should not make Bluey feel frozen.

- Blank clicks and blank drags should pass through to the underlying app.
- Real Bluey controls should remain clickable.
- Wheel and trackpad scrolling over Bluey scrollable panes should scroll Bluey.
- The user should not need to turn click-through off just to read older answers or canvas content.

## Fix

- Added macOS global scroll-wheel routing while click-through mode is active.
- When the pointer is over Bluey's expanded window, scroll-wheel events are routed to the matching pane:
  - history drawer
  - live transcript preview
  - composer area
  - answer feed
  - right-side canvas
- Blank click-through semantics are preserved because the change only routes wheel events.
- Updated the shortcuts/help copy to explain that blank clicks pass through but Bluey panes still scroll.
- Desktop workspace version bumped to `0.1.86`.

## Verification

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cd native/macos/cue-overlay && swift build -c release
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.86
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

Result:

- Swift parse passed.
- Native macOS release build passed.
- Rust format/check passed.
- Release artifact dev-flag/secret scan passed.
- `latest.json` signature verified.
- Installer MIME checks passed.
- Darwin arm64 artifact SHA verified.
- Unpacked binaries report `0.1.86`.
- Public installer smoke installed `0.1.86`.
- Local daemon started successfully with pid `39283`.

## Deployment

Desktop release:

```text
0.1.86
```

Live artifact:

```text
https://bluey.sh/releases/v0.1.86/bluey-0.1.86-darwin-arm64.tar.gz
```

Artifact SHA256:

```text
f7232a5aa03ea5b8aae329194462f5aaf7f004da7ad994dfbc617fade7de488a
```

## Windows Parity

This round changed the macOS Swift overlay's AppKit event routing. The Windows overlay has a separate native implementation and did not share this exact global monitor path, so no Windows source change was made here.

Parity rule for Windows: if click-through is enabled, blank clicks should pass through, but scroll-wheel input over Bluey's feed, canvas, history, captions, and composer panes should still scroll the matching Bluey pane.
