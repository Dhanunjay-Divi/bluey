# Overlay Utility Controls - 2026-06-15

## Goal

Tighten the macOS overlay controls after visual QA showed too many boxed controls and unreliable header clicks.

## Changes

- Flattened passive top-bar utilities:
  - navigation/sidebar icon
  - new session icon
  - app icon
  - route/status label
  - document status label
  - balance label
  - full-window icon
  - hide icon
  - close icon
- Flattened the bottom attach `+` control and opacity surface.
- Kept tactile button surfaces only for the active controls the user operates repeatedly:
  - Tone
  - Auto
  - Screen
  - Listen
  - Answer
- Changed the macOS answer button label to `Answer Cmd+Enter` via `Answer ⌘↵`.
- Added `Cmd+Enter` as a real key equivalent on macOS. Plain Enter still answers, and Shift+Enter still inserts a new line.
- Reworked header hit testing so empty header space drags the overlay window while nested controls remain clickable.

## Windows Note

The current implementation is the native macOS Swift overlay. The matching Windows overlay should use the Windows key equivalent requested by the user when that platform UI lands.

## Verification

- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh`
- `swift build -c release --package-path native/macos/cue-overlay`
- `git diff --check`
- Visible overlay QA with `BLUEY_DEV_OVERLAY=1 BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 ./target/debug/bluey on`
- Screenshot captured at `/tmp/bluey-debug/flat-controls-smoke.png`

## Follow-Up

Keep using the visual smoke path before shipping future overlay layout changes. The debug capture flag is only for local QA and must not be present in production artifacts.
