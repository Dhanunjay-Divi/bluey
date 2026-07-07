# Round 406 - Auto Reload Submit Flow

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Goal

Make the Add Credits modal feel like one checkout flow: the user should not need a separate `Save Auto Reload` click before continuing.

## Changes

- Removed the standalone `Save Auto Reload` button from the Add Credits modal.
- Kept the dashboard Auto Reload save action for settings changes made outside checkout.
- Updated modal copy so Auto Reload says it saves when the user continues.
- Preserved the existing submit-time behavior: `Continue to checkout` saves Auto Reload settings before opening checkout when a saved card is available.
- Removed the unused modal-only save handler and fixed the invalid-amount branch to write errors to the Auto Reload helper instead of referencing removed copy.
- Bumped the web asset cache key to `2026070708`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Static web deploy to `bluey.sh` with cache key `2026070708`.
- Live `https://bluey.sh/` references `bluey-site.css?v=2026070708` and `bluey-site.js?v=2026070708`.
- Live HTML contains `Auto Reload saves when you continue.`
- Live HTML no longer contains `reloadSetupSaveAutoButton` or `Save Auto Reload`.
- Live JavaScript for `v=2026070708` passes `node --check`.
