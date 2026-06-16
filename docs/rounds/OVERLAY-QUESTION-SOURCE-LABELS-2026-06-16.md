# Overlay Question Source Labels - 2026-06-16

## What Changed

- Submitted asks now show as `Question` instead of `You`.
- Bluey answers continue to show as `Bluey`.
- Transcript source labels in submitted question text are highlighted inline:
  - `System: ...`
  - `Mic: ...`
- The live caption ticker is source-first and dynamic:
  - `System · ...`
  - `Mic · ...`
  rather than repeating `Transcribing · System/Mic · ...`.

## Why

The overlay should read like a small AI chat surface, not a debug transcript log. `System` and `Mic` are context sources, while the moment the user presses Answer the combined context becomes a submitted `Question`.

## Verification

- `BLUEY_OVERLAY_SWIFT_CONFIGURATION=debug bash native/macos/cue-overlay/build.sh`
- `swift build -c release --package-path native/macos/cue-overlay`
- `cargo test -p cue-daemon overlay_`
- `git diff --check`

## Reviewer Notes

- This is a presentation-only change for transcript labels and question card titles.
- The payload sent to the LLM still includes the source-labeled transcript lines so the model can distinguish system audio from microphone speech.
