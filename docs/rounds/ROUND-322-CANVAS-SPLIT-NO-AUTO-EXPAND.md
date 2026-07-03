# Round 322 - Canvas Split No Auto Expand

Date: 2026-07-03
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner reported that Bluey was expanding the overlay window by itself when canvas opened, and asked for canvas mode to be size adjustable instead of forcing the whole window larger. The owner also repeated that click-through move handling needs to feel reliable.

## Root Cause

- macOS canvas open called `ensureRoomForCanvas()`, which widened the entire overlay up to the canvas target width whenever the current window was smaller.
- Canvas width was computed from a fixed ratio with no user-adjustable divider.
- In click-through mode, normal explicit-control hit testing could win before the special move-handle drag path, making the cyan four-way move handle feel like it did not actually drag the overlay.

## Fix

- Removed automatic outer-window resizing from normal canvas open/close.
  - Opening canvas now keeps the window exactly where the user put it.
  - Closing canvas no longer shrinks/restores the outer window.
  - Explicit canvas full-window expand/restore remains available from the canvas header button.
- Added a macOS split-pane divider between feed and canvas.
  - Divider appears only when canvas is open.
  - Drag left/right to resize the canvas pane.
  - Split fraction is saved in user defaults under `bluey.overlay.canvas.splitFraction.v1`.
  - Chat pane is protected by a minimum width so the feed does not disappear.
- Improved click-through move-handle hit priority.
  - The cyan move handle wins hit testing before generic button/control routing.
  - Click-through mode still lets blank space pass through to the app behind Bluey.

## Windows Parity Check

Checked `native/windows/cue-overlay/main.c`. The Windows overlay currently does not expose the same macOS canvas split pane or auto-expand canvas path, so there was no equivalent Windows canvas auto-grow behavior to patch in this round.

## Verification

Passed locally:

```bash
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh
cargo check -p cue-daemon
BLUEY_UPDATE_PUBKEY="$(cat /Users/uno/.bluey/release/bluey-release-ed25519.pub.b64)" make package-darwin-arm64
BLUEY_RELEASE_SIGNING_KEY_FILE=/Users/uno/.bluey/release/bluey-release-ed25519.pem PUBLISH_DO=1 PUBLISH_HOST=root@165.227.77.152 PUBLISH_PATH=/var/www/bluey scripts/deploy-bluey-sh-manual.sh
```

## Deployment

- Published desktop release `0.1.61` to `bluey.sh`.
- Live release metadata:
  - `https://bluey.sh/latest.json`
  - artifact: `https://bluey.sh/releases/v0.1.61/bluey-0.1.61-darwin-arm64.tar.gz`
  - artifact SHA256: `51c2b45c4d98f64e19ef6c74d14f30268fccb086ce7c534f1e69bb6186df1c59`
  - artifact size: `9185521` bytes
- Release artifact dev-flag/secret scan passed.
- `latest.json` signature verified.
- `/install.sh` returned `application/x-shellscript`.
- `/install.ps1` returned `application/x-powershell`.
- Unpacked macOS release binaries reported `0.1.61`.

## Current State

The macOS overlay now treats canvas as an internal adjustable split instead of an outer-window resize trigger. Desktop release `0.1.61` is live.

## Remaining QA / Gates

- Run a visible local overlay smoke:
  - open a code/system-design answer with canvas
  - verify the outer window does not auto-grow
  - drag the split divider in both directions
  - toggle click-through and drag the cyan move handle
  - explicit fullscreen canvas still expands/restores only when clicked
