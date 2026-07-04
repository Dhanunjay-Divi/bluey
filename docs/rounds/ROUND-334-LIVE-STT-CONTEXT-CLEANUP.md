# Round 334 - Live STT Context Cleanup

Date: 2026-07-04
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-overlay-spacing-20260626

## Trigger

The owner reported that saying `given a set of two numbers` appeared in the live caption strip as `given acetone two numbers`. This is especially bad because users are paying for transcription and expect coding/interview phrases to be repaired the way mature transcription apps clean up final transcript text.

## Finding

- Bluey was showing and consuming mostly raw realtime STT text.
- Deepgram can bias vocabulary with `keyterm` hints, but realtime STT can still choose a plausible English word like `acetone` when the intended phrase is `a set of`.
- Most polished transcription apps separate two layers:
  - fast partial captions that may be imperfect,
  - finalized/corrected transcript text after endpointing or reprocessing.
- Bluey had speed/keyterm improvements from Round 333, but it did not yet have a deterministic local cleanup pass for obvious coding/interview mishears before the text entered the overlay, session transcript, and answer pipeline.

## Changes

### Desktop Daemon

- Added `clean_live_stt_text` before live STT segments are shown, saved, or used for answers.
- Repairs high-confidence coding/interview mishears without an extra model call:
  - `given acetone two numbers` -> `given a set of two numbers`
  - `acetone numbers` -> `a set of numbers`
  - `lro cache` / `lru cash` -> `LRU cache`
  - `leet code` / `leak code` / `lead code` -> `LeetCode`
  - `fibinacci` / `fibbonacci` -> `Fibonacci`
  - `memo is asian` -> `memoization`
- Cleanup is phrase-boundary aware so ordinary references such as `acetone bottle` are left unchanged.
- Added metadata-only logs when cleanup happens:
  - source
  - final/partial flag
  - raw/cleaned char counts
  - raw/cleaned word counts
- The logs intentionally do not include transcript text.

### Server STT Relay

- Expanded built-in Deepgram keyterms with common coding/interview phrases:
  - `LeetCode`
  - `Fibonacci`
  - `Two Sum`
  - `given a set`
  - `set of two numbers`
  - `array of integers`
  - `single digit`
  - `double digit`
  - `hash map`
  - `linked list`
  - `binary search`
  - `sliding window`
  - `monotonic stack`
  - `dynamic programming`
- Raised the managed keyterm cap from `50` to `64`, leaving room for configured deployment keyterms after the built-ins.

## Why This Helps

- The live caption strip should no longer show the exact `given acetone two numbers` failure from the screenshot.
- The answer pipeline receives cleaner question text, so the model is less likely to answer the wrong problem.
- The fix is deterministic and free: it does not add another LLM call or extra user charge.
- It gives us a narrow safety net while we separately improve true realtime caption UI and session-specific vocabulary.

## What This Does Not Finish

- This is not a full second-pass transcript model. It is a targeted cleanup layer for common coding/interview errors.
- True best-in-class transcription still needs:
  - dynamic per-session keyterms from attached docs/screen/question context,
  - confidence-aware final transcript repair,
  - a horizontal partial-caption UI that updates smoothly as speech arrives,
  - deterministic audio replay tests that measure first-partial, first-final, and word accuracy.

## Verification

Passed:

```bash
cargo fmt -p cue-daemon
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml stt::tests -- --nocapture
cargo test -p cue-daemon live_stt_cleanup -- --nocapture
cargo test -p cue-daemon live_stt_ -- --nocapture
```

## Deployment

Pending packaging/deploy verification for `0.1.74`.
