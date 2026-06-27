# Round 207 - Mac Header Badge Spacing

## Trigger

The owner shared a visible-mode screenshot where the expanded macOS overlay header still felt too spread out. The `Ready` route badge and `Show 5 files` badge were visually drifting across the toolbar instead of reading as one compact status group.

## Root Cause/Fix

- The macOS expanded header still allocated broad badge slots from the full middle toolbar width.
- That made short labels appear far apart because their text was centered inside oversized frames.
- Tightened the left header cluster:
  - reduced the history button width and nearby gaps
  - reduced the logo-to-wordmark gap
  - reduced the wordmark frame cap and post-brand gap
  - changed route/file badge widths to text-measured compact widths
  - reduced the badge gap from the older wider middle spacing to a compact 6px gap

## Windows Parity Check

Windows does not currently render the same `Ready` plus `Show files` header badge cluster in that position. No equivalent Windows spacing patch was needed for this specific macOS-only visual issue.

## Verification

```bash
swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift
BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh
BLUEY_BIN="$PWD/target/debug/bluey" scripts/bluey-visible-local.sh
target/debug/bluey status
```

## Current State

- Local visible/debug Bluey was relaunched after the rebuild.
- Visible mode remains for QA only:

```bash
target/debug/bluey off
bluey on
```

Use that to return to the normal capture-excluded production-style local overlay.

## Remaining QA/Gates

- Owner should visually confirm the tightened header spacing on the live overlay.
- Public downloadable binaries still need a new release publish if this visual polish should ship outside the local debug build.
