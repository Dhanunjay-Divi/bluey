# Round 227 - Canvas Sidebar Double-Click Guard

Date: 2026-06-28 02:21 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner pointed at the sidebar/canvas header icon and reported that clicking it twice caused Bluey to expand.

## Root Cause

The canvas/sidebar button opens the canvas and can cause the overlay layout/window width to shift. A rapid second click from the same physical mouse position could then land on newly exposed canvas chrome, including the canvas full-window expand control, making it feel like double-clicking the sidebar icon expanded Bluey.

History had the same general rapid-toggle risk, where a double click could immediately open and close the drawer.

## Fix

- Added a short rapid-repeat guard for the History toggle.
- Added a short rapid-repeat guard for the canvas/sidebar toggle.
- When the canvas is opened from the header button, temporarily suppress canvas full-window expansion for 450ms.
- Normal single-click behavior remains unchanged:
  - one click opens canvas
  - later click closes canvas
  - explicit canvas expand still works after the short guard window

## Mac / Windows Parity

- The bug was in the macOS overlay header/canvas layout.
- Windows does not currently have the same macOS sidebar/canvas header interaction path.
- Windows overlay syntax was still checked in this round to keep parity coverage.

## Verification

Passed:

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
- `git diff --check`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

## Current State

- Local visible QA overlay was restarted from the rebuilt debug binary:
  - daemon pid `41825`
  - overlay visible `true`
  - overlay capture excluded `false`
  - screen capture active `false`

## Remaining QA / Gates

- Double-click the canvas/sidebar icon and confirm it does not expand the canvas/window.
- Confirm one normal click still opens canvas.
- Confirm clicking the canvas expand button directly still works after the guard window.
- Before release/upload, return visible QA mode to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
