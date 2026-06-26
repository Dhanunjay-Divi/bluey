# Round 157 - Attachment chip preview UX

## What changed

- Sent question cards now render every attached context item in a horizontal strip instead of hiding extra files behind `+N more`.
- Sent attachment chips are clickable. If the chip has a local path, Bluey opens the screenshot, image, or document through macOS.
- Bottom attachment chips are clickable on the label/icon area and still keep the `x` action for removal.
- The header files badge is no longer selectable text, so clicking Show/Hide files toggles the bottom attachment strip more reliably.

## Expected behavior

- Clicking a Screen context chip opens the captured screenshot file.
- Clicking a document chip opens the document.
- If 10 files are attached to an answer, all 10 appear in the question card strip and can be reached by horizontal scrolling.
- The header file badge toggles between showing all saved context files and hiding the bottom file strip.

## Verification

- `./native/macos/cue-overlay/build.sh`
- Local visible install refreshed in `~/.bluey/bin`.
- Visible Bluey relaunched with `./scripts/bluey-visible-local.sh`.
