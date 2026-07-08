# ROUND-440 Answer Typography Skimming

Date: 2026-07-08
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Request

Make Bluey answers easier to skim, closer to ChatGPT-style readable formatting: more breathing room, clearer paragraph spacing, and bold/visible important keywords or section labels.

## Changes

- Added answer-side attributed rendering in the macOS overlay feed.
- Added improved answer line spacing and paragraph spacing so responses do not feel like dense walls of text.
- Added visual emphasis for common answer sections and labels:
  - Approach
  - Code
  - Implementation
  - Explanation
  - Complexity
  - Edge cases
  - Line notes
  - Time Complexity
  - Space Complexity
  - Key idea
  - Takeaway
- Widened answer text layout slightly so long answers do not wrap into an overly narrow column.
- Kept question/transcript rendering separate so user bubbles and live captions keep their tighter layout.

## Verification

- Passed macOS overlay build:
  - `native/macos/cue-overlay/build.sh`

## Deployment

No deploy was performed in this round. Per owner instruction, these changes stay local until an explicit deploy or signed deploy is requested.

## Notes

This is a renderer-side improvement, so it improves readability even when the model returns plain section labels instead of perfect markdown.
