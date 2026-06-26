# Round 162 - Copy Paste Shortcuts - 2026-06-24

## Why

The borderless click-through overlay could miss normal macOS copy/paste shortcuts because command-key equivalents do not always flow through the same keyDown path as typed characters.

## Change

- Routed `Cmd+A`, `Cmd+C`, `Cmd+V`, and `Cmd+X` through the overlay window key-equivalent path.
- Kept paste focused on the Ask anything composer after the composer is clicked.
- Made `Cmd+C` copy the selected composer text when there is a selection.
- Made `Cmd+C` copy selected answer or canvas text before falling back to composer shortcuts.
- Kept `Cmd+A` working inside selected answer/canvas text views so users can select the visible block.
- Made `Cmd+C` copy the latest Bluey answer when Bluey is focused and the composer has no selection.
- Kept the existing message and canvas copy buttons with checkmark feedback.

## Intended UX

- Click Ask anything, then paste normally with `Cmd+V`.
- Select composer text and use `Cmd+C`, `Cmd+X`, or `Cmd+A` normally.
- Select any visible answer or canvas text and use `Cmd+C` normally.
- Press `Cmd+C` with no composer selection to copy the latest Bluey answer.
