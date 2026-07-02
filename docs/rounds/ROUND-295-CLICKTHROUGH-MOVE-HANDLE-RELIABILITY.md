# Round 295 - Click-Through Move Handle Reliability

Date: 2026-07-02
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Release: `0.1.49`

## Trigger

The user reported that the blue four-way-arrow move handle in click-through mode was still not useful: holding it did not move the overlay reliably.

## Root Cause

The click-through move path had two fragile edges:

- The parent overlay hit-test returned the panel itself for the padded move-handle area instead of the `HeaderMoveButton`, so the button drag callbacks could be bypassed.
- If the overlay was ignoring mouse events when the user pressed the handle, the first mouse-down could belong to the app behind Bluey. The global monitor then armed a separate drag state, while later drag events could be split between global and local paths.

## Fix

- Enlarged the click-through move handle from `34px` to `42px`.
- Enlarged the hit padding from `18px` to `26px` so the user does not need pixel-perfect aim.
- In click-through mode, move-handle hit-testing now returns the actual `moveHandleButton`.
- Added shared manual drag methods on the expanded panel:
  - begin drag at a screen point
  - update drag to a screen point
  - end drag and persist the frame
- Rewired the global click-through mouse monitor to call the same panel drag methods as the button/local path.
- Kept a guarded global frame fallback so movement still works if macOS keeps the drag sequence outside Bluey.
- Added lifecycle logs for `drag_started` and `drag_ended` around handle movement.

## Verification

- `swift build -c debug --package-path native/macos/cue-overlay`
- `cargo check -p cue-daemon --quiet`
- `cargo check -p cue-core --quiet`
- `git diff --check`

## Release Verification

- Built macOS release package with the production update public key.
- Deployed `0.1.49` to `https://bluey.sh`.
- `https://bluey.sh/latest.json` reports `version: 0.1.49`.
- Live artifact:
  `https://bluey.sh/releases/v0.1.49/bluey-0.1.49-darwin-arm64.tar.gz`
- Live artifact SHA256:
  `247a300a6a91362bf99752638f41117841173ed9400c216f96179bdb7650a542`
- Live `latest.json.sig` verified successfully against the release Ed25519 public key.
- `/install.sh` returns `application/x-shellscript`.
- `/install.ps1` returns `application/x-powershell`.
- Unpacked live release reports:
  - `bluey 0.1.49`
  - `bluey-daemon 0.1.49`
- Release artifact scan passed with no configured secrets/dev capture flags present.

## Windows Parity

No Windows source change was needed in this round. Windows already uses `HTCAPTION` for the click-through move handle; this failure was specific to the macOS AppKit/global-monitor split.

## Remaining QA

- Live-test click-through mode on macOS:
  - Turn click-through on.
  - Press and hold the blue four-way-arrow handle.
  - Drag Bluey across the screen.
  - Release and verify the new position persists after expand/minimize.
