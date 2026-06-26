# Round 188 - Overlay Drag Anywhere Canvas Scroll

## Trigger

Owner said the earlier click-through work made blank top/bottom/chrome areas too hard to use, and asked for Bluey to move easily by clicking and holding anywhere on the overlay. Owner also asked that canvas/full-size modes stay small and that scrolling feel smooth.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`  
Workspace: `/Users/uno/Downloads/cue`  
Round completed: 2026-06-26 08:34 EDT

## Root Cause

- macOS had two separate gates:
  - the window-level mouse policy only accepted mouse events over known controls in click-through mode
  - the expanded panel hit-test returned `nil` for blank header, composer chrome, drawer, transcript, and feed/canvas space
- That made blank overlay areas pass clicks to the app behind Bluey, but it also meant those areas could not reliably start a window drag.
- Windows had the same shape at the OS hit-test layer: blank expanded overlay areas returned `HTTRANSPARENT`.
- macOS canvas and full-size expansion paths still used screen-filling frames, which could make the overlay feel too large for an always-on-top assistant.

## Implemented

- macOS:
  - Changed expanded panel hit testing so real controls still receive clicks, while blank Bluey surface returns the panel itself and can start a drag.
  - Changed the window-level mouse policy so any point inside the expanded Bluey panel can receive mouse events in move mode.
  - Kept composer text input clickable/focusable; clicking composer chrome outside the text area now starts a drag.
  - Kept selectable canvas text and canvas/session/composer scrollbars interactive.
  - Routed wheel/trackpad scrolling directly to the feed and canvas even in move/click-through mode.
  - Updated interaction-mode tooltip/toast copy from old click-through wording to move-mode wording.
  - Changed full-size and canvas expansion from true screen-filling frames to a bounded centered focus size.
  - Reduced the bounded canvas/full-size maximum width to `1120`.
  - Clamped restored/saved expanded frames to the new bounded envelope.
- Windows:
  - Changed expanded overlay `WM_NCHITTEST` so controls return `HTCLIENT`, while blank overlay surface returns `HTCAPTION`.
  - Removed the stale header-only drag helper.
  - Updated the Windows help text to say blank Bluey space can be dragged and controls stay clickable.
- Local macOS install:
  - Rebuilt the macOS overlay bundle.
  - Refreshed `~/.bluey/bin/bluey-overlay-macos`, `~/.bluey/bin/cue-overlay-macos`, and `~/.bluey/bin/BlueyOverlay.app`.
  - Restarted only the overlay child process and confirmed `bluey overlay show` returned `ok`.

## Files Touched

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-188-OVERLAY-DRAG-ANYWHERE-CANVAS-SCROLL.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Verification

Passed:

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
- `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- killed the previous overlay child so the daemon respawned the refreshed app
- `~/.bluey/bin/bluey overlay show`
- `~/.bluey/bin/bluey status`

## Current State

- macOS installed local overlay is refreshed and visible from the running daemon.
- Current daemon PID observed after refresh: `93283`.
- Current overlay child observed after refresh: `51809`.
- Overlay status after refresh:
  - visible: `true`
  - opacity: `0.94`
  - capture excluded: `false` because this is local visible test mode
- Blank Bluey surface is now the drag handle. This intentionally replaces the previous blank-space click-through behavior for the expanded panel.
- Actual controls still click, and text/scroll regions that need interaction remain interactive.

## Remaining QA Gates

- Manual macOS feel test:
  - drag from header blank space
  - drag from feed blank space
  - drag from bottom/composer chrome outside the text field
  - confirm real controls still click
  - confirm composer text still focuses and text selection/cursor placement works
  - confirm feed and canvas scroll with trackpad/wheel
  - confirm canvas expansion stays bounded instead of full screen
- Manual Windows feel test on a Windows build:
  - blank overlay surface drags via OS `HTCAPTION`
  - controls still click
  - old edge resize expectations are acceptable after drag-anywhere behavior
- If owner wants true behind-app click-through and drag-anywhere at the same time, add an explicit mode or modifier because the same blank left-click cannot both pass through and start a window drag.
