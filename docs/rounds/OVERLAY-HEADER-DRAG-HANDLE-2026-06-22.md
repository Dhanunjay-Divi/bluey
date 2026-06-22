# Overlay header drag handle

## Why

The expanded Bluey header should feel like a normal window title bar: hold any non-button area and move the overlay wherever it should sit.

The header already had drag behavior, but click-through mode could stop delivering pointer events after the cursor left the header during a drag. That made dragging feel unreliable.

## Change

- Track active header drags from the header bar.
- Keep the expanded overlay accepting pointer events while a header drag is in progress.
- Reset the drag state when the mouse button is released.
- Keep actual header buttons clickable instead of turning them into drag handles.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
