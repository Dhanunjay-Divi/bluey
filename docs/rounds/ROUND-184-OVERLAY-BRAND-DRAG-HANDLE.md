# Round 184 - Overlay Brand Drag Handle

## Trigger

Owner noticed that after making black header and bottom chrome truly click-through, the old behavior of clicking and holding the top or bottom bars to move the overlay no longer worked. Owner asked whether to restore the previous behavior or add a specific button/area for hold-and-move.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-26 06:13 EDT

## Root Cause

- The old move behavior used broad blank chrome as a drag handle.
- Round 183 intentionally made that blank chrome click-through, so restoring old top/bottom bar dragging would also restore the click-blocking dead zones.
- The overlay needed an explicit visible move handle that is not empty space.

## Fix

- Made the visible Bluey logo/wordmark area in the macOS header an explicit move handle while click-through mode is on.
- Added `Drag Bluey` tooltips to the macOS logo, wordmark, and brand stack.
- Kept all other blank header, bottom, border, drawer, and content chrome click-through in click-through mode.
- Added Windows parity by treating the visible logo/wordmark rectangle as `HTCAPTION` in `WM_NCHITTEST`.
- Kept Windows empty chrome transparent outside the brand drag handle, visible child controls, collapsed pill, and file-drag capture.

## Mac Windows Parity

- macOS: hold the Bluey logo/wordmark area to move the overlay in click-through mode.
- Windows: hold the matching Bluey logo/wordmark area to move the expanded overlay.
- Broad blank top/bottom bar dragging remains disabled in click-through behavior so empty space stays click-through.

## Verification

Passed:

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only native/windows/cue-overlay/main.c -municode`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
- `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- overlay child restart and daemon respawn check
- `~/.bluey/bin/bluey overlay show`

## Files Touched

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-184-OVERLAY-BRAND-DRAG-HANDLE.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Current State

- Empty chrome remains click-through.
- The visible Bluey brand area is now the intentional hold-and-drag target.
- The refreshed macOS loose binaries and `BlueyOverlay.app` bundle are installed in `~/.bluey/bin`.
- The visible local overlay has been restarted through the daemon supervisor.

## Remaining QA Gates

- Manually hold the Bluey logo/wordmark area and confirm the overlay moves in click-through mode.
- Manually click blank top/bottom/header/content chrome and confirm the click still reaches the app behind Bluey.
- Manual Windows GUI QA should confirm the brand drag area moves the overlay and blank expanded chrome stays transparent.
