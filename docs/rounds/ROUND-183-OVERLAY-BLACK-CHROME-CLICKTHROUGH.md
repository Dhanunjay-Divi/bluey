# Round 183 - Overlay Black Chrome Clickthrough

## Trigger

Owner shared a screenshot of the overlay header and asked why the black spaces, including bottom black spaces, were still not clickable through. Owner asked to look at the whole window screenshot.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-26 03:01 EDT

## Root Cause

- Round 182 tightened AppKit view hit testing, but macOS click-through depends on the whole overlay window setting `ignoresMouseEvents` before the click lands.
- The whole-window policy still treated some empty chrome as interactive:
  - resize edges across the top/bottom/side borders
  - the full session drawer rectangle
  - the short composer armed timer, even when the cursor was over empty chrome
- Windows parity also still preserved blank resize borders through `WM_NCHITTEST`.

## Fix

- Captured and inspected a full-screen screenshot at `/tmp/bluey-round183-fullscreen.png` to confirm the visible overlay shape.
- Tightened macOS whole-window mouse policy:
  - composer armed mode now keeps mouse handling only over explicit controls
  - empty session drawer background no longer makes the whole window interactive
  - empty resize edges no longer make click-through mode interactive
- Tightened macOS stale-event paths:
  - blank resize edges no longer return a hit in click-through mode
  - blank resize edges cannot start a resize if a stale mouse-down event is delivered
  - header drag remains disabled while click-through mode is on
- Tightened Windows parity:
  - removed empty resize-border `HTLEFT`/`HTRIGHT`/`HTTOP`/`HTBOTTOM` returns in expanded mode
  - kept collapsed pill, visible child controls, and active file drags interactive
  - ordinary empty expanded-overlay chrome now returns `HTTRANSPARENT`.
- Refreshed the installed macOS `.app` bundle as well as the loose binaries. This matters because the daemon launches `~/.bluey/bin/BlueyOverlay.app`, not only `~/.bluey/bin/bluey-overlay-macos`.
- Restarted only the overlay child process and confirmed the daemon respawned it from the refreshed app bundle.

## Mac Windows Parity

- macOS click-through mode now treats black visual chrome as glass unless the pointer is on a real control or modal safety surface.
- Windows expanded overlay now follows the same empty-space rule for ordinary hit testing.
- Resize by empty border is intentionally not available while in click-through-style behavior; use interactive mode on macOS when the overlay itself needs to be moved or resized.

## Verification

Passed:

- `screencapture -x /tmp/bluey-round183-fullscreen.png`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -municode`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
- `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- overlay child restart and daemon respawn check
- `~/.bluey/bin/bluey overlay show`

## Files Touched

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-183-OVERLAY-BLACK-CHROME-CLICKTHROUGH.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Current State

- Empty macOS header, drawer, composer, border, and bottom black chrome should no longer keep the overlay mouse-active in click-through mode.
- Actual controls still remain clickable.
- The rebuilt macOS overlay binary has been installed to `~/.bluey/bin` under both expected loose binary names and inside `BlueyOverlay.app`.
- The visible local overlay process has been restarted through the daemon supervisor and should be running the refreshed app bundle.
- Windows empty expanded overlay chrome now avoids the prior blank resize-border hit regions.

## Remaining QA Gates

- Manually test blank header space, blank content/card space, blank bottom composer chrome, and blank border edges against a clickable app behind Bluey.
- Manual Windows GUI QA should confirm expanded-overlay empty borders/chrome pass through while visible controls and collapsed pill still work.
