# Round 130 - Sent Attachment Chips

## Change
- Added structured attachments to `CueCard` so question cards can carry the exact files and screenshots used for the answer.
- Mapped answer context documents and screen captures into sent-card attachments in the daemon.
- Rendered sent attachments as compact chips in the macOS overlay question bubble.
- Added Windows overlay parsing and chip rendering for the same sent attachment field.
- Consumed one-shot screen capture chips from the pending context strip after a question is sent, so the same screenshot does not remain below the chat after it is already attached to the sent turn.

## Product Intent
When a user clicks Screen, attaches files, then presses Answer or Enter, the chat should show those items on the sent question, similar to ChatGPT. The model already receives the context; this change makes the UI tell the same truth.

## Notes
- The daemon still includes a text fallback for older overlay clients.
- Screen and screenshot attachments are treated as image-style chips.
- User-attached documents remain in the pending strip after send because they are often reused for follow-up questions.
- Transcript context remains hidden from the attachment strip because it is live session context, not a user-attached file.
