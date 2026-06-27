# Round 224 - Privacy-Safe Live QA Diagnostics

Date: 2026-06-27 18:41 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner asked whether Bluey has proper logs for the live issues currently being seen:

- History drawer stuck on `Loading...`
- Code/canvas artifact buttons disappearing after switching chats
- Answers streaming or saving without the expected code/canvas artifact
- Captions/transcripts being sent twice or not clearing after an answer
- Auto-send firing with little or no usable transcript
- Transcript clear behavior and later billing/support questions
- Clicks/actions happening in the overlay without enough support breadcrumbs

## Root Cause

Bluey already had useful answer request/completion diagnostics and some canvas lifecycle logging, but the diagnostics were uneven around the exact user-visible failure points.

The main gaps were:

- session-list refreshes did not log request, count, elapsed time, or send failures
- history replay did not log whether code/canvas artifacts were restored or inferred
- answer persistence did not log the saved answer/artifact shape
- transcript duplicate decisions did not log why a final segment was skipped
- overlay transcript consume/clear and auto-send/manual-send paths had little structured metadata

## Fix

- Added daemon logs for `session_list_requested`, session refresh elapsed time, session counts, context/image totals, active count, and overlay send failures.
- Added history replay diagnostics for rebuilt card count, restored artifacts, inferred artifacts, question cards, answer cards, attachment chips, transcript fallback, and empty-session fallback.
- Added hydration delivery diagnostics for pushed versus failed history cards.
- Added answer persistence diagnostics for saved answer shape, code-fence count, visible context count, attachment count, artifact type, artifact confidence, and artifact body length.
- Added duplicate transcript diagnostics for both direct transcript add and audio/STT transcript paths.
- Added transcript clear diagnostics with cleared segment/action/decision counts.
- Added macOS overlay lifecycle events for:
  - History drawer opened
  - sessions rendered
  - transcript buffer consumed
  - consumed transcript skipped
  - transcript context cleared
  - auto-send sent/skipped
  - manual Ask sent/skipped
- Added Windows parity lifecycle events for transcript clear and manual Ask send.
- Allowed daemon logs to print lifecycle `detail` only for the newly added safe diagnostic stages.

## Privacy / Security

No raw user content is logged.

The added logs intentionally avoid:

- question text
- answer text
- transcript text
- code snippets
- filenames
- URLs
- source titles

The logs use metadata only: ids, counts, lengths, booleans, route/provider labels, elapsed times, source labels, artifact type, and error categories.

## Mac / Windows Parity

- Shared daemon diagnostics apply to both Mac and Windows.
- macOS has the current History drawer, so drawer-open and sessions-rendered lifecycle events were added there.
- Windows does not currently have the same History drawer, but it now emits matching safe lifecycle events for transcript clear and Ask send.
- Windows syntax check passed.

## Verification

Passed:

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo test -p cue-daemon overlay_history_cards_restore_code_artifact_button --lib`
- `cargo test -p cue-daemon overlay_history_cards_replay_saved_conversation --lib`
- `cargo test -p cue-daemon duplicate_transcript_detection --lib`
- `cargo test -p cue-daemon answer_overlay_artifact --lib`
- `cargo build -p cue-daemon --bin bluey-daemon`
- `cargo build -p cue-cli --bin bluey`
- `git diff --check`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug native/macos/cue-overlay/build.sh`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`
- `cargo test -p cue-daemon --lib` (`273 passed`, `2 ignored`)

## Current State

- Logging is now strong enough to diagnose the current screenshots without needing the user's private data.
- This round is diagnostics-only: it does not change the answer model, billing rules, web search behavior, click-through behavior, or STT accuracy directly.
- The next time History, transcript, auto-send, code artifact, or restored-chat behavior fails, the logs should show which handoff point failed.

## Remaining QA / Gates

- Local visible QA overlay was restarted from the rebuilt debug binary:
  - daemon pid `96701`
  - overlay visible `true`
  - overlay capture excluded `false`
  - screen capture active `false`
- Reproduce the History drawer, code artifact restore, Listen/send, and auto-send flows once and inspect logs for the new events.
- Before release/upload, return visible QA mode to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
