# ROUND-462 Listen Auto-Stop Countdown

Date: 2026-07-09
Backup thread: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## User Issue

Listen can auto-stop without enough visible warning, which makes it feel random. The user asked for a grace window, a visible final countdown, and a clear popup when Bluey stops listening to avoid STT billing.

## Changes

- Changed the default idle transcript guard from 5 minutes to 1 minute.
- Added a 10-second countdown window before auto-stop.
- Added a typed overlay IPC command: `audio_auto_stop_countdown`.
- The daemon emits countdown updates once per second only during the final countdown.
- The macOS overlay renders the countdown in the live captions rail with warning color.
- When the guard stops Listen, the overlay shows a warning toast and the rail says Listen auto-stopped to avoid STT billing.
- Added diagnostics for countdown start and auto-stop so support can trace why recording stopped.

## Notes

- Existing env overrides still work:
  - `BLUEY_AUDIO_IDLE_STOP_SECS`
  - `CUE_AUDIO_IDLE_STOP_SECS`
  - `BLUEY_AUDIO_IDLE_STOP_COUNTDOWN_SECS`
  - `CUE_AUDIO_IDLE_STOP_COUNTDOWN_SECS`
- This guard is based on time since the last transcript update, not raw audio energy. If audio is flowing but STT is not producing captions, the countdown still appears and stops Listen to avoid unnecessary billing.
- No deploy was performed in this round per user instruction.

## Verification

- `cargo test -p cue-core overlay --quiet`
- `cargo test -p cue-daemon duration_seconds_ceil --quiet`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh`
- `git diff --check`
