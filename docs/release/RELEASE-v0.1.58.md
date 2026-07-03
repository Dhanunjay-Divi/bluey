# Bluey 0.1.58

## Focus

Tone editor focus clarity.

## Changes

- Added a clear blue typing indicator beside the Tone input on macOS.
- Tone input now uses a stronger blue border and glow while focused.
- Tone focus chrome refreshes on open, click/edit begin, edit end, save, dismiss, and theme changes.

## Verification

- `swift build -c debug --package-path native/macos/cue-overlay`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `cargo check -p cue-daemon --quiet`
- `cargo test -p cue-core overlay --lib`
