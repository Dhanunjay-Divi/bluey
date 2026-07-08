# Round 422 - Billing Card Update Modal

Date: 2026-07-08
Branch: `codex/bluey-web-ui-parallel-20260704`

## Goal

Make the Billing tab stop showing a duplicate inline card editor, and make `Update card` actually open a focused card-update flow.

## Changes

- Removed the embedded card form from the Billing payment card.
- Added a compact `Update card` modal for the saved Auto Reload card.
- Rewired the Billing `Save card` / `Update card` button to open the modal and save through the Square card setup path.
- Kept `Cancel Auto Reload` as the billing management action, while removing the extra inline `Cancel` from the old embedded form.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
