# Overlay Compact Conversation Pass - 2026-06-17

## Goal

Make the expanded Bluey overlay feel closer to a compact AI conversation surface:

- lighter composer chrome with more room for questions and answers
- clearer transcript strip behavior
- reliable click targets without the overlay stealing the whole screen in pass-through mode
- a calmer first-run empty state

## Changes

- Compacted the composer row:
  - reduced the text box, Listen, Answer, Tone, Auto, Screen, attach, and opacity control heights
  - tightened horizontal spacing while keeping manual hit padding around buttons
  - dimmed non-primary button chrome so the bottom bar reads less heavy
- Changed transcript strip semantics:
  - `TRANSCRIBING` displays as `LIVE`
  - idle/paused/ready paths display as `READY` instead of `IDLE`
  - repeated Mic/System labels are suppressed until the audio source changes
- Narrowed pass-through hit testing:
  - pass-through mode now catches only explicit controls, the composer, the live caption strip, and copy affordances
  - broad feed/canvas/header regions no longer consume unrelated host-app clicks
- Simplified the empty state:
  - replaced the large `New recording` panel with a compact `Ready` state
  - removed the four chunky capability chips
  - kept one drop zone for documents and a short user-facing hint
- Updated the macOS visual smoke contract for the new compact opacity width.

## Verification

```bash
swift build -c release --package-path native/macos/cue-overlay
bash native/macos/cue-overlay/build.sh
bash scripts/macos-overlay-visual-smoke.sh
```

Visual smoke result:

- expanded overlay bounds stable at `820x520`
- header visible
- capture-visible debug screenshot generated at `/tmp/bluey-smoke-shots/macos-overlay-visual-smoke.png`

## Notes For Review

- This pass intentionally does not rework the full conversation renderer. It only reduces chrome weight and fixes interaction boundaries.
- Pass-through mode still allows text/copy interactions only where explicit controls exist. Interactive mode remains fully clickable and movable.
- `bluey-dev.db` remains local-only and was not touched.
