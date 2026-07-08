# Round 423 - Session History Waiting State

## Goal
- Make Session History less misleading when Bluey has only uploaded the session record.
- Stop showing an active "Open tab" action for rows with no transcript, answers, or context.
- Keep the account dashboard polished in dark and light themes.

## Changes
- Updated Session History copy to say saved desktop chats appear after transcript or answer upload.
- Added a waiting banner when every listed session is still record-only.
- Changed empty session rows from active "Open tab" links to disabled "Waiting" buttons.
- Kept "Open" only for sessions that have uploaded transcript, answer, or context content.
- Tightened row layout, action spacing, link styling, and light-theme waiting states.

## Files
- `web/index.html`
- `web/assets/bluey-site.js`
- `web/assets/bluey-site.css`
