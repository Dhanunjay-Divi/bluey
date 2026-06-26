# Round 054 - Overlay UX Declutter Pass - 2026-06-17

## Goal

Make the expanded Bluey overlay feel calmer and closer to the compact Pinky overlay language: fewer always-on status labels, less empty-state noise, and more room for the actual question/answer surface.

## Reference

Compared Bluey against `/Users/uno/Downloads/pinky-git/cmd/pinky-host-overlay/main.swift`. Pinky keeps the toolbar compact and lets the content carry the experience. Bluey had too many simultaneous status surfaces: brand subtitle, route state, empty docs state, balance, mode controls, and an instructional empty card.

## Changes

- Header subtitle now hides by default. It only appears for meaningful temporary states such as sign-in, screen analysis, answer streaming, or audio issues.
- `Docs empty` and `Docs locked` no longer render in the header. The document badge now appears only while indexing or when documents are actually ready.
- Header geometry was tightened: smaller icon hit visuals, shorter route/doc badges, and narrower balance label while preserving clickable hit areas.
- Empty recording state was reduced to one clear prompt plus a compact document drop target.
- Visual smoke source contracts were updated so tests enforce the new uncluttered header instead of pinning the old empty docs label.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
- `bash native/macos/cue-overlay/build.sh`
- `bash scripts/macos-overlay-visual-smoke.sh`

Visual smoke result:

- Expanded overlay stayed stable at `820x520`.
- Header visibility pixel check passed with the quieter header.
- Screenshot: `/tmp/bluey-smoke-shots/macos-overlay-visual-smoke.png`

## Notes

The smoke uses `BLUEY_DEV_OVERLAY=1` and `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` intentionally for QA screenshots. Release overlay builds still ignore the capture-visible escape hatch.
