# Canvas Workbench UX Pass - 2026-06-21

## Context

User feedback: Bluey canvas was duplicating long answers from the chat pane, clipping text horizontally, and opening for generic long responses in a way that made the answer feel cut in half. Desired behavior:

- Coding: concise explanation stays in chat; code, patches, or changed blocks live in canvas.
- Coding follow-ups: prefer inline edits/diffs instead of replacing a whole solution.
- System design: recommendation/explanation stays in chat; architecture, data flow, APIs, storage, scaling, and failure modes live in canvas.
- Canvas must wrap readable text instead of cutting off long lines.

## Changes

- `crates/cue-daemon/src/app.rs`
  - Overlay answer streams now preserve explicit managed artifacts from Bluey server streams.
  - Added managed artifact mapping into `CueCardArtifact`.
  - Final overlay card updates prefer explicit artifacts before heuristic inference.
  - Strengthened the managed prompt contract around chat-vs-canvas split and inline follow-up edits.
  - Added regression coverage for managed diff artifacts mapping to the code canvas.

- `crates/cue-daemon/src/llm/answer.rs`
  - Mirrored the canvas split rule in the local answer prompt contract.

- `native/macos/cue-overlay/Sources/cue-overlay/main.swift`
  - Chat pane now summarizes artifact-backed answers instead of duplicating the full canvas body.
  - Code/system/screen/document artifacts get user-facing suffixes like `Code is in the canvas.`
  - Canvas text wraps to the pane width and no longer exposes horizontal scrolling by default.
  - Canvas width is larger in full-size mode and compact mode.
  - Generic structured artifacts no longer auto-open the canvas; code, system design, and screen analysis still auto-open.

## Verification

- `cargo fmt --all --check`
- `swift build -c debug --package-path native/macos/cue-overlay`
- `cargo test -p cue-daemon --all-targets`
- `cargo test -p cue-daemon app::tests::answer_overlay_artifact`
- `cargo test -p cue-daemon app::tests::llm_overlay_artifact_preserves_managed_code_canvas`

## Notes / Risks

- This pass fixes rendering and artifact plumbing, not a full multi-document canvas editor.
- Follow-up inline edit quality still depends on provider prompt compliance and server artifact generation.
- Session persistence currently stores conversation turns, but the live overlay artifact is the main path for immediate canvas rendering.
