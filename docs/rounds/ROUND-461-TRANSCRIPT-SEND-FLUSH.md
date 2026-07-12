# ROUND-461 Transcript Send Flush

Date: 2026-07-09
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Problem

When Listen was active, pressing Enter could submit the transcript text that had arrived so far while the last words were still in-flight from STT. The answer would be sent with a partial phrase, and the late final transcript text could remain in the live captions strip. The next Enter could then accidentally include that leftover text.

## Fix

- Added a short manual-send settle window for transcript-backed sends. If the Ask box is empty and Listen is active, pressing Answer/Enter now waits 450 ms before composing the question.
- The wait cancels any pending auto-send so manual Enter stays in control.
- After a transcript-backed answer is sent, Bluey now remembers the consumed transcript fingerprints for 1.25 seconds and suppresses late matching final/interim transcript updates from any source.
- Clearing transcript context now also cancels any pending manual-send settle work and clears the late-final suppression buffer.

## New Logs

- `manual_answer_waiting_for_transcript_settle`
- `manual_answer_settle_already_pending`
- `transcript_buffer_skip_late_consumed`
- `ask_answer_sent origin=manual|manual_after_transcript_settle`

These logs should make it easier to debug reports where the user pressed Enter before the latest spoken words appeared in the transcript rail.

## Verification

- `git diff --check -- native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh`

## Deployment

No deploy in this round. User asked not to deploy until explicitly requested.
