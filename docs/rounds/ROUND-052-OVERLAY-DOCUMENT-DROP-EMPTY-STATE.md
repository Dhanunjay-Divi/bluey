# Round 052 - Overlay Document Drop Empty State - 2026-06-17

## Why

The expanded Bluey overlay empty state still presented four internal buckets:
Audio, Files, Screen, and Canvas. That made the first-run state feel like a
feature checklist instead of the actual user action: add useful context and ask.

The user direction for this pass was to remove those blocks, keep Auto as the
answer-routing mental model, and make document attachment work through both the
button and drag-and-drop.

## What Changed

- Replaced the empty-state subtitle with plain user-facing copy:
  "Attach documents or drop them here. Auto chooses the fastest accurate answer."
- Removed the `Audio / Files / Screen / Canvas` chips.
- Added a compact document drop target that names supported context types:
  PDF, DOCX, TXT, MD, and code.
- Registered the expanded overlay as an AppKit file drop destination.
- On Finder file drop, Bluey now:
  - highlights the feed boundary,
  - shows a small in-window drop hint,
  - sets the docs badge to indexing,
  - displays a temporary "Indexing dropped document(s)..." chip,
  - emits `attach_files_requested` with the dropped file paths.
- The existing `+` attach button behavior is preserved and still asks the
  daemon to open the native file picker.

## Verification

- `swift build -c release --package-path native/macos/cue-overlay`
- `git diff --check`

## Notes

This pass only changes native macOS overlay UI and IPC emission. It does not
change daemon-side document parsing, indexing, or storage behavior.

