# Round 423 - Session History Waiting State

## Goal
- Make Session History less misleading when Bluey has only uploaded the session record.
- Stop showing an active "Open tab" action for rows with no transcript, answers, or context.
- Keep the account dashboard polished in dark and light themes.

## Changes
- Updated Session History copy to say saved desktop chats appear after transcript or answer upload.
- Collapsed record-only sessions into one empty-state message instead of listing unreadable rows.
- Kept "Open" only for sessions that have uploaded transcript, answer, or context content.
- Tightened row layout, action spacing, link styling, and light-theme behavior.

## Files
- `web/index.html`
- `web/assets/bluey-site.js`
- `web/assets/bluey-site.css`
