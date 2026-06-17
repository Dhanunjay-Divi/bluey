# Overlay Header State Polish — 2026-06-17

## Goal

Remove duplicate audio state language in the expanded Bluey overlay header. The screenshot showed the brand subtitle saying `Recording off` while the center chip said `Paused`, which made the header feel noisy and contradictory.

## Changes

- Kept the brand subtitle calm and product-oriented: `Bluey online` for normal use, `Local ready` before sign-in, and `Audio issue` only when audio actually fails.
- Moved live audio state ownership to the center route badge: `Ready`, `Starting`, `Listening`, or `Audio`.
- Prevented short system toasts/cards such as recording start/stop messages from overwriting the brand subtitle.
- Stopped using `Paused` as the resting UI label after recording is off; the idle state now reads as ready.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
- `bash native/macos/cue-overlay/build.sh`
- `git diff --check`

## Notes

- This is UI polish only. No daemon protocol or billing behavior changed.
- `bluey-dev.db` remains local and untracked.
