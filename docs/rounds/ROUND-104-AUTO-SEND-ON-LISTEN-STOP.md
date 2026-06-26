# Round 104 - Auto Send On Listen Stop - 2026-06-22

## Goal

Make the normal Listen flow feel lighter: when the user explicitly stops audio capture, Bluey can send the current typed question, transcript, screen context, or attached files automatically.

## Changes

- Added a default-on auto-send toggle beside the answer controls in the macOS overlay.
- Scoped auto-send to the explicit Stop/Listen button path, not idle auto-stop or background daemon state changes.
- Added a short delay on macOS after Stop so the final caption event has time to arrive before Answer is sent.
- Mirrored the same default-on toggle in the Windows overlay for parity.

## Verification

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `swiftc -typecheck native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
