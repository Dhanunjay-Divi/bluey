# Round 045 - Overlay Interaction Mode - 2026-06-16

## What Changed

- Added a header control beside the full-window button for switching between:
  - **Interactive on**: all Bluey controls, composer, resize, drawer, canvas, and buttons are clickable.
  - **Click-through on**: host app clicks pass through Bluey except text/feed/canvas regions, transcript scrolling, copy controls, and the mode toggle.
- Added in-window toast feedback when the mode changes.
- Reused the existing answer-card `cost_label` path for in/out token, latency, and cost display. The overlay now keeps that status visible where the daemon/server provide it.

## Why

The prior expanded window forced `ignoresMouseEvents = false` continuously, which made Bluey safe for buttons but impossible to use as a light pass-through overlay. The product needs both modes:

- Active editing/testing mode where every button works.
- Host-friendly pass-through mode where Bluey can stay visible without stealing ordinary clicks.

## Verification

- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh`
- `swift build -c release --package-path native/macos/cue-overlay`
- `git diff --check`

## Reviewer Notes

- Pass-through is implemented by the existing tracking timer using the current mouse location. Normal interactive mode keeps the window fully clickable.
- Pass-through keeps the mode toggle available so users can return to interactive mode without restarting Bluey.
- Modal overlays and the session drawer remain fully interactive to avoid trapping the user in a half-clickable state.
