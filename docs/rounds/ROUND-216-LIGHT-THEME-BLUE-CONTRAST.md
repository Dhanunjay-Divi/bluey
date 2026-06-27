# Round 216 - Light Theme Blue Contrast

Date: 2026-06-27 01:45 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner showed the white/light overlay composer and said the blue was not visible enough.

The affected areas were the white-theme Bluey chrome and controls, especially the panel outline, plus button, Answer arrow/border, History border, screen-ready status, click-through/canvas active icons, and drag/drop highlight.

## Root Cause / Fix

- The dark theme uses a bright cyan glow that works well on black.
- The white theme reused the same pale cyan on grey glass surfaces, which made outlines and icon accents look washed out.
- Light mode did not have its own high-contrast blue tokens.

Implemented:

- Added dedicated macOS light-theme accent tokens:
  - `BlueyLightTheme.accent`
  - `BlueyLightTheme.accentBorder`
  - `BlueyLightTheme.accentSoft`
- Strengthened the macOS white-theme panel border and feed/drop-target borders.
- Routed white-theme control styling through the new accent tokens for:
  - plus/attach icon
  - Answer accent button
  - History button border/icon
  - active canvas button
  - click-through interaction button
  - screen-ready route badge
  - drop-highlight outline/shadow
- Kept the dark-theme neon cyan behavior unchanged.

## Mac / Windows Parity

- macOS overlay now uses darker, higher-contrast light-theme accent tokens.
- Windows overlay now has matching light-theme accent constants and uses them for:
  - owner-drawn button borders
  - light-theme pressed button fill
  - Direct2D header/composer strokes
  - GDI fallback header/composer strokes
- No backend or web changes were needed.

## Verification

Passed:

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
- `git diff --check`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

Latest local status after relaunch:

- daemon pid `45940`
- overlay visible `true`
- overlay capture excluded `false`
- overlay opacity `0.92`

## Current State

- Local visible/debug Bluey is running with the rebuilt macOS overlay.
- White-theme blue controls and outlines should now read as blue instead of pale grey-blue.

## Remaining QA / Gates

- Visually confirm the white theme in the running overlay against the owner's target screen.
- Before release/deploy, restart normal capture-excluded mode and run the visible-flag release hygiene scan.
