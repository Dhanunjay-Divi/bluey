# ROUND-397-DESKTOP-REVOKE-LIVE-BALANCE

Date: 2026-07-05
Branch: `codex/bluey-fast-answer-latency-20260705`
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Goal

Fix the confusing state where a desktop/account was removed from the web UI, but the local Bluey overlay/dashboard could still look signed in or continue showing stale credits. Also reduce balance drift between the overlay and the account UI.

## What Changed

- Kept the server-side removal model intact: web device removal already revokes the linked device and its refresh tokens.
- Added a local daemon device-link verification path so actions and balance refreshes check `/account/devices/status` when a persisted desktop device id exists.
- Centralized local signed-out handling in the daemon:
  - clear Listen account verification cache
  - stop balance polling
  - clear the balance watch
  - send overlay `signed_in=false`
  - show `Sign in` instead of a stale balance
- Cleared the Listen verification cache when the balance watch publishes signed-out state, so a warm Listen cache cannot survive web-side device removal.
- Updated Listen/live audio preflight to verify the linked device before capture starts.
- Updated manual/overlay balance refresh so auth revocation is treated as `SignedOut`, not just “balance unavailable.”
- Lowered daemon balance polling default from 30 seconds to 10 seconds.
- Updated the native dashboard commands so revoked tokens clear local account state and return signed out.
- Added the same linked-device check to native dashboard `account_me` and `get_balance_snapshot`.
- Lowered native dashboard balance indicator polling from 30 seconds to 10 seconds.
- Added quiet 10-second account refresh in dashboard Home and Settings cards.
- Added web account-page live balance polling every 10 seconds while the account page is open.
- Changed web checkout copy so it says balance refreshes automatically rather than making manual refresh the normal path.
- Updated `docs/HOW-IT-WORKS.md` to reflect 10-second balance polling.

## Expected Behavior

- If a user removes a desktop from the web UI, that desktop should sign itself out locally on the next balance poll or the next protected action.
- Listen should not start when the linked desktop has been removed, even if the old access token has not expired yet.
- Overlay balance should update within about 10 seconds from passive polling, and still update immediately after paid actions that trigger an explicit refresh.
- Web and native account dashboards should converge to the same balance without requiring a manual refresh button.

## Verification Plan

- Rust format/check for touched Rust crates.
- Run daemon/dashboard targeted tests.
- Run UI build or type check if available.
- Manual smoke:
  - sign in locally
  - remove desktop from web UI
  - wait up to 10 seconds or press Listen
  - confirm overlay changes to `Sign in`
  - confirm native dashboard account card signs out
  - confirm web account balance updates without manual refresh

## Notes

This does not remove the manual `Refresh balance` button; it remains a fallback. The normal user path should be automatic.
