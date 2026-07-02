# ROUND-303-AUTOSEND-STOP-CANCELS

Date: 2026-07-02
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Fix the bad Listen/Auto-send behavior where clicking Stop could submit a delayed transcript. Stop must mean stop/cancel, not "answer late".

## Changes

- macOS auto-send no longer schedules from explicit Listen Stop.
- macOS auto-send now schedules only after a final caption settles for 900ms while Listen is still active.
- macOS Stop, paused state, and pill Stop cancel pending auto-send work and clear only the auto-send buffer.
- macOS auto-send copy now says captions settle, and Stop cancels pending auto-send.
- macOS bumped the auto-send preference version to reset old stored stop-triggered choices back to off.
- Windows auto-send defaults to off instead of system-stop.
- Windows final captions schedule a 900ms settle timer only while recording is active.
- Windows Stop, transcript clear, and session switch cancel pending auto-send timers.
- Windows menu/tooltip copy now says captions settle, and Stop cancels pending auto-send.

## Behavior Contract

- Pressing `Listen` starts capture and prepares an auto-send buffer.
- Final captions can auto-send only if auto-send is enabled, the selected source matches, and Listen is still active after the settle delay.
- Pressing `Stop` cancels any pending auto-send and does not send the transcript.
- The transcript remains available for a manual `Answer`/Enter path unless the user clears it.

## Verification

- `swift build -c debug --package-path native/macos/cue-overlay`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c`
- `git diff --check`
- `rg` scan confirmed old stop-trigger strings and stop-trigger function names are gone.
