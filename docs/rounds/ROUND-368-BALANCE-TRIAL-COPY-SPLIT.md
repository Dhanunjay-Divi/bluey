# Round 368 - Balance Trial Copy Split

Date: 2026-07-05
Branch: codex/bluey-web-ui-parallel-20260704
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Make the dashboard balance card explain the account state clearly. Temporary Try Us accounts should show free trial time remaining, while regular accounts should show credit balance and reload controls.

## Changes

- Changed the dashboard account-state logic to treat only `is_temporary` accounts as free trials.
- Regular accounts no longer show leftover trial-minute copy in the balance card or Summary card.
- Added a compact top-level `Add credits` button for regular accounts.
- Temporary accounts show `Free trial` and minutes remaining instead of dollar balance.
- Hid Auto Reload controls on temporary trial accounts.
- Added light/dark styling for the trial-mode balance card.
- Updated the static bundle cache key.

## Files

- `web/index.html`
- `web/assets/bluey-site.js`
- `web/assets/bluey-site.css`

## Verification

- Passed `node --check web/assets/bluey-site.js`.
- Passed `git diff --check`.
- Deployed static web assets to `bluey.sh`.
- Confirmed `https://bluey.sh/account` serves cache key `2026070425`.
- Confirmed deployed JavaScript contains the `is_temporary` trial/balance split and top reload button wiring.
- Confirmed deployed CSS contains trial-mode balance styling.
- Confirmed protected installer/signature artifacts still return 200 after deploy.
