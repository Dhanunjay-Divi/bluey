# Round 338 - Canvas Click-Through Hit Testing

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

Branch: `codex/bluey-overlay-spacing-20260626`

## Trigger

The owner reported that canvas mode click-through was not working and asked how to make it better.

## Root Cause

The macOS expanded overlay treated the entire open canvas pane as interactive while click-through was enabled. That made the large canvas body and blank pane area keep Bluey mouse-active, so clicks that should have landed in the app behind Bluey were blocked by the canvas.

There was also a generic interactive-hit path that could classify the selectable canvas text view as interactive. That meant even if the blank canvas pane was fixed, the code/body text area could still prevent click-through.

## Fix

- Added `CanvasPaneView.passThroughInteractiveHit(at:)`.
- In click-through mode, the canvas now only keeps these areas interactive:
  - enabled canvas header buttons
  - canvas scroller
- The canvas body, code/text area, and blank canvas background now click through to the app behind Bluey.
- Moved canvas hit-testing ahead of the generic explicit-interactive path so selectable canvas text does not accidentally keep the overlay mouse-active.
- Kept the canvas divider and blue move handle interactive.
- Updated the shortcuts copy:
  - `With click-through on, canvas body clicks behind Bluey; canvas buttons stay usable.`
- Bumped desktop workspace version to `0.1.77`.

## Product Rule

In click-through mode:

- Canvas controls are still usable.
- Canvas body is pass-through.
- To select/copy arbitrary canvas text with the mouse, turn click-through off or use the canvas copy button.

This is the least surprising behavior for an overlay: click-through should not be defeated just because a large detail pane is open.

## Verification

```bash
cd native/macos/cue-overlay && swift build -c release
cargo check -p cue-daemon -p cue-cli
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/publish-bluey-release.sh
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem scripts/bluey-release-live-verify.sh 0.1.77
curl -fsSL https://bluey.sh/install.sh | bash
/Users/uno/.bluey/bin/bluey --version
/Users/uno/.bluey/bin/bluey-daemon --version
```

## Release

Desktop release `0.1.77` is live on `bluey.sh`.

Darwin arm64 artifact:

`https://bluey.sh/releases/v0.1.77/bluey-0.1.77-darwin-arm64.tar.gz`

Artifact SHA256:

`40013718b5a79394f166186274d1b12782aa52bc93a49465182a6a5ed5c4bca2`

Public installer smoke installed `0.1.77` locally. In this non-interactive shell, sudo prompting was unavailable, so install correctly fell back to the user-local symlink path.

## Windows Parity

The issue and fix are in the macOS Swift overlay canvas. The Windows C overlay does not currently share this same canvas pane/hit-test path, so there was no Windows code change in this round.
