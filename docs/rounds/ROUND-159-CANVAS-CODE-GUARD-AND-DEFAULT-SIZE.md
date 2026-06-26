# Round 159 - Canvas Code Guard And Default Size - 2026-06-24

## Why

The canvas could accept a weak provider artifact labeled as `code` even when the body was only prose, such as `This should return:`. That could replace the useful previous code block with an empty-looking workbench. Canvas also opened with a narrow fixed pane, so the default split felt cramped.

## Change

- Require code canvases to contain real implementation, SQL, patch, diff, or recognizable code syntax.
- Reject prose-only managed code artifacts before they reach the overlay.
- Add the same guard in the macOS overlay so old local history cannot reopen bad code canvases.
- Make the normal canvas pane responsive instead of fixed at 360 px.
- Expand the default window wider when canvas opens.

## Intended UX

- Previous real code stays visible unless a follow-up actually changes code.
- Weak prose answers stay in chat and do not replace the canvas.
- Canvas opens as a readable workbench by default, with full-window canvas still available when needed.

## Verification

- `cargo test -p cue-daemon llm_overlay_artifact --lib`
- `cargo test -p cue-daemon answer_overlay_artifact --lib`
- `swiftc -parse native/macos/cue-overlay/Sources/cue-overlay/main.swift`
