# Round 408 - Dashboard Auto Reload Autosave

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Goal

Make the dashboard balance and Auto Reload card feel like a simple one-step control, not a form with a separate save action.

## Changes

- Removed the dashboard `Save` button from the compact Auto Reload card.
- Made the dashboard Auto Reload toggle save immediately when changed.
- Made dashboard Auto Reload amount/threshold fields save automatically on blur when a saved payment method or active Auto Reload setting exists.
- Simplified Auto Reload copy:
  - On: reload rule is shown directly.
  - Off: says Auto Reload is off.
  - No card: prompts the user to add credits once to set up Auto Reload.
- Simplified balance copy:
  - `$0.00`: `Add credits to start.`
  - Less than `$1`: `Almost out. Add credits to keep Bluey ready.`
  - Low balance under `$5`: `Low balance. Add credits soon.`
  - Otherwise: `Ready for paid cloud work.`
- Bumped the web asset cache key to `2026070710`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Static web deploy to `bluey.sh` with cache key `2026070710`.
- Live `https://bluey.sh/` references `bluey-site.css?v=2026070710` and `bluey-site.js?v=2026070710`.
- Live HTML contains `Updates automatically.`
- Live HTML no longer contains `saveAutoReloadButton`, `Click Save`, or `Turn on and click Save`.
- Live JavaScript contains the dashboard autosave function and the new zero/low-balance hints.
- Live JavaScript for `v=2026070710` passes `node --check`.
