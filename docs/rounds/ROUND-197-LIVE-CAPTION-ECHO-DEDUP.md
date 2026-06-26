# Round 197 - Live Caption Echo Dedup

## Trigger

Owner reported that captions appear doubled for whatever they speak.

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`
Workspace: `/Users/uno/Downloads/cue`
Branch: `codex/bluey-overlay-routing-hardening`
Round completed: 2026-06-26 15:31 EDT

## Root Cause

- Bluey can listen to microphone and system audio together.
- The daemon's duplicate transcript check only treated exact recent repeats from the same speaker/source as duplicates.
- If the same utterance arrived once as `Mic` and once as `System`, both final captions could be saved, indexed, and reused as answer context.
- The macOS overlay live-caption strip also kept separate preview buffers per source, so an identical Mic/System echo could visually replace or repeat the same words.

## Implemented

- Daemon:
  - Added same-speaker duplicate window constant: 8 seconds.
  - Added Mic/System echo duplicate window constant: 2.5 seconds.
  - Treats exact final-caption repeats from Mic/System within the echo window as one caption.
  - Keeps older cross-source repeats, so a real later response is not incorrectly dropped.
- macOS overlay:
  - Suppresses identical cross-source live-caption preview echoes.
  - If `Mic` and `System` have the same caption body, `Mic` wins for the preview and pending answer context.
  - Removes stale duplicate System preview/autosend buffers when Mic wins.
- Windows:
  - No Windows overlay-specific patch was needed because the Windows overlay keeps one global transcript final/partial buffer, not separate per-source preview buffers.
  - The daemon dedup fix applies to Windows too.

## Files Touched

- `crates/cue-daemon/src/app.rs`
- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `docs/rounds/ROUND-197-LIVE-CAPTION-ECHO-DEDUP.md`
- `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md`

## Verification

Passed:

- `cargo test -p cue-daemon duplicate_transcript_detection -- --nocapture`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo build --release -p cue-cli -p cue-daemon`
- `native/macos/cue-overlay/build.sh`

Installed locally:

- `target/release/bluey` to `~/.bluey/bin/bluey`
- `target/release/bluey-daemon` to `~/.bluey/bin/bluey-daemon`
- `native/macos/cue-overlay/.build/bluey-overlay-macos` to `~/.bluey/bin/bluey-overlay-macos`
- `native/macos/cue-overlay/.build/cue-overlay-macos` to `~/.bluey/bin/cue-overlay-macos`
- `native/macos/cue-overlay/.build/BlueyOverlay.app` to `~/.bluey/bin/BlueyOverlay.app`

Local status after install:

- `bluey on` succeeded.
- Daemon pid: `44710`
- Overlay visible: `true`
- Overlay capture excluded: `true`
- Active meeting: `e5645fad-8644-4b7c-80e5-26587b488bb3`

## Current State

- Speaking while Mic + System are both active should no longer create two saved final captions for the same utterance when both sources hear the same words nearly simultaneously.
- The macOS live-caption strip should suppress the common duplicate System echo when Mic already has the same caption.
- Answer context should prefer the Mic copy if both sources produce the same caption.

## Remaining QA Gates

- Live audio smoke:
  - Start Listen with Mic + System.
  - Speak a short sentence.
  - Confirm the bottom caption strip does not show the same sentence twice.
  - Press Answer and confirm the question/context card does not contain duplicate caption lines.
- Test remote-meeting audio separately:
  - Let only the remote/system speaker talk.
  - Confirm Bluey still keeps System captions when there is no matching Mic echo.
