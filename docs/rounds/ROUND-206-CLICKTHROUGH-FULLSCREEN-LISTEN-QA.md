# Round 206 - Clickthrough Fullscreen Listen QA

## Trigger

The owner reported four live overlay issues in visible QA mode:

- click-through did not let all blank spaces click the app behind Bluey
- full-screen did not expand all the way to screen borders and restore should return to compact
- panel edges/gaps felt too large
- Listen felt non-realtime, inaccurate, or doubled captions

## Root Cause

Round 204 intentionally changed the emergency behavior so the expanded panel always received mouse events and blank space dragged the window. That restored dead buttons and movement, but it made the click-through toggle behave like "move anywhere" instead of true blank-space passthrough.

The full-screen button was also still using focus-size metrics, not the full screen frame, and the normal expanded frame retained a 32px screen inset.

For Listen, a direct local test showed the daemon can start native live STT and emit transcript segments. The weak areas are live feedback clarity and duplicate mic/system echo suppression when both sources hear the same words.

## Fix

- Restored distinct macOS overlay modes:
  - Interactive mode: blank Bluey space receives mouse events and moves/resizes the panel.
  - Click-through mode: blank Bluey space returns no hit and the window ignores mouse events there.
  - In click-through mode, real controls stay clickable and the Bluey logo/wordmark is the explicit move handle.
- Added an interaction-mode callback so the window mouse policy updates immediately after toggling click-through.
- Kept the History drawer and open canvas interactive in click-through mode so scrolling those panes does not leak to the app behind Bluey.
- Changed macOS full-screen to use the actual screen frame, set zero corner radius while full-screen, and restore to Bluey's compact default frame.
- Reduced expanded-panel screen inset from 32px to 12px and tightened fixed chrome gaps/insets.
- Mirrored Windows blank-space behavior to match its help text: controls return `HTCLIENT`, logo/wordmark returns `HTCAPTION`, blank expanded space returns `HTTRANSPARENT`.
- Widened cross-source transcript echo dedupe from 2.5s to 6s so delayed mic/system duplicate finals are more likely to be suppressed.

## Verification

Commands run:

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c
cargo test -p cue-daemon duplicate_transcript_detection -- --nocapture
cargo build -p cue-cli -p cue-daemon
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh
BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh
target/debug/bluey audio status
```

Results:

- Swift parse passed.
- Windows C syntax-only check passed.
- Transcript duplicate tests passed.
- Debug daemon/CLI build passed.
- Debug macOS overlay build passed.
- Local visible overlay relaunched with `--bluey-dev-overlay --bluey-local-visible-overlay --bluey-overlay-capture-visible`.
- Audio status after relaunch is idle/ready with native helper installed.

## Current State

Bluey is currently running in local visible/debug QA mode from:

```bash
/Users/uno/Downloads/cue/target/debug/bluey-daemon
/Users/uno/Downloads/cue/native/macos/cue-overlay/.build/BlueyOverlay.app/Contents/MacOS/bluey-overlay-macos
```

Return to normal capture-excluded mode with:

```bash
target/debug/bluey off
bluey on
```

## Remaining QA

- Manually test click-through with the mode button:
  - blank top/header/body/bottom space should click the app behind Bluey
  - buttons/composer/history/canvas controls should still click
  - logo/wordmark should move the panel
- Manually test full-screen and restore on the current display.
- Speak into Listen for at least 10-15 seconds and confirm live captions appear quickly and no obvious mic/system duplicate remains.
- If Listen still feels delayed, add stronger live status telemetry around relay websocket open, first audio byte, first provider event, first transcript partial, and first final.
