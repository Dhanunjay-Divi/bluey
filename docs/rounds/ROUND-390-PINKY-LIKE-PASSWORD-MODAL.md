# Round 390 - Pinky-Like Password Modal

## Goal

Make the Bluey account change-password modal feel like Pinky's compact account modal while keeping Bluey colors and existing password-change behavior.

## Changes

- Removed the visible account-email explainer from the change-password dialog so the modal focuses only on the task.
- Kept accessible label text for the three password fields while showing Pinky-style placeholder-only inputs.
- Hid the large close control for this modal and left the existing Cancel action as the visible escape path.
- Restyled the password modal to a compact dark card with Bluey focus rings, left-aligned Update/Cancel actions, and matching light-theme treatment.
- Added mobile sizing so the modal stays compact on narrow screens.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.css docs/rounds/ROUND-390-PINKY-LIKE-PASSWORD-MODAL.md`
- `awk '/[ \t]$/{print FILENAME ":" FNR ": trailing whitespace"; bad=1} END{exit bad}' docs/rounds/ROUND-390-PINKY-LIKE-PASSWORD-MODAL.md`
