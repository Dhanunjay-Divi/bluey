# Round 164 - Files Drawer Conversation Scope - 2026-06-24

## Why

The files control should be obvious: clicking it should show every document, image, and screen context saved on the current conversation. Pending context for the next answer is a separate concept and should stay compact.

## Change

- Renamed the header file toggle copy to `Show files · N` and `Hide files · N`.
- Updated the tooltip to say it shows every file attached to the conversation.
- Kept the bottom attachment strip as the horizontal scroll surface when the full conversation file list is open.

## Intended UX

- The header control is the opener for all conversation files.
- The bottom strip remains compact unless the user asks to show files.
- After Answer, pending one-shot attachments clear, but saved conversation files remain available from the header opener.
