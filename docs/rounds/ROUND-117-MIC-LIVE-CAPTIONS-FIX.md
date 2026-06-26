# Round 117 - Mic Live Captions Fix - 2026-06-22

## What Changed

- Fixed the macOS microphone helper so it writes the real AVAudioEngine input buffer directly instead of routing mic audio through a converter path that could emit silent PCM.
- Added an explicit microphone permission check in the helper so Bluey fails loudly when TCC blocks mic access instead of silently streaming zeros.
- Updated the overlay live-caption seed text to `Mic + System: captions appear here.` so the active source label is clear and green.
- Kept separate latest live transcript memory per source so a mic partial and a system partial do not overwrite each other before the final caption arrives.
- Changed the auto-send control from a bare checkmark into `Auto-send ✓` on macOS and Windows.

## Verification

- `swiftc -typecheck native/macos/cue-audio/Sources/cue-audio/main.swift`
- `swiftc -typecheck native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `bash native/macos/cue-audio/build.sh`
- `bash native/macos/cue-overlay/build.sh`
- Direct helper smoke after rebuild: mic PCM changed from all-zero to real signal (`rms 1835.9`, `peak 16708`, `98.04%` nonzero samples).
- Live Bluey smoke: `bluey audio start`, played a short spoken phrase, and `bluey audio status` reported 8 transcript segments with both system and mic/user sources in `active-meeting.json`.
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -municode -DUNICODE -D_UNICODE -Wall`

## Local Install

- Copied the rebuilt helper and overlay into `~/.bluey/bin`.
- Restarted Bluey locally.
- Stopped the test listener after verification so live STT does not keep running in the background.
