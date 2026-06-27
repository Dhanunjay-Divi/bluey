# Round 208 - Overlay Resize Stability

## Trigger

The owner reported that the Bluey overlay window resizing still did not feel good after the header spacing pass.

## Root Cause/Fix

- The macOS top resize band could lose to header dragging because header drag was checked before resize edges.
- Click-through mode did not treat the visible border as an interactive resize target.
- Manual resize was still bounded by old focus/canvas limits, so the window could not grow to the available screen.
- The previous frame update could feel jumpy because screen clamping happened after the custom resize math.

Fixes:

- Edge resize now wins before header dragging on macOS.
- Visible borders can resize even when click-through is enabled; blank interior space still passes through.
- Full-screen mode disables manual edge resize until restored.
- Manual resize now anchors the opposite edge and clamps against the current screen before setting the frame.
- Resize frames are snapped to the backing pixel grid to avoid drift.
- Manual resize can shrink below the default compact width while the default open size remains familiar.
- Mac max resize now uses the available screen instead of the old canvas/focus cap.
- Windows parity:
  - adds border hit-testing for expanded resize
  - adds minimum track size
  - removes the old 1040x620 post-resize clamp so resized windows do not snap back to the focus size
  - clamps saved/restored expanded windows to the monitor work area

## Verification

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh
BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh
target/debug/bluey status
```

## Current State

- Local visible/debug Bluey was relaunched with the resize patch.
- Visible mode is still QA-only and capture-visible:

```bash
target/debug/bluey off
bluey on
```

Use that to return to normal capture-excluded local mode.

## Remaining QA/Gates

- Owner should manually test all four edges and four corners.
- Verify click-through mode:
  - blank interior clicks the app behind Bluey
  - logo/wordmark moves Bluey
  - visible border resizes Bluey
- Public binaries still need a release publish if this should ship to download users.
