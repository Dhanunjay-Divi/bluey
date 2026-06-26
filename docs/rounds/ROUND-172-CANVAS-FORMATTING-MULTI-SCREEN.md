# Round 172 - Canvas, Formatting, And Multi-Screen Attachments - 2026-06-25

## Goal

Fix the overlay behavior where a coding canvas stayed open for the next unrelated question, answers with inline recommendation lists rendered as one cramped paragraph, and repeated Screen captures only sent the latest pending image.

## Changes

- `crates/cue-daemon/src/app.rs`
  - Added an overlay answer formatting pass that turns obvious inline recommendation bullets into separate lines.
  - Keeps provider status lines and em dashes out of visible answers.
  - Added regression tests for inline recommendation formatting and multiple sent screen attachments.

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - Closes the canvas when a completed answer has no workbench artifact, so unrelated follow-up questions do not keep the old Q1 coding pane open.
  - Stops the overlay from inventing canvases from plain answer text when the daemon did not send an artifact.
  - Keeps multiple fresh screen captures pending for the next Answer instead of replacing older pending screen captures.

## Verification

- `cargo fmt --check -p cue-daemon`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
- `cargo test -p cue-daemon sanitize_answer_text_splits_inline_recommendation_lists --lib`
- `cargo test -p cue-daemon overlay_question_cards_keep_multiple_screen_attachments --lib`
- `cargo test -p cue-daemon answer_overlay_artifact_does_not_canvas_screen_chat --lib`
- `cargo build --release -p cue-cli -p cue-daemon`
- `native/macos/cue-overlay/build.sh`

## Local Install

Copied the rebuilt `bluey`, `bluey-daemon`, and macOS overlay binaries into `~/.bluey/bin`, then restarted local visible mode with `scripts/bluey-visible-local.sh`.
