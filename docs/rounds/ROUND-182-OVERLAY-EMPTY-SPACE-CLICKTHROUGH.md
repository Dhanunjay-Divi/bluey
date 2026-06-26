# Round 182 - Overlay Empty Space Clickthrough

## Trigger

Owner asked that all empty space in the overlay should click through to the app behind Bluey when click-through mode is turned on.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-26 02:50 EDT

## Root Cause

- The macOS overlay already had click-through mode, but several broad container regions still counted as interactive:
  - blank header drag space
  - blank composer-bar space
  - blank session drawer background
  - broad chrome containers such as the header and composer surfaces
- Windows already let middle-card clicks pass through, but `WM_NCHITTEST` still treated the whole header and composer bands as interactive.

## Fix

- Tightened macOS expanded-overlay hit testing in click-through mode:
  - blank header space now returns no hit instead of acting as a drag handle
  - blank composer-bar space returns no hit instead of focusing the composer
  - blank session drawer background returns no hit unless the click lands on an actual interactive child
  - broad visual containers were removed from the explicit interactive chrome list
  - real controls remain clickable, including buttons, menus, the opacity scrubber, the composer text area, scrollbars, resize edges, and modal confirmations
- Added a stale-event guard so blank header space cannot start a drag while click-through mode is on.
- Tightened Windows parity:
  - added a helper for visible child-control hit detection
  - `WM_NCHITTEST` now returns `HTCLIENT` for resize edges, collapsed pill, active file drags, and visible child controls
  - otherwise empty expanded-overlay space returns `HTTRANSPARENT`.

## Mac Windows Parity

- macOS behavior now matches the click-through toggle promise: empty visible glass should pass through, while actual controls still work.
- Windows has no separate click-through toggle, but its expanded overlay now follows the same empty-space behavior for ordinary hit testing.
- Windows still preserves explicit resize edges, collapsed pill interaction, and drag/drop capture.

## Verification

Passed:

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -municode`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`

## Files Touched

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-182-OVERLAY-EMPTY-SPACE-CLICKTHROUGH.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Current State

- In macOS click-through mode, blank overlay areas should no longer block clicks to the app behind Bluey.
- Header dragging in macOS expanded mode now requires interactive mode; this is intentional so click-through mode behaves like glass.
- The rebuilt macOS overlay binary has been installed to `~/.bluey/bin` under both expected overlay names.
- Windows empty expanded-overlay space now hit-tests as transparent except for real controls and resize/drop affordances.

## Remaining QA Gates

- Manual macOS visual/interaction QA should confirm blank header, blank composer padding, blank card area, and blank drawer area pass through while controls still work.
- Manual Windows GUI QA should confirm blank expanded-overlay header/composer/card areas pass through while the edit box, buttons, combo box, resize edges, and collapsed pill still work.
