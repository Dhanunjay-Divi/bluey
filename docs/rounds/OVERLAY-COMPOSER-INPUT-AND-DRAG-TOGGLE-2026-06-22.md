# Overlay composer input and drag/toggle repair

## Why

The controls-only click-through pass made the composer feel too fragile, and the click-through button lost its visible on/off state. The top header also needed a clearer draggable affordance.

## Change

- Explicitly focus the composer and key the overlay window when the Ask anything field is clicked.
- Handle composer `Cmd+A`, `Cmd+C`, `Cmd+V`, and `Cmd+X` directly inside the composer text view.
- Restore the click-through toggle as a real two-state control:
  - click-through on: answer text, captions, and empty space pass through
  - interactive on: the whole panel accepts clicks for selection/scrolling/editing
- Restore changing click-through icons/tooltips/toasts.
- Add an open-hand cursor over the header drag bar.
- Keep header buttons clickable while non-button header areas remain draggable.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
