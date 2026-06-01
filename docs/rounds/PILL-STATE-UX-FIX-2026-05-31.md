# Pill State UX Fix - 2026-05-31

## Context

The collapsed Bluey pill was still too large and behaved like a static logo. When the expanded overlay collapsed back to the pill, users could not tell whether Bluey was ready, connecting, actively listening, paused, or failed without reopening the full panel.

## Changes

- Reduced the native macOS collapsed pill from `128x38` to `112x34`.
- Reworked the collapsed surface into a compact Bluey identity segment with an attached mini-control rail.
- Added direct collapsed controls:
  - Style opens the inline answer-style editor in the expanded panel.
  - Play / pause toggles listening without opening the full panel.
  - Power opens the full panel and shows the existing turn-off confirmation.
- Added a compact state indicator:
  - Ready / paused: play.
  - Listening: pause with green status.
  - Connecting: amber activity glyph.
  - Failed: red warning glyph.
- Kept the Bluey logo, label, and status dot, but tightened spacing so the dot sits close to the wordmark.
- Wired `listening_state_changed` overlay IPC into the native pill and expanded panel.
- Kept the current run state in the overlay coordinator so expand/collapse preserves the visible state.
- Made transcript partial/final IPC update the live captions strip without forcing the expanded window open.

## Product Behavior

- `bluey on` starts with the compact pill centered by default.
- Clicking the pill opens the full workspace.
- Hiding/collapsing the workspace returns to the pill.
- If Bluey is actively listening, the collapsed pill remains small but shows the listening state and exposes pause.
- Passive daemon updates do not reopen the expanded overlay.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay` passes.
