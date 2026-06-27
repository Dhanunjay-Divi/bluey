# Round 225 - Overlay Arrow Cursor and Input Caret

Date: 2026-06-27 18:57 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner showed the overlay using an I-beam cursor inside Bluey and asked for the normal mouse cursor everywhere within the overlay window, with a blue input indicator for typing.

Expected behavior:

- Hovering inside Bluey should use the normal arrow cursor, not an I-beam.
- Resize edges should still show resize cursors.
- If click-through/passthrough gives the mouse to an editor behind Bluey, the behind app may show its own editor cursor.
- Bluey's active text input caret should stay blue.

## Root Cause

- Some Bluey text views already tried to add arrow cursor rects, but AppKit could still restore the text I-beam after `mouseMoved`.
- Answer body labels were selectable `NSTextField`s, so AppKit treated them like text surfaces and showed an I-beam on hover.
- Theme refresh changed the composer insertion point from cyan to the normal text color.
- Windows had resize cursor handling on the main window, but the edit control could still request an I-beam cursor.

## Fix

- macOS text-view and text-field subclasses now force the arrow cursor on cursor updates, mouse move, and mouse drag after AppKit runs.
- macOS answer body labels now use the arrow-cursor text field subclass while remaining selectable/copyable.
- macOS overlay window now reapplies a top-level cursor policy after mouse move/cursor update dispatch:
  - resize cursor on resize edges
  - arrow cursor inside interactive Bluey surfaces
  - no forced cursor when passthrough leaves the pointer to the app behind Bluey
- macOS composer caret now stays on the themed blue accent.
- macOS Tone/rename field editors also receive the themed blue insertion point.
- Windows edit control now returns an arrow cursor instead of an I-beam.
- Windows main client-area cursor handling now explicitly returns arrow while preserving resize cursors.

## Mac / Windows Parity

- macOS received the full overlay cursor policy and blue caret fix because the reported issue was in the macOS overlay.
- Windows received matching no-I-beam behavior for the native edit control and client area.
- Windows custom blue caret was not added in this round because the current Win32 edit control uses the platform caret; adding a custom colored caret would require a larger owner-drawn input change.

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
  - daemon pid `8782`
  - overlay visible `true`
  - overlay capture excluded `false`
  - screen capture active `false`

## Remaining QA / Gates

- Manually hover the composer, answer text, canvas text, History drawer, Tone field, and rename field to confirm the I-beam is gone.
- Verify resize edges still show resize cursors.
- Verify click-through/passthrough blank areas still let the background app own its cursor.
- Before release/upload, return visible QA mode to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
