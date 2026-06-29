# Round 234 - Live Caption Visible Question Timing

Date: 2026-06-29 18:25 EDT
Branch: `codex/bluey-overlay-spacing-20260626`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Trigger

The owner spoke "Explain LRU cache" through Listen, but the visible Question
card showed the generic live-caption instruction:

`Answer the latest live captions from the current session transcript...`

The answer itself used the transcript and generated an LRU cache response, but
the visible card looked wrong. The owner also reported that streaming felt slow.

## Root Cause

- Round 230 intentionally stopped dumping long raw live transcripts into visible
  Question cards so the overlay would not create huge repeated transcript
  bubbles.
- That safety change was too broad for short spoken asks. When the composer was
  empty, macOS and Windows sent the generic live-caption instruction as the
  visible question even when the spoken transcript was a short clear question.
- The screenshot showed first output around `1.4s`; that is acceptable first
  token latency, but the overall response can still feel slow when Bluey streams
  a longer answer plus a code/canvas artifact. We did not have enough
  privacy-safe timing logs to separate provider delay, stream delay, and overlay
  render delay.

## Fix

- macOS now uses the actual short live-caption text as the visible Question
  card when it is safe:
  - up to `220` characters
  - at most `2` non-empty lines
  - not a placeholder such as "Live captions preview" or "Listening..."
- Long or messy live-caption transcript sends still use the generic live-caption
  intent so giant raw transcript blocks do not flood the chat.
- Auto-send-after-stop uses the same short-question rule, so spoken asks and
  manual Enter sends behave consistently.
- Windows now has parity for empty-input transcript sends: short safe transcript
  text is shown, otherwise it falls back to the generic live-caption instruction.
- Ask lifecycle logs now include whether the visible question was the generic
  live-caption prompt.
- macOS answer-card lifecycle logs now record privacy-safe stream timing:
  - first overlay update latency
  - total answer render duration
  - final body length
  - whether an artifact/canvas was present
- Server streaming route logs now record first provider stream event latency and
  whether the first event was a delta, done, error, or end.

## Privacy / Diagnostics

No raw transcript or answer text is added to logs. The new diagnostics only log
metadata such as character counts, route event kind, timing, artifact presence,
and generic-prompt boolean flags.

## Mac / Windows Parity

- macOS overlay: visible question selection and overlay stream timing logs.
- Windows overlay: visible question selection parity and generic-prompt flag in
  ask lifecycle logs.
- Server: provider stream first-event timing applies to both platforms.

## Verification

Passed:

- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `x86_64-w64-mingw32-gcc -fsyntax-only -municode native/windows/cue-overlay/main.c`
- `cargo fmt --manifest-path server/Cargo.toml`
- `cargo check --manifest-path server/Cargo.toml`
- `native/macos/cue-overlay/build.sh`
- `cargo test --manifest-path server/Cargo.toml first_token_deadline -- --nocapture`

The local visible QA overlay was restarted after the macOS build:

- daemon pid `89067`
- active meeting id `3a84f982-4c7d-4df5-8a3c-0062bec9a8cb`
- `overlay_visible: true`
- `overlay_capture_excluded: false` for visible local QA
- `transcript_segments: 3`

## Current State

Next spoken short asks like "Explain LRU cache" should show that spoken text in
the Question card instead of the generic live-caption instruction. Long
transcript context still stays compact and is supplied to the model through the
existing meeting transcript context path.

## Remaining QA / Gates

- Live-test a short spoken ask such as "Explain LRU cache" and confirm the
  visible Question card uses that short text.
- Live-test a long rambling transcript and confirm Bluey keeps the card compact
  instead of showing a giant raw transcript bubble.
- Inspect lifecycle logs for `answer_stream_first_update`,
  `answer_stream_finished`, and server `first_event_latency_ms` if streaming
  still feels slow.
- Before release/upload, return to normal capture-excluded mode and verify
  `overlay_capture_excluded: true`.
