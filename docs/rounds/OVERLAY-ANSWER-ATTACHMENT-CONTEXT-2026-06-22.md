# Overlay Answer Attachment Context - 2026-06-22

## Goal

Make attached documents and captured screen context feel like message attachments when the user presses Enter or Answer.

## Behavior

- Attached documents remain visible as compact chips before the answer is sent.
- Captured screen context is kept out of the persistent document chip strip, but is still attached to the next answer.
- The sent Question card now includes an `Attached to this answer` section listing document and screen context names.
- If the composer is empty, Answer can still run when the user has attached documents or captured a screen:
  - docs only: answer using attached documents and current session context
  - screen only: analyse the attached screen capture
  - screen plus docs: answer using screen, documents, and current session context

## Provider Payload

No provider-side shortcut was added. The daemon still builds the answer request from the active meeting context:

- document previews are included as bounded text context
- screenshots/images are promoted to a vision route and sent as image parts when allowed
- live captions and recent answer history remain part of session context

## Verification

Focused daemon tests cover the visible Question-card attachment summary. The macOS overlay build verifies the new fallback path compiles.
