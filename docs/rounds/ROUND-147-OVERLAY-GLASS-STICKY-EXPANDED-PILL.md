# Round 147 - Overlay Glass And Sticky Placement

## Problem

The Bluey pill and expanded panel did not feel as steady as Pinky. The collapsed pill only remembered its dragged position while the current overlay process stayed alive, and the expanded panel reset to the centered compact frame every time it opened.

## Change

- Added a local macOS placement store for the collapsed pill and expanded panel.
- Restored the collapsed pill position across drag, open, collapse, and relaunch.
- Restored the expanded panel position and size after header drag, resize, collapse, and relaunch.
- Widened the macOS resize edge so top, left, right, and bottom resizing is easier to catch.
- Refreshed the macOS pill and expanded panel with a darker glass finish, cyan edge, inner highlight, and softer shadow.
- Added the same persistent expanded and collapsed placement behavior to the Windows overlay source.

## Manual Check

1. Start Bluey in visible local mode.
2. Drag the collapsed pill, open it, then hide it again.
3. Confirm it returns to the same pill position.
4. Drag the expanded top bar, resize the panel, collapse, and reopen.
5. Confirm the expanded panel reopens where it was left.
