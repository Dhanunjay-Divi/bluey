# Round 133 - Streaming Code Canvas And Status Cleanup

Date: 2026-06-22

## What Changed

- Removed provider thinking text from the answer body. The daemon no longer writes `Thinking with ...` into the streamed answer card.
- Added answer sanitizing for leaked provider status lines before display or save.
- Made answer cards start with an empty body while the card status shows streaming state.
- Sent inferred canvas artifacts during streaming instead of only after the final answer event.
- Let the macOS overlay open and update the canvas while an answer is still streaming.
- Treated unfinished fenced code blocks as code so the canvas can open before the closing fence arrives.
- Strengthened the answer prompt so coding answers keep chat explanation short and put complete code in fenced blocks for the workbench.

## Why

Coding answers should feel like Codex style: the left side explains the idea, the right side holds the complete code or patch, and provider routing/status text should never look like part of the answer.

## Verification

- `swiftc -typecheck native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `cargo fmt --check --package cue-daemon`
- `cargo test -p cue-daemon sanitize_answer_text_removes_provider_status_lines`
- `cargo test -p cue-daemon answer_overlay_artifact_detects_streaming_partial_code`
- `cargo test -p cue-daemon provider_messages_include_overlay_friendly_answer_shape`
- `cargo build -p cue-daemon --release`
- `bash native/macos/cue-overlay/build.sh`

## Local Install

Copied rebuilt binaries into `~/.bluey/bin`:

- `bluey-daemon`
- `cue-daemon`
- `bluey-overlay-macos`
- `cue-overlay-macos`
- `BlueyOverlay.app`
