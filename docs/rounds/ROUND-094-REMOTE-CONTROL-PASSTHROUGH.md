# Round 094 - Remote-Control Passthrough Round - 2026-06-21

## Context

The request was to make Bluey avoid stealing clicks when a remote-control operator is driving the host machine. This is a host UX safety feature: the remote user should click the underlying app, not accidentally hit Bluey controls.

This is not a stealth or bypass feature. Bluey keeps the existing macOS capture-exclusion posture, but does not try to identify or block arbitrary third-party people. macOS does not reliably label every remote-control vendor's mouse events as remote input.

## What Changed

- Added `OverlayCommand::SetPassthrough { enabled, duration_ms }` to the shared overlay protocol.
- The macOS overlay now accepts both `set_passthrough` and `input_passthrough` inbound commands.
- The macOS overlay arms a short remote-input passthrough window that sets both pill and expanded Bluey windows to ignore mouse events, then restores normal host controls automatically.
- Added a trusted Pinky-compatible event monitor:
  - Pinky remote-control input on macOS is tagged with `eventSourceUserData = 0x70696e6b797231`.
  - Bluey recognizes that marker and briefly passes mouse events through, so Pinky-driven remote clicks do not activate Bluey controls.
  - Re-arming is throttled to avoid lifecycle log spam during remote mouse movement.

## Guarantees

- Bluey-owned or Pinky-tagged remote input can pass through Bluey without clicking Bluey controls.
- Host controls recover automatically after the short passthrough window.
- Existing manual click-through mode remains available for third-party remote-control tools that do not tag input.
- The existing screen-capture exclusion behavior is unchanged.

## Known Boundary

For remote-control tools that inject events indistinguishably from local host input, Bluey cannot reliably know whether the host or remote person clicked. In that case the safe product control is explicit click-through mode, plus host-side shortcuts/CLI to restore interactivity.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
- `cargo test -p cue-core overlay -- --nocapture`

## Likely Follow-Up

- Add the same trusted-input passthrough guard to the future Windows Bluey overlay before advertising Windows as ready.
- If Bluey later ships its own remote-control path, call `input_passthrough` before injecting host mouse input, matching Pinky's pre-arm pattern.
