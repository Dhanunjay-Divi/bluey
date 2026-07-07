# Round 409 - Billing Tab Pinky Style

Date: 2026-07-07

Scope:
- Keep the work limited to Bluey web UI files.
- Make Billing read more like Pinky's compact billing area while preserving Bluey's prepaid wallet model.

Changes:
- Renamed the Billing section copy around prepaid credits, card updates, and Auto Reload cancellation.
- Replaced subscription language with Bluey-specific wallet language.
- Added a compact Billing status table for credits, Auto Reload, and saved card state.
- Added an explicit `Cancel Auto Reload` action that appears only when Auto Reload is on.
- Changed saved-card action copy from `Change card` to `Update card`.
- Removed the repeated `$X adds $X credits` helper line from the Billing card.
- Added dark and light theme styles for the new Billing status table.

Validation:
- `node --check web/assets/bluey-site.js`

