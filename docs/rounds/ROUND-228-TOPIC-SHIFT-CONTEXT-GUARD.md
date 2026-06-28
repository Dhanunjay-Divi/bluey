# Round 228 - Topic Shift Context Guard

Date: 2026-06-28 02:38 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner showed a live QA case where the user asked for Fibonacci code, followed up about reducing time complexity, and then asked a new question: "Can you explain LRO/LRU cache?" Bluey answered the new cache question as if it were still part of the Fibonacci context.

## Root Cause

The daemon added the last 10 Bluey Q&A turns to every answer request. That helps true follow-ups like "can we reduce time complexity for this?", but it also over-anchors standalone new-topic questions to stale session history.

The provider prompt also said to prefer recent relevant turns, but did not explicitly say that a standalone new topic should be treated as fresh.

## Fix

- `answer_context_from_meeting` now receives the latest question and conditionally includes recent Bluey Q&A.
- Recent Q&A is kept for explicit follow-ups such as "this", "that", "the code", "the answer", "previous", "what about", or similar references.
- Recent Q&A is skipped for standalone new-topic requests such as "explain LRU cache", "write Fibonacci", or other named-topic asks that do not explicitly refer back.
- Topic matching ignores filler and ASR noise such as "so", "okay", "six", "numbers", and normalizes common LRU transcription noise from `lro` to `lru`.
- The Human-speak contract now tells the model to answer standalone new topics directly and not connect them to prior context unless the user asks to compare, continue, modify, or use the previous answer.
- Added privacy-safe debug lines when recent Q&A is skipped for a likely new-topic question.

## Mac / Windows Parity

- This is shared daemon/backend context selection behavior.
- It applies equally to macOS and Windows overlays because both send answer requests through the same daemon path.
- No native overlay code changed in this round.

## Verification

Passed:

- `cargo fmt --manifest-path crates/cue-daemon/Cargo.toml`
- `cargo test -p cue-daemon meeting_context_ --lib`
- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape --lib`
- `cargo test -p cue-daemon follow_up_context_ --lib`
- `cargo test -p cue-daemon --lib` (`275 passed`, `2 ignored`)
- `cargo build -p cue-daemon --bin bluey-daemon`
- `BLUEY_BIN=/Users/uno/Downloads/cue/target/debug/bluey scripts/bluey-visible-local.sh`
- `/Users/uno/Downloads/cue/target/debug/bluey status`

## Current State

- Local visible QA overlay was restarted from the rebuilt debug daemon:
  - daemon pid `59487`
  - overlay visible `true`
  - overlay capture excluded `false`
  - screen capture active `false`

## Remaining QA / Gates

- Re-test:
  - "Can you write Fibonacci series?"
  - "Is there a way you can reduce time complexity for this?"
  - "Can you explain LRU cache?"
- Expected result: the time-complexity follow-up uses Fibonacci context, while the LRU cache answer does not say "in the Fibonacci context" unless the user explicitly asks for that comparison.
- Before release/upload, return visible QA mode to normal capture-excluded mode and verify `overlay_capture_excluded: true`.
