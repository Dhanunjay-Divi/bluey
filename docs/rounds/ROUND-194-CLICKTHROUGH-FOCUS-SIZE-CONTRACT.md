# Round 194 - Click-Through Focus Size Contract

## Trigger

Owner said click-through and fullscreen still did not feel right. The specific expectation was that Bluey should behave like a useful overlay: blank space should not block the app behind it when click-through is enabled, and large/canvas modes should not become an awkward full-screen takeover.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Workspace: `/Users/uno/Downloads/cue`
Round completed: 2026-06-26 14:44 EDT

## Root Cause

- Round 188 made click-through mode behave like drag-anywhere mode.
- On macOS, `ExpandedPanelView.isInteractiveAtLocalPoint` still treated almost the whole expanded panel as mouse-active while `passThroughMode` was enabled.
- That meant blank header, feed, drawer, composer chrome, and bottom-bar space could still intercept clicks instead of passing them to the app behind Bluey.
- The product contract had become internally contradictory: the same blank left-click cannot both pass through to the app behind Bluey and start a Bluey window drag.
- Windows had the same policy problem at the OS hit-test layer because blank expanded overlay surface was being treated as draggable overlay chrome.
- The old fullscreen/canvas code could restore or expand into screen-filling frames, which is too heavy for an always-on-top assistant.

## Implemented

- macOS click-through contract:
  - Click-through mode now means blank Bluey surface passes clicks to the app behind Bluey.
  - Real controls, chips, buttons, composer input, canvas controls, scroll views, drawers, and modal overlays remain clickable.
  - The Bluey logo/wordmark area stays mouse-active as the intentional move handle while click-through is on.
  - Interactive mode is now the mode for moving/resizing from blank overlay space.
  - Tooltip/toast copy now explains the difference between click-through and interactive mode.
- macOS focus-size contract:
  - Full-size button copy changed to focus-size language.
  - Canvas/focus expansion now uses a bounded centered frame instead of a true fullscreen frame.
  - Saved/restored expanded frames are clamped back into the bounded focus envelope.
- Windows parity:
  - Expanded overlay hit-testing now keeps controls clickable.
  - Blank expanded surface returns `HTTRANSPARENT` in click-through behavior.
  - The Bluey logo/wordmark area returns `HTCAPTION` so the window can still be moved deliberately.
  - Saved/restored expanded placement is clamped to a bounded focus area.
  - Windows help text now matches the new contract.
- Local macOS install:
  - Rebuilt the macOS overlay bundle.
  - Refreshed `~/.bluey/bin/bluey-overlay-macos`, `~/.bluey/bin/cue-overlay-macos`, and `~/.bluey/bin/BlueyOverlay.app`.
  - Restarted only the overlay child process and confirmed `bluey overlay show` returned `ok`.

## Files Touched

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `native/windows/cue-overlay/main.c`
- `docs/rounds/ROUND-194-CLICKTHROUGH-FOCUS-SIZE-CONTRACT.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Verification

Passed:

- `git diff --check -- native/macos/cue-overlay/Sources/cue-overlay/main.swift native/windows/cue-overlay/main.c`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `native/macos/cue-overlay/build.sh`
- `install -m 755 native/macos/cue-overlay/.build/bluey-overlay-macos ~/.bluey/bin/bluey-overlay-macos`
- `install -m 755 native/macos/cue-overlay/.build/cue-overlay-macos ~/.bluey/bin/cue-overlay-macos`
- `rm -rf ~/.bluey/bin/BlueyOverlay.app && cp -R native/macos/cue-overlay/.build/BlueyOverlay.app ~/.bluey/bin/BlueyOverlay.app`
- killed the previous overlay child so the daemon respawned the refreshed app
- `~/.bluey/bin/bluey overlay show`
- `~/.bluey/bin/bluey status`

Latest observed local daemon status:

- daemon pid: `91017`
- overlay visible: `true`
- overlay opacity: `0.94`
- overlay capture excluded: `true`
- screen capture active: `false`
- active meeting id: `58424518-6b07-4828-8676-5622a32ddc78`
- context items: `5`

## Current State

- Click-through on:
  - blank Bluey space clicks the app behind it
  - Bluey controls still click
  - drag the Bluey logo/wordmark to move the overlay
- Interactive on:
  - blank Bluey space belongs to Bluey
  - blank space can move/resize the panel
  - controls and text remain clickable
- Full-size/canvas:
  - opens as a bounded focus-size overlay, not a true OS fullscreen takeover
  - restore paths clamp oversized saved frames back into a usable size

## Remaining QA Gates

- Manual macOS feel test:
  - click blank header/feed/body/bottom-bar space over an app behind Bluey and confirm the behind app receives the click
  - drag from the Bluey logo/wordmark in click-through mode
  - click real controls in click-through mode
  - switch to interactive mode and confirm blank-space move/resize works
  - open canvas/focus size and confirm it stays bounded and scrolls smoothly
- Manual Windows build test:
  - blank expanded surface passes through
  - logo/wordmark moves the window
  - controls still click
  - restored expanded placement is bounded instead of fullscreen
