# Round 166 - History attachment restore

## What changed

- Conversation turns now persist the context item IDs used for that answer.
- Reopened session history rebuilds structured attachment chips for each saved question.
- Older saved turns that only have the visible `Attached to this answer` text recover chips from the session's saved context when possible.

## Expected behavior

- New answers remember exactly which documents or screen captures were sent with the question.
- When a user reopens a saved recording, the question card can show the same attachment chips again.
- Clicking those chips opens the saved local file path when it still exists.

## Notes

- The actual files are still saved as local session context artifacts. This change links answers back to those saved files.
- If a user deletes the local image/document file from disk, the chip can still show metadata but macOS cannot open the missing file.

## Verification

- `cargo check -p cue-core`
- `cargo check -p cue-daemon`
