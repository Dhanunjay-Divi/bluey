# Round 267 - Keyboard Shortcuts Disclosure

## Trigger

The overlay needed a visible keyboard/shortcuts entry point beside the theme control, with platform-specific shortcut copy. The Mac shortcuts also needed a live sanity test after the Round 266 shortcut routing work.

## Fix

- Added a macOS header keyboard icon beside the theme icon.
- Added an in-overlay macOS shortcuts panel using the existing Bluey modal surface, not a system popup.
- The macOS panel shows:
  - `Ctrl+Option+B/T/L/S/I/Enter`
  - local `L/S/I/H/F/Esc` when Ask is not focused
  - a note that typing always wins inside Ask
- Added a Copy action for the shortcut list.
- Added a Windows `Keys` header button beside Theme.
- Added Windows-specific shortcut copy:
  - `Ctrl+Alt+B/T/L/S/I/Enter`
  - local `L/S/I/H/F/Esc` when Ask is not focused
- Updated Windows Help copy to point users to the Keys button.

## Verification

Passed:

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
git diff --check
BLUEY_OVERLAY_SWIFT_CONFIGURATION=release bash native/macos/cue-overlay/build.sh
codesign --verify --verbose=2 ~/.bluey/bin/bluey-overlay-macos ~/.bluey/bin/cue-overlay-macos
```

Live macOS checks:

- Rebuilt and hot-installed the macOS overlay into `~/.bluey/bin`.
- Restarted Bluey successfully.
- Verified `Ctrl+Option+B` through `System Events`: first press moved `bluey status` to `overlay_visible=false`; second press returned it to `overlay_visible=true`.
- Exercised `Ctrl+Option+I` twice and `Ctrl+Option+Enter` on an empty session; daemon stayed healthy and no transcript or context rows were created.
- Verified the installed signed binary contains the shortcut panel strings.

Limit:

- A Swift Accessibility focused-element probe returned `AX` error `-25204`, so this terminal could not prove `Ctrl+Option+T` focus via AX. The routing code and installed binary are present, but final visual/manual confirmation should happen from the live overlay.

## Current State

Mac local overlay is installed and running with the new keyboard icon and shortcut sheet. Windows source has parity but still needs a real Windows host build/smoke.
