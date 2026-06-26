# Round 099 - Terminal Persistence + Document Chip Polish - 2026-06-21

## User Goal

Keep Bluey running after the terminal closes, keep Bluey private from normal screen capture surfaces, and make attached documents use much less overlay space while still exposing details on hover.

## What Changed

- Confirmed `bluey on` already launches `bluey-daemon` detached from the terminal:
  - daemon stdio is set to null
  - Unix/macOS uses `setsid()`
  - Windows uses detached process flags
- Compact document chips in the macOS overlay:
  - reduced attachment strip height from 34 to 28
  - reduced placeholder chip and real document chip height
  - removed the second-line `LOADED · TYPE` label from each chip
  - kept a small remove affordance
  - added macOS tooltips with full title, file type, and path
- Kept document chips as elongated capsules so they read as light context, not large cards.

## Safety Boundary

Bluey can exclude its own overlay from normal host screen-capture APIs for privacy, and the app should avoid capturing itself. This is not a guarantee against every remote-control or monitoring product, and Bluey should not implement selective stealth or input blocking designed to hide from another participant's remote-control/session-monitoring tool. The supported behavior is privacy-first capture exclusion plus explicit interactive/click-through modes for the local user.

## Files

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `crates/cue-cli/src/app.rs` reviewed for daemon detach behavior

## Verification

- `git diff --check`
- `swift build -c release --package-path native/macos/cue-overlay`
- Installed the rebuilt overlay to:
  - `~/.bluey/bin/bluey-overlay-macos`
  - `~/.bluey/bin/cue-overlay-macos`
- Ad-hoc signed both local overlay binaries.
- Restarted Bluey from the installed bundle.
- `bluey status` confirms:
  - `overlay_visible: true`
  - `overlay_position: "center"`
  - `overlay_capture_excluded: true`
