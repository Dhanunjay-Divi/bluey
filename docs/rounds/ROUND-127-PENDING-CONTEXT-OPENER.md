# Round 127 - Pending Context Opener

## Problem

Attached documents and images were still rendered in the lower chip row after a question was sent. That made saved session context look like it would be sent again on the next answer, and long chip lists were hard to scroll.

## Change

- Split saved context from pending context in the macOS overlay.
- The lower strip now shows only files that are pending for the next send.
- After Answer is sent, pending chips move into the sent question card and the lower strip clears.
- The top file badge remains as the saved context opener. Click it to show or hide all saved files for the session.
- The lower file tray now forces a scrollable document width so longer lists can be scrolled horizontally.
- Windows now clears pending context chips after Send as well, so it does not keep stale queued files below the composer.
- The overlay now sends explicit `visible_context_ids` with Answer. The daemon still uses saved files as model context, but only those pending IDs are shown as attached to the sent question.
- Existing saved files sent during overlay startup/session hydration update the top opener only. They no longer become pending chips just because Bluey restarted.
- Sent question cards compact large attachment sets to the first few chips plus a `+N more` chip, with the hidden file names in the tooltip.

## Manual Check

1. Attach several documents and images.
2. Confirm the lower row shows them before send.
3. Press Answer.
4. Confirm the sent question bubble shows the attached chips.
5. Confirm the lower row clears.
6. Click the top file badge, for example `5 files ready`, and confirm saved files can be opened in the horizontal tray.
7. Ask a follow-up without newly attaching files and confirm saved documents are not repeated in the sent question bubble.
