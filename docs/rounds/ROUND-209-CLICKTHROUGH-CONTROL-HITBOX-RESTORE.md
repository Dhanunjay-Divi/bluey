# Round 209 - Click-Through Control Hitbox Restore

## Trigger

The owner reported that after the resize/click-through work, Bluey buttons were no longer clickable while click-through mode was enabled. Expected behavior is:

- blank overlay interior passes through
- Bluey controls remain clickable
- the visible border remains resizable
- logo/wordmark remains movable

## Root Cause/Fix

- The macOS window-level mouse policy relied too much on AppKit's normal `hitTest`.
- Some Bluey controls are manually framed or manually routed, so the window could classify a point as blank even when it was inside a padded Bluey control zone.
- Short remote-input passthrough windows could also force the expanded overlay to ignore mouse events while the pointer was over a Bluey control.

Fixes:

- Added `hasManualInteractiveControl(at:)` and used it in both:
  - pass-through `hitTest`
  - window-level `shouldReceiveMouseEvents`
- Included manually routed buttons, opacity scrubber, and transcript clear hit zones in the click-through interactive region.
- Changed remote-input passthrough so it does not force the expanded overlay to ignore events when the pointer is currently over a Bluey interactive control.
- Added a short in-progress click hold window so a button mouse-down/mouse-up sequence is not interrupted if the pointer moves by a few pixels during the click.

## Windows Parity Check

No Windows change was needed in this round. Windows click-through and resize behavior is handled through `WM_NCHITTEST`, and Round 208 already makes controls return `HTCLIENT`, borders return resize handles, logo/wordmark return `HTCAPTION`, and blank space return `HTTRANSPARENT`.

## Verification

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh
BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh
target/debug/bluey status
```

## Current State

- Local visible/debug Bluey was rebuilt and relaunched with the click-through control-hitbox fix.
- Visible mode is still QA-only and capture-visible:

```bash
target/debug/bluey off
bluey on
```

Use that to return to normal capture-excluded local mode.

## Remaining QA/Gates

- Owner should manually test click-through mode:
  - History button clicks
  - new-session button clicks
  - file badge clicks
  - theme/fullscreen/click-through/hide/close buttons click
  - composer buttons click
  - opacity scrubber works
  - blank interior still clicks the app behind Bluey
  - border still resizes
- Public binaries still need a release publish if this should ship to download users.
