# Round 219 - History, Canvas, and STT Clarity

Date: 2026-06-27 02:54 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked four follow-up questions from live QA:

- why the History drawer did not show saved recordings
- what the two answer-card icons meant
- why an LRU cache request created/confused canvas behavior but did not reliably show code or a clear canvas opener
- whether stopping and clearing transcript still deducts price

## Root Cause

History:

- The current Bluey data directory existed at `~/Library/Application Support/bluey`, but local recordings from earlier builds still lived under the legacy `~/Library/Application Support/cue` directory.
- The History drawer asked the daemon for local meetings, but `MeetingStore` only read the current Bluey active/archive paths.
- Result: the app looked empty even though legacy local recordings existed on disk.

Answer-card icons:

- The first icon was copy.
- The second icon pasted the answer into the app behind Bluey, but the old `arrow.down.doc` symbol looked like a download/file action.
- Cards with a canvas artifact had no direct per-card canvas opener beside the text, so reopening the right canvas was not obvious.

LRU/code behavior:

- The answer-shape prompt told Bluey to avoid replacing code for follow-ups, but repeated build requests could be interpreted as "this already exists above."
- That caused a repeated "build me LRU cache" request to sometimes answer with "already in the session" instead of regenerating/showing the implementation.

STT pricing clarity:

- Stopping and clearing captions are two different operations:
  - stop settles the active cloud STT reservation against actual elapsed transcribed time and refunds unused reserved trial/credit time
  - clear removes local caption text from the next answer context
- Clearing text cannot undo provider work that already happened for cloud transcription, so the UI needed clearer wording.

## Fix

Local history:

- `MeetingStore` now detects the legacy sibling `cue` data directory when the current directory is `bluey`.
- `all_meetings`, `last_meeting`, `load_by_id`, `rename`, and `delete` now read both current Bluey local history and legacy Cue local history.
- Results are sorted newest-first and deduped by meeting id.
- New writes still stay in the current Bluey store; this is a read/manage compatibility bridge, not a rollback to the old path.

Answer cards:

- Added a dedicated canvas opener button on macOS answer cards that have an artifact.
- The button opens the canvas assigned to that specific answer card and logs a metadata-only lifecycle event.
- The paste-into-behind-app button now uses a text-cursor style icon when available, with the existing paste behavior preserved.
- Copy, canvas, and paste buttons share the same compact action layout.

LRU/code behavior:

- Backend prompt rules now explicitly say repeated build/implement/write requests should show or regenerate the implementation.
- Code mode and General mode have the same rule, so "build me LRU cache" should not be answered with only "already above" unless the user explicitly asks whether it already exists.

STT clarity:

- The macOS transcript-clear tooltip now says clearing captions affects the next answer context and that already transcribed cloud audio may still count as used.
- Existing stop/settle behavior remains unchanged: cloud STT charges settled elapsed provider work and refunds unused reservation; local/offline transcription has no cloud STT charge.

## Mac / Windows Parity

- History compatibility and answer-shape prompt changes are shared backend behavior for Mac and Windows.
- The new answer-card canvas opener is macOS-specific because this branch has the rich macOS card/canvas pane there.
- Windows does not currently have the same native rich per-card canvas UI in this branch, so Windows received the shared backend behavior and a native syntax check.

## Verification

Passed:

- `git diff --check`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test -p cue-daemon meeting_store_reads_legacy_cue_history_from_bluey_store --lib`
- `cargo test -p cue-daemon storage::security_tests::meeting_store --lib`
- `cargo test -p cue-daemon mode_instructions --lib`
- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape --lib`
- `cargo test -p cue-daemon answer_overlay_artifact --lib`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
- `cargo build -p cue-daemon --bin bluey-daemon`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

Latest local visible status after reinstall:

- daemon pid `2855`
- overlay visible `true`
- overlay capture excluded `false`
- screen capture active `false`

## Current State

- Local visible QA mode is running from the repo debug build.
- History should now include legacy local Cue recordings as well as current Bluey recordings.
- Answer cards now expose copy, open-canvas when available, and paste-into-behind-app as distinct actions.
- Repeated implementation requests should regenerate/show code instead of deflecting to prior context.

## Remaining QA / Gates

- Test the History drawer manually with old local sessions visible.
- Test one code answer that creates a canvas and confirm the new per-card canvas opener opens the matching canvas.
- Before release/upload, return to normal capture-excluded mode and confirm `overlay_capture_excluded: true`.
- The STT pricing explanation is now clearer in the UI, but a full billing receipt line for "transcribed seconds settled/refunded" would be a separate product/billing round.
