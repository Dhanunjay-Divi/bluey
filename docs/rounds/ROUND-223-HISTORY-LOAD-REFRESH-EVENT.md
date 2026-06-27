# Round 223 - History Load Refresh Event

Date: 2026-06-27 18:12 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner opened the macOS History drawer and it stayed on `Loading...`, then asked how long it should take.

Expected behavior: local history should usually render in under a second. If it stays on `Loading...` beyond a couple seconds, the overlay missed the session list refresh or the daemon did not send one.

## Root Cause

- The macOS History drawer showed `Loading...` when it had not yet received sessions.
- Opening the drawer did not explicitly request a fresh session list.
- It depended on the daemon's startup `set_sessions` push. If that startup message was missed, delayed, or happened before the drawer was visible, the drawer could sit on `Loading...`.

## Fix

- Added a protocol event: `session_list_requested`.
- macOS now emits `session_list_requested` every time the History drawer opens.
- The daemon handles `SessionListRequested` by calling `refresh_overlay_sessions`, which sends `set_sessions` back to the overlay.
- Kept the existing `Loading...` placeholder while the refresh is in flight.
- Added a core serialization test for the new event.

## Mac / Windows Parity

- macOS has the drawer that needed the active refresh behavior.
- The shared daemon/core protocol now understands `session_list_requested`, so Windows can emit the same event if/when it gains a session drawer.
- Current Windows UI still uses a simple session prompt rather than this drawer; Windows overlay syntax check passed and no UI fork was required.

## Verification

Passed:

- `cargo fmt --manifest-path crates/cue-core/Cargo.toml`
- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test -p cue-core session_list_event_serializes --lib`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `cargo test -p cue-daemon overlay_history_cards_replay_saved_conversation --lib`
- `cargo build -p cue-daemon --bin bluey-daemon`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `git diff --check`
- `cargo build -p cue-cli --bin bluey`
- `cargo test -p cue-core --lib`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

## Current State

- Local visible QA mode was restarted from the rebuilt debug binary:
  - daemon pid `75048`
  - overlay visible `true`
  - overlay capture excluded `false`
  - screen capture active `false`
- Opening History should now actively request local sessions instead of waiting on a stale/missed startup push.

## Remaining QA / Gates

- Live-test by opening History in the running overlay. It should leave `Loading...` quickly and show sessions or the empty-state message.
- Before release/upload, return from visible QA mode to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
