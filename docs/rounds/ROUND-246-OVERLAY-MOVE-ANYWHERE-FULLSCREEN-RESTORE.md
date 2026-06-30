# Round 246 - Overlay Move Anywhere Fullscreen Restore

## Trigger

The owner reported two live overlay problems:

- with click-through enabled, holding blank Bluey space no longer moved the
  overlay
- after moving Bluey, expanding to full screen, then restoring/minimizing, the
  panel jumped back toward the default top-middle position instead of returning
  to the user-chosen position

Backup thread id remains `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause

The previous strict click-through behavior made blank Bluey surface transparent
to mouse events. That let the app behind Bluey receive blank clicks, but macOS
and Windows then cannot also start a Bluey window drag from the same blank
pixel.

The full-screen restore path had a separate bug: `preWindowFullSizeFrame` was
captured before full-screen expansion, but `restoreWindowFromFullSize()` cleared
it and restored `ExpandedPanelMetrics.compactFrame(in:)`, which recalculated
the default compact placement.

## Fix

- macOS expanded overlay:
  - click-through/default mode now behaves as move-anywhere mode for blank
    Bluey surface
  - real controls still receive clicks
  - resize edges still resize
  - blank panel surface calls the existing drag path and persists the moved
    expanded frame
  - tooltip/toast copy now says `Move-anywhere on` instead of promising strict
    blank-space pass-through
- macOS full-screen restore:
  - `restoreWindowFromFullSize()` now captures `preWindowFullSizeFrame` before
    clearing it
  - restore fits that saved frame back into the visible screen, falling back to
    the saved expanded frame or compact default only when needed
  - restored frame is persisted through the existing `onWindowFrameChanged`
    path
- Windows parity:
  - blank expanded surface now returns `HTCAPTION` instead of `HTTRANSPARENT`
    so hold-and-drag moves the overlay
  - controls still return `HTCLIENT`
  - resize edges still return resize hit results
  - help copy now says blank Bluey space can be held to move the overlay
- Visual smoke source contract:
  - updated the stale full-screen tooltip marker
  - added markers for the move-anywhere wording and saved full-screen restore
    frame

## Verification

Passed:

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c
bash -n scripts/macos-overlay-visual-smoke.sh
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh
```

Also refreshed the local installed macOS overlay helpers for live testing:

```bash
install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos "$HOME/.bluey/bin/bluey-overlay-macos"
install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos "$HOME/.bluey/bin/cue-overlay-macos"
rm -rf "$HOME/.bluey/bin/BlueyOverlay.app"
cp -R native/macos/cue-overlay/.build/BlueyOverlay.app "$HOME/.bluey/bin/BlueyOverlay.app"
```

## Current State

The current product contract is now:

- blank Bluey surface is a drag handle in the default/click-through-labelled
  mode
- controls remain clickable
- strict behind-app blank click-through is no longer the active default
- full-screen restore should return to the position/size held before entering
  full-screen

This intentionally favors the owner's latest live-testing preference: easy
overlay movement from blank space.

## Remaining QA

- Relaunch Bluey so the installed helper reloads:
  `bluey off && bluey on`
- Manually verify:
  - drag from blank center/feed/chrome space while the mode button is active
  - header buttons still click
  - composer, Listen, Answer, Screen, History, and full-screen buttons still
    click
  - move Bluey, enter full screen, restore, and confirm it returns to the
    moved position
  - repeat the same behavior on Windows before release packaging

