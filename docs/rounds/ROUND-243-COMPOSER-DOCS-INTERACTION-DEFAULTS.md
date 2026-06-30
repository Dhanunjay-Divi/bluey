# Round 243 - Composer Docs Interaction Defaults

## Trigger

The owner reported three overlay UX issues during live testing:

- clicking `Ask anything...` should immediately show the blue typing indicator
- attached documents should become visible after upload/send without requiring
  a `Show documents` click
- the overlay should default to interactive-off/click-through mode, while real
  controls and the composer stay clickable

The owner also asked that Mac changes keep Windows parity where applicable.

## Root Cause

- The custom macOS composer already used a cyan insertion point, but the empty
  placeholder could visually hide the native caret until typing began.
- macOS document context updates could keep the visible strip collapsed behind
  the files badge, especially around newly indexed context.
- macOS `passThroughMode` defaulted to `false`, so new overlay launches started
  in fully interactive mode instead of blank-space click-through mode.
- Windows drew context chips when present, but cleared them immediately after a
  send, which made attachments feel like they disappeared from the overlay.

## Fix

macOS overlay:

- Added an explicit focused blue blinking caret beside the empty composer
  placeholder.
- Added a focused blue border/shadow on the composer input surface.
- Kept the cursor as the normal arrow over the composer, matching the existing
  no-I-beam policy.
- Armed the same caret path from mouse clicks, keyboard-routed input, and
  programmatic focus.
- Defaulted `passThroughMode` to `true`, so new overlay windows start with
  blank Bluey space passing through to the app behind it.
- Preserved explicit controls, composer, resize edges, and header move handles
  as clickable in click-through mode.
- Revealed newly added context items automatically by setting the attachment
  strip to visible when new files/screens arrive.
- Kept attached context visible after send by leaving the saved-context strip
  open rather than requiring the user to click `Show files`.

Windows overlay:

- Kept visible context chips after a send instead of clearing them immediately.
- Verified the existing Windows ask box already forces the normal arrow cursor
  and blank overlay space passes through except controls/resize/brand drag.

## Verification

Passed:

```bash
git diff --check
native/macos/cue-overlay/build.sh
x86_64-w64-mingw32-gcc -municode -D_WIN32_WINNT=0x0601 -o /tmp/bluey-win-check/bluey-overlay.exe native/windows/cue-overlay/main.c -luser32 -lgdi32 -ld2d1 -ldwrite -luuid -lshell32 -lcomctl32
install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos "$HOME/.bluey/bin/bluey-overlay-macos"
install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos "$HOME/.bluey/bin/cue-overlay-macos"
"$HOME/.bluey/bin/bluey" off
"$HOME/.bluey/bin/bluey" on
"$HOME/.bluey/bin/bluey" status
```

Mac build output:

```text
Build complete! (35.73s)
.build/bluey-overlay-macos
```

## Current State

- Source changes are in:
  - `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - `native/windows/cue-overlay/main.c`
- The local repo branch remains `codex/bluey-overlay-spacing-20260626`.
- The rebuilt macOS overlay was installed into `~/.bluey/bin` and Bluey was
  restarted normally.
- Current post-restart status showed:
  - `overlay_visible: true`
  - `overlay_capture_excluded: true`
  - active daemon pid `59401`
  - active meeting id `56431f96-80c0-438f-94c0-e650e9b17064`

## Remaining QA/Gates

- Live-test in the restarted overlay:
  - click `Ask anything...` with empty composer
  - verify blue caret/focus ring appears immediately
  - attach/drop documents and verify chips appear without clicking files badge
  - send a question and verify attached docs remain visible
  - verify click-through blank spaces pass through while controls still click
- Package/sign Windows overlay from the normal Windows build environment before
  release, because the local MinGW check is only a smoke compile.
