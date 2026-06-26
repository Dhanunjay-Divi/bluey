# Round 105 - Auto-Send Order And Stop Behavior - 2026-06-22

## What Changed

- Reordered the macOS bottom controls so the right-side sequence reads: `Auto-send`, `Auto`, `Screen`.
- Made `Auto-send` off by default on macOS and Windows.
- With the default state, pressing `Stop` only stops Listen and does not submit an answer.
- Updated tooltips to make the behavior clear: Auto-send is optional, not automatic surprise-submit behavior.

## Verification

- `swiftc -typecheck native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -municode -DUNICODE -D_UNICODE -Wall`
- `bash native/macos/cue-overlay/build.sh`
- Copied the rebuilt macOS overlay into `~/.bluey/bin` and restarted Bluey locally.
