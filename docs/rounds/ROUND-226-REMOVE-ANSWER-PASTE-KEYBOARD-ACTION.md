# Round 226 - Remove Answer Paste Keyboard Action

Date: 2026-06-28 02:12 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner pointed at the keyboard icon on answer cards and asked to remove it because it does not make sense and was breaking Bluey.

## Root Cause

The keyboard icon was the answer-card action for pasting Bluey's answer into the app behind the overlay. It had been changed from earlier confusing icons, but it still added another action button to every completed answer and could interfere with the intended simple answer-card controls.

## Fix

- Removed the macOS answer-card paste action button from the card layout.
- Removed the macOS `PasteCardButton` class, keyboard icon setup, paste-click handler, paste-success flash, and parent callback chain.
- Kept the normal copy button.
- Kept the canvas/code artifact button.
- Left the daemon `paste_text_requested` protocol path intact for compatibility with older clients/tests, but the current Mac overlay no longer exposes a UI entry point for it.
- Hid and disabled the Windows native `Paste answer` button so Windows matches the current product behavior.
- Removed the Windows help text line for `Paste answer`.

## Mac / Windows Parity

- macOS no longer renders the keyboard/paste action on answer cards.
- Windows no longer exposes the matching native paste-answer control.
- The backend protocol remains unchanged on both platforms.

## Verification

Passed:

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
- `git diff --check`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

## Current State

- Local visible QA overlay was restarted from the rebuilt debug binary:
  - daemon pid `34112`
  - overlay visible `true`
  - overlay capture excluded `false`
  - screen capture active `false`
- Answer cards should now show copy and canvas/code actions only.

## Remaining QA / Gates

- Ask a completed answer in the visible overlay and confirm the keyboard button is gone.
- Confirm copy still works.
- Confirm code/canvas artifact buttons still open the canvas.
- Before release/upload, return visible QA mode to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
