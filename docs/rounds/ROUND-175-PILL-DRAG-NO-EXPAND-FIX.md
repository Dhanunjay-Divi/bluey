# Round 175 - Pill Drag No Expand Fix

Date: 2026-06-25 14:25 EDT

## Summary

Fixed the macOS collapsed pill opening when the user is trying to drag it somewhere else.

## Change

- Reworked `PillView` drag handling in `native/macos/cue-overlay/Sources/cue-overlay/main.swift`.
- Tracks the mouse-down screen point and starting window frame.
- Moves the pill window directly while dragging instead of using AppKit `performDrag`.
- Suppresses `onClick` on mouse-up whenever the movement crossed the drag threshold.
- Clamps the pill to the visible screen while dragging.
- Keeps the mini rail buttons as normal buttons because hit testing still routes those clicks to the buttons.

Windows already had the corresponding behavior through `g_collapsed_drag_moved`, so this brings macOS in line with Windows.

## Verification

```bash
native/macos/cue-overlay/build.sh
install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos "$HOME/.bluey/bin/bluey-overlay-macos"
install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos "$HOME/.bluey/bin/cue-overlay-macos"
rm -rf "$HOME/.bluey/bin/BlueyOverlay.app"
cp -R native/macos/cue-overlay/.build/BlueyOverlay.app "$HOME/.bluey/bin/BlueyOverlay.app"
./scripts/bluey-visible-local.sh
```

Live local state after restart:

- Daemon pid: `64055`
- Overlay visible: true
- Overlay capture excluded: false, because visible local test mode is active

Manual QA still needed:

- Drag the pill a few times and confirm it stays collapsed.
- Click without moving and confirm it expands.
- Click the ask/listen/off mini buttons and confirm they still work.
