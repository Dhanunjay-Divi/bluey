# Round 367 - Admin Trial Protection Tab

Date: 2026-07-05
Branch: codex/bluey-web-ui-parallel-20260704
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Goal

Remove trial-abuse telemetry from the normal paid/account Summary experience while keeping the operational view available for admin accounts.

## Changes

- Moved the trial protection panel into a dedicated `Admin` dashboard tab.
- Hid the `Admin` tab unless `/account/me` returns `is_admin`.
- Redirected non-admin `#admin` navigation back to the normal Computers tab.
- Renamed the visible panel from `Trial abuse` to `Trial protection` and clarified that it is admin-only.
- Updated the static bundle cache key.

## Files

- `web/index.html`
- `web/assets/bluey-site.js`

## Verification

- Passed `node --check web/assets/bluey-site.js`.
- Passed `git diff --check`.
- Deployed static web assets to `bluey.sh`.
- Confirmed `https://bluey.sh/account` serves cache key `2026070424`.
- Confirmed the live account HTML includes the hidden Admin tab and `Trial protection` copy instead of the customer-facing `Trial abuse` heading.
- Confirmed deployed JavaScript contains admin-tab availability gating.
- Confirmed protected installer/signature artifacts still return 200 after deploy.
