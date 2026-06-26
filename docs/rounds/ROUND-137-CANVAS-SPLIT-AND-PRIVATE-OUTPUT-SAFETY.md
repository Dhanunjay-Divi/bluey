# Round 137 - Canvas Split And Private Output Safety

Date: 2026-06-23

## What Changed

- Tightened the answer contract so system-design chat stays as the short recommendation, assumptions, and key tradeoff.
- Kept full architecture detail in the workbench: architecture, components, data flow, APIs, storage, scaling, tradeoffs, failure modes, observability, and rollout.
- Removed pointer-only chat endings such as "architecture is in the canvas"; chat now has to stand on its own.
- Made system-design canvas auto-open only for structured design answers, not casual explanations that mention architecture, APIs, or storage.
- Added final chat compaction when a system-design canvas exists, so the left side does not duplicate the whole workbench.
- Added a macOS overlay privacy filter so old, streamed, or client-detected private-instruction-looking content is refused and never promoted into canvas.

## Why

The overlay should feel like chat plus a workbench, not like the answer jumps halfway into another pane. The left side is for the speakable explanation and follow-ups; the right side is for durable code, architecture, and deeper reference material.

## Verification

- `cargo test -p cue-daemon answer_overlay_artifact_detects_system_design --lib`
- `cargo test -p cue-daemon answer_overlay_artifact_does_not_canvas_casual_system_design_chat --lib`
- `cargo test -p cue-daemon system_design_artifact_keeps_chat_body_compact --lib`
- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape --lib`
- `cargo test -p cue-daemon internal_disclosure --lib`
- `native/macos/cue-overlay/build.sh`
- `cargo check -p cue-daemon -p cue-cli`
- Local visible Bluey restarted with the rebuilt daemon and overlay.
