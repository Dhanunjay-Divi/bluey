# Round 293 - Clickthrough Move Handle Drag

## Trigger

The owner reported that the blue four-direction move handle shown during click-through mode could not be held and dragged to move the full Bluey window.

## Root Cause

The macOS overlay had two competing paths for the same handle:

- the click-through manual button fallback saw the move handle as a normal `NSButton`, highlighted it, and waited for mouse-up
- that stole the mouse-down before the handle could start a drag
- the handle then depended on AppKit `performDrag`, which is fragile for a transparent, mouse-policy-toggled overlay window

## Fix

- Added explicit mouse-down, drag, and mouse-up hooks to `HeaderMoveButton`.
- Added manual window-drag state to the expanded panel:
  - stores the start mouse location and start window frame
  - updates the window frame directly on mouse drag
  - clamps the moved frame to the visible screen
  - persists the final position through `onWindowFrameChanged`
- Excluded the move handle from the generic manual-button click fallback so it can behave as a drag handle instead of a click button.
- Kept the existing global click-through fallback as a backup for the case where the overlay is temporarily ignoring mouse events.
- Bumped desktop workspace version to `0.1.47`.

## Mac/Windows Parity

- macOS received the functional fix.
- Windows already uses native hit testing for this behavior: in click-through mode the handle returns `HTCAPTION`, so dragging it moves the window through the OS window manager.
- Windows source was syntax-checked again.

## Verification

Passed locally:

```bash
swift build -c debug --package-path native/macos/cue-overlay
cargo check -p cue-daemon --quiet
x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
git diff --check
```

Release verification:

```bash
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

Live checks passed:

- `https://bluey.sh/latest.json` reports version `0.1.47`.
- Live `latest.json.sig` verifies successfully against the release Ed25519 key.
- Live `darwin-arm64` artifact:
  `releases/v0.1.47/bluey-0.1.47-darwin-arm64.tar.gz`
- Live/local artifact SHA256:
  `069752aea6c8c4eeb79052cfb2e2e366208ba8de11c583e39e67adbd108123d4`
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.

## Current State

- macOS `0.1.47` is live and downloadable from the droplet.
- The click-through move handle now has a direct drag gesture instead of behaving like a clickable button.
- Windows source remains syntax-checked and keeps its native `HTCAPTION` move-handle path.

## Remaining QA

- Owner should update, run Bluey, turn click-through on, then hold the blue move handle and drag the overlay to several positions.
- Confirm the moved position is remembered after hide-to-pill, restore, full-size toggle, minimize/default-size toggle, and `bluey off && bluey on`.
