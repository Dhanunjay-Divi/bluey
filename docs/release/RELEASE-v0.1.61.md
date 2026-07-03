# Release v0.1.61

Date: 2026-07-03
Round: `ROUND-322-CANVAS-SPLIT-NO-AUTO-EXPAND.md`

## Summary

This release fixes macOS overlay canvas/window ergonomics.

## Changes

- Canvas open/close no longer auto-resizes the outer Bluey overlay window.
- Canvas is now an adjustable split pane with a draggable cyan divider.
- Canvas split width is persisted across sessions.
- Click-through move handle gets drag priority before generic button routing.

## Verification

- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh`
- `cargo check -p cue-daemon`

## Live Release

- `https://bluey.sh/latest.json` reports `0.1.61`.
- macOS artifact SHA256:
  `51c2b45c4d98f64e19ef6c74d14f30268fccb086ce7c534f1e69bb6186df1c59`
- Live installer MIME checks passed.
- Live manifest signature verified.
- Unpacked macOS release binaries reported `0.1.61`.
