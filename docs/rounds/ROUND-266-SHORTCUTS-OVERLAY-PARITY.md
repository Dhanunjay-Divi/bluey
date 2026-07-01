# Round 266 - Shortcuts Overlay Parity

## Trigger

The shortcut model was getting confusing: direct letters risked stealing typing, old `Ctrl+Shift` chords collide with common app/browser/dev shortcuts, and Mac/Windows overlay behavior needed one clear rule.

## Decision

Use one global Bluey chord family on both platforms:

- macOS: `Ctrl+Option+key`
- Windows: `Ctrl+Alt+key`

Global actions:

- `B` shows or hides Bluey.
- `T` focuses the Ask field.
- `L` toggles Listen.
- `S` captures screen context.
- `I` toggles click-through/interactive mode.
- `Enter` sends Answer.

Local overlay-only direct letters are allowed only when Bluey is interactive, the full overlay is visible, and the Ask/text editor is not focused:

- `L` Listen
- `S` Screen
- `I` Interactive/click-through
- `H` History
- `F` Files
- `Esc` close/cancel the active local panel

F19 remains only a legacy fallback where already wired; it is no longer the taught shortcut.

## Fix

- Added macOS global key routing with `NSEvent.addGlobalMonitorForEvents`.
- Added macOS local shortcut routing before the composer auto-focus path, while preserving normal typing inside Ask and other text editors.
- Updated the macOS hidden toast to teach `Ctrl+Option+B`.
- Added Windows `RegisterHotKey` support for `Ctrl+Alt+B/T/L/S/I/Enter`.
- Added Windows local direct shortcuts when the Ask box is not focused.
- Added Windows `g_interactive_mode`: click-through mode sends blank space to the app behind Bluey; interactive mode lets blank space drag Bluey.
- Updated the older dashboard accelerator registrations away from `Ctrl+Shift`.
- Documented the shortcut model in `README.md`.

## Verification

Passed:

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
/opt/homebrew/bin/x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c
cargo check -p cue-dashboard --quiet
git diff --check
BLUEY_OVERLAY_SWIFT_CONFIGURATION=release bash native/macos/cue-overlay/build.sh
codesign --verify --verbose=2 ~/.bluey/bin/bluey-overlay-macos ~/.bluey/bin/cue-overlay-macos
~/.bluey/bin/bluey status
```

## Current State

Source is ready for Mac and Windows parity testing. The Mac overlay was rebuilt, hot-installed into `~/.bluey/bin`, ad-hoc signed, and restarted locally. `bluey status` shows the daemon running, overlay visible, and capture excluded.

## Remaining QA

- Live-test macOS shortcuts in both click-through and interactive modes.
- Confirm Windows hotkey registration on a Windows build host.
- Decide whether the UI should visibly label the current shortcut chord in the help drawer or tooltips.
