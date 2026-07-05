# Round 386 - Balance Reload Save Flow

## Goal

Make the signed-in dashboard balance card simple: show remaining balance, keep Add credits visible there, and make Auto Reload a deliberate saved setting.

## Changes

- Renamed the signed-in balance heading to "Remaining balance".
- Removed the desktop-account mismatch helper copy from the balance card.
- Made the top-card Add credits button visible again.
- Kept Auto Reload threshold and reload amount fields visible while Auto Reload is off.
- Added a Save button for Auto Reload.
- Changed Auto Reload toggle and amount field edits to draft-only changes.
- Saved Auto Reload settings only when the user clicks Save.
- Stopped saved-card setup from automatically enabling Auto Reload.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.js web/assets/bluey-site.css`
