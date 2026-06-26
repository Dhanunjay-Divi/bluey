# Round 145 - Full Chat Answer With Canvas

Date: 2026-06-23

## Changed

- Removed the chat-side truncation used for answers that also create a canvas.
- The left chat now keeps the full non-code explanation instead of cutting it to a short preview with `...`.
- Code blocks still stay out of the chat body and live in the canvas, keeping the split useful without hiding the spoken answer.

## Verified

- Ran `./native/macos/cue-overlay/build.sh`.
