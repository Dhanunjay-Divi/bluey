# Round 191 - History Drawer Scroll Capture

## Trigger

Owner opened History and tried to scroll the history chats, but the underlying chat conversation scrolled instead.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause

The macOS overlay scroll router checked the main feed and canvas before the History drawer. Because the drawer sits above the feed, points inside the drawer can still also be inside the feed rectangle underneath it. The feed won the wheel event first, so the visible history surface did not scroll.

The drawer also only caught wheel events over the inner `sessionScroll` rectangle. Scrolling over drawer padding, title, subtitle, or row chrome could still fall through.

## Fix

- Added a dedicated `SessionDrawerView` that forwards any wheel event inside the drawer to the drawer's `sessionScroll`.
- Reordered root overlay `scrollWheel` handling so a visible History drawer captures scroll before feed/canvas routing.
- Broadened History scroll capture from only the inner list rect to the entire drawer rect.

## Windows Parity

Windows has no equivalent scrollable History drawer in the current overlay. Its `Session` button opens a simple yes/no/cancel dialog for continue/start-clean behavior, so there is no Windows scroll routing bug to fix in this round.

I still ran a Windows overlay syntax check because this is an overlay behavior round.

## Verification

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `native/macos/cue-overlay/build.sh`

## Local Install

Installed refreshed macOS overlay artifacts into `~/.bluey/bin`:

- `native/macos/cue-overlay/.build/bluey-overlay-macos`
- `native/macos/cue-overlay/.build/cue-overlay-macos`
- `native/macos/cue-overlay/.build/BlueyOverlay.app`

Restarted only the overlay child process, then confirmed:

- `~/.bluey/bin/bluey overlay show`
- `~/.bluey/bin/bluey status`

Current local status after overlay refresh:

- daemon pid `79599`
- active meeting id `fbdb0894-1212-4fde-87cd-c42168e25009`
- overlay visible `true`
- overlay capture excluded `true`
- overlay position `center`
- overlay opacity `0.94`

## Current State

- When History is visible, scroll/wheel gestures inside the drawer should move the history list instead of the underlying chat.
- Scrolling over drawer header/padding should still scroll the history list.
- Main chat and canvas scrolling remain unchanged outside the drawer.

## Remaining QA

- Manual macOS check with enough saved recordings to overflow the History drawer.
- Confirm scrolling over the drawer title, list rows, empty padding, and inner scrollbar all moves History.
- Confirm scrolling outside the drawer still scrolls the main chat or canvas as expected.
