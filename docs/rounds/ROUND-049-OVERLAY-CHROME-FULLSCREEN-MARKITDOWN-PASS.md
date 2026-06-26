# Round 049 - Overlay Chrome, Fullscreen, and Ingestion Pass

Date: 2026-06-17
Branch: codex/bluey-ai-site

## Why

The expanded overlay could place session/status content behind the top Bluey header after resize, fullscreen, or canvas toggles. The opacity slider also felt inert in pass-through mode because the hit-test surface was too narrow. The fullscreen action was not truly fullscreen; the generic frame clamp kept shrinking it back to an inset tool window.

## What Changed

- Added a full-visible-frame clamp path for fullscreen/canvas-fullscreen windows.
- Reset fullscreen fill mode when returning to compact overlay geometry.
- Reworked the defensive overlay layout pass so header, workspace, transcript preview, attachment strip, and composer are all placed from one fixed vertical stack.
- Based the defensive layout on the actual NSWindow content rect, not stale view bounds, so restored/resized windows cannot push the header above the visible panel.
- Expanded interactive hit testing to include the header stack, the whole composer bar, and opacity label/value controls so the opacity slider remains clickable in pass-through mode.
- Kept the pill hidden while the expanded window is open so it cannot drift down behind the header after expand/collapse cycles.
- Added a manual button dispatch path for the expanded window so visible controls remain clickable while the overlay transitions between pass-through and interactive modes.

## Expected UX

- Cards and session banners stay below the Bluey header.
- The `Docs empty` badge no longer has content bleeding behind it.
- Opacity controls are draggable/clickable when the overlay is visible.
- Fullscreen fills the visible display frame instead of becoming a merely larger centered panel.

## MarkItDown Note

Microsoft MarkItDown is a strong candidate for Bluey document ingestion because it converts many source formats into Markdown, which is easier for LLM context, chunking, and RAG. It should be evaluated for a background ingestion worker, not bundled into the foreground overlay path. The worker should:

- Convert supported docs to Markdown.
- Store the original file metadata and generated Markdown separately.
- Chunk Markdown structurally before embedding.
- Run with least privilege and clear file-size/time limits, because conversion tools read user-provided files with process permissions.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
- `git diff --check`
- Visible debug smoke with `BLUEY_DEV_OVERLAY=1 BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1 BLUEY_OVERLAY_FORCE_RAW_HELPER=1` and the debug overlay binary. The first smoke caught a stale-bounds layout issue where the header could move above the visible frame; after the content-rect fix, the second screenshot showed the header visible, content below it, and `bluey overlay opacity 70` reducing the background while keeping text readable.

Note: capture-visible smoke only works with the debug overlay helper because the release helper intentionally ignores `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1`. Release builds remain capture-excluded.

## Follow-Up

- Run the visible overlay smoke on a real desktop and verify fullscreen, opacity drag, and resize/canvas toggles visually.
- If MarkItDown is adopted, add a separate ingestion round with file-type allowlist, sandbox/timeouts, and storage tests.
