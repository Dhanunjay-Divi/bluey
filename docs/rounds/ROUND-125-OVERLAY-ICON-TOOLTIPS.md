# Round 125 - Overlay Icon Tooltips

Date: 2026-06-22

## Why

Several Bluey controls are icon-only in the compact header. Users should be able
to hover and immediately learn what the eye, click-through, resize, history,
canvas, attach, screen, and close controls do.

## Changes

- Expanded macOS overlay tooltip copy for the top bar and common action buttons.
- Added a tooltip to the draggable header area: "Drag this bar to move Bluey".
- Clarified icon-only controls:
  - eye: minimize Bluey to the small pill
  - resize: expand / restore Bluey
  - click-through: explains pass-through vs fully interactive mode
  - close: turn Bluey off
  - history, new recording, canvas, attach, screen, tone, listen, answer, balance
- Added native Win32 tooltip support for the Windows overlay using
  `TOOLTIPS_CLASSW`.
- Attached Windows tooltips to the question box and all visible overlay buttons.
- Linked `comctl32.lib` in the Windows overlay build script.

## Product Rule

Every icon-only or ambiguous control should explain itself on hover. The wording
should describe the user-visible effect, not the implementation.

