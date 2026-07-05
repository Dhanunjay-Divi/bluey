# Round 350 - macOS Click-Through Scroll Recursion

## Trigger

- The owner attached a macOS crash report for `bluey-overlay-macos`.
- The crash happened while the overlay was running in expanded mode and handling mouse/scroll routing.

## Root Cause

- The crash report showed:
  - `Exception Type: EXC_BAD_ACCESS (SIGSEGV)`
  - `Exception Message: Thread stack size exceeded due to excessive recursion`
  - recursion through `ExpandedPanelView.scrollWheel(with:)`
  - entry from `ExpandedPanelView.routeScrollWheelAtScreenPoint(_:event:)`
  - entry from `OverlayApp.handleGlobalMouseEventForClickThrough(_:)`
- The click-through global scroll path forwarded the original `NSEvent` into child scroll views with calls like `scrollWheel(with:)`.
- If AppKit did not consume that event inside the child scroll view, it bubbled back up to the parent `ExpandedPanelView.scrollWheel(with:)`.
- The parent then routed the same event back into the child again, causing an unbounded recursion loop until the main thread stack overflowed.

## Fix

- Added a direct `scrollViewDirectly(_:withWheelEvent:)` helper for macOS overlay scrolling.
- The helper converts the wheel delta into a clipped scroll offset update with `scrollClipView(...)`.
- Feed, canvas, session drawer, transcript strip, and composer scrolling now use direct scroll-offset movement instead of redispatching the original AppKit wheel event.
- `CanvasPaneView` now has a direct `scrollByWheelEvent(_:)` path and an override that uses it.
- `ExpandedPanelView.routeScrollWheelAtScreenPoint(_:event:)` now routes click-through scroll to direct scroll methods, removing the recursive AppKit event path.

## Windows Parity

- The crash is specific to the macOS Swift overlay and AppKit `NSEvent` bubbling.
- The Windows overlay is a separate native C implementation and does not use `ExpandedPanelView`, `NSScrollView`, or AppKit event forwarding.
- No Windows code change was required for this crash class.

## Verification

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cd native/macos/cue-overlay && swift build -c release
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.89
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey off
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

## Current State

- Desktop workspace version bumped to `0.1.89`.
- Desktop release `0.1.89` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.89/bluey-0.1.89-darwin-arm64.tar.gz`
- Artifact SHA256:
  `3f69b5d73636e01c464e45515e6515a429f3ac1ac11a62b182b34204da9e3e26`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.89`
- Public installer smoke installed `0.1.89` locally and both installed binaries report `0.1.89`.
- Local daemon restarted successfully with pid `50816`.

## Remaining QA

- Manually verify click-through scroll over:
  - answer feed
  - canvas pane
  - history drawer
  - transcript strip
  - composer input area
- Confirm scroll direction feels natural on the owner's trackpad/mouse.
