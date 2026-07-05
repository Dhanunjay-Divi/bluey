# Round 351 - Click-Through Content Scroll Policy

## Trigger

- The owner asked for a better approach to scrolling while click-through is enabled.
- Desired behavior:
  - scrolling over question/answer content should scroll Bluey
  - scrolling over blank black/empty panel space should pass through to the app behind Bluey
  - a visible Bluey scroller should still be usable as an intentional scroll target

## Product Decision

- In click-through mode, Bluey should only capture scroll when the pointer is over real Bluey content or a visible Bluey scroller.
- Blank glass/background should not behave like an invisible scroll target.
- In normal interactive mode, Bluey can continue scrolling the full feed region because the user has explicitly made Bluey interactive.

## Fix

- Added `FeedView.shouldCapturePassThroughScroll(at:)`.
- The method returns true only when:
  - the pointer is inside a visible card bubble, or
  - the pointer is on the feed's visible vertical scroller.
- Updated click-through feed scroll routing in `ExpandedPanelView.scrollWheel(with:)`.
- Updated global click-through scroll routing in `ExpandedPanelView.routeScrollWheelAtScreenPoint(_:event:)`.
- Empty feed panel space now returns false so the global monitor observes the event but does not also scroll Bluey.

## Windows Parity

- This round changes macOS AppKit click-through scroll routing.
- Windows uses a separate native overlay implementation and does not share this Swift `FeedView` / `ExpandedPanelView` path.
- The desired Windows parity behavior is the same product rule: content or visible scroller captures scroll; blank overlay space passes through.

## Verification

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
cd native/macos/cue-overlay && swift build -c release
cargo fmt --check
cargo check -p cue-cli -p cue-daemon
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.90
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
/Users/uno/.bluey/bin/bluey off
/Users/uno/.bluey/bin/bluey on
/Users/uno/.bluey/bin/bluey status
```

## Current State

- Desktop workspace version bumped to `0.1.90`.
- Desktop release `0.1.90` is live on `https://bluey.sh/latest.json`.
- Darwin arm64 artifact:
  `https://bluey.sh/releases/v0.1.90/bluey-0.1.90-darwin-arm64.tar.gz`
- Artifact SHA256:
  `260f5351872fedb552370a39aa87217b1eb4652dc3271d52d4bd70ff1879738b`
- Release verification passed:
  - `latest.json` signature verification
  - installer MIME checks
  - Darwin arm64 artifact SHA verification
  - unpacked `bluey` and `bluey-daemon` version checks for `0.1.90`
- Public installer smoke installed `0.1.90` locally and both installed binaries report `0.1.90`.
- Local daemon restarted successfully with pid `61294`.

## Remaining QA

- Click-through on:
  - scroll over answer text should scroll Bluey
  - scroll over a question bubble should scroll Bluey
  - scroll over blank black feed area should scroll the app behind Bluey
  - scroll over the visible feed scroller should scroll Bluey
- Click-through off:
  - normal feed area scrolling should continue to work.
