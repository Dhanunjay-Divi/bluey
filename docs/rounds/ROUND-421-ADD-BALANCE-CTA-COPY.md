# Round 421 - Add Balance CTA Copy

## Trigger

The Add balance modal button could read `Add $15.00 + Auto Reload` when Auto Reload was selected, making the primary action feel cluttered.

## Root Cause/Fix

- The checkout button label appended `+ Auto Reload` whenever the modal Auto Reload toggle was on.
- The button action is still adding balance, while Auto Reload setup is already visible in the panel beside it.

Fix:

- Keep the button label focused on the balance action: `Add $15.00`.
- Preserve the existing Auto Reload behavior and setup controls.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`

## Current State

Only web UI copy and round documentation changed. Native overlay, audio, and backend runtime files were not touched.
