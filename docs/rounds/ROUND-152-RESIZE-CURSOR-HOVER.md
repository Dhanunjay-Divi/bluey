# Round 152 - Resize Cursor Hover

## Why

Bluey could resize from the custom borders, but macOS did not consistently show resize cursors because the overlay switches mouse passthrough on and off.

## Changed

- Added explicit resize cursors for left, right, top, and bottom edges.
- Added diagonal resize cursors for all four corners.
- Updates the cursor from the same hit-test path that controls passthrough, so the cursor appears before dragging.
- Added Windows cursor handling for edge and corner resize hit targets.

## Verified

- Built the macOS overlay with `./native/macos/cue-overlay/build.sh`.
- Compiled the Windows overlay smoke binary with MinGW.
