# Round 148 - Pill Run Expands And History States

## Why

The small Bluey pill could start or stop Listen without opening the expanded panel. That made it hard to see captions, history, and recording state immediately after clicking play or stop.

The history control was also icon-only, so it was easy to miss even though sessions are saved locally and sent to the overlay.

## Changed

- The pill play and stop action now opens the compact expanded Bluey window before changing Listen state.
- The top bar history button shows a readable `History` label when there is enough width.
- The full-size tooltip now describes the compact expanded and fully expanded states more clearly.

## Current Window States

- Pill: smallest parked control.
- Compact expanded: normal working surface with chat, captions, files, and history.
- Fully expanded: larger work surface for canvas-heavy answers.

## Storage Note

Local recordings are still saved under Bluey's app data and the daemon continues refreshing session rows into the overlay. RAG indexing remains local-first and is rebuilt from saved content when sessions are opened or changed.
