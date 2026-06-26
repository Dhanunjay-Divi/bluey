# Round 140 - Click-through and Auto-send menu round

## Goal

Make pass-through mode feel like true pass-through for the reading surfaces, while keeping the controls that users actually need usable.

## Changes

- Kept the composer, header drag, resize edges, menus, buttons, overlays, and session drawer interactive.
- Stopped answer text, canvas text, captions, and passive background panes from taking click or text-selection focus while pass-through is active.
- Prevented feed and canvas scroll routing while pass-through is active, so background apps can receive scroll gestures outside Bluey's active controls.
- Kept the Auto-send control compact when closed, while making the dropdown show the full choices:
  - Don't auto-send
  - Auto-send when mic stops
  - Auto-send when system stops
  - Auto-send when mic or system stops
- Widened only the dropdown menu, not the bottom toolbar button.

## Verified

- `./native/macos/cue-overlay/build.sh`
