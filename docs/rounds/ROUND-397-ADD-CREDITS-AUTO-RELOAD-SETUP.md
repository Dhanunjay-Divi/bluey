# Round 397 - Add Credits Auto Reload Setup

Date: 2026-07-06
Branch: `codex/bluey-web-ui-parallel-20260704`

## Goal

Make the account balance card and Add Credits flow feel clear for first-time credit setup: remaining balance stays simple, Add Credits opens a focused setup popup, and Auto Reload setup lives in that popup instead of a confusing disabled dashboard state.

## Changes

- Kept the dashboard balance card neutral at `$0.00` and changed its primary action to a simple `Add credits`.
- Removed the dashboard copy that said `Save a card in Billing before turning on`.
- Hid dashboard Auto Reload save actions when no saved payment method exists.
- Made the dashboard Auto Reload toggle open the Add Credits setup when a card is still needed.
- Defaulted Auto Reload on inside the Add Credits popup for eligible first-time setup.
- Kept one-time Square checkout unblocked, while saving Auto Reload settings before checkout when a saved card already exists.
- Bumped the web asset query version to `2026070602`.

## Verification

- `node --check web/assets/bluey-site.js`
- Local browser harness with a zero-balance Square-enabled mock account:
  - Dashboard showed neutral `$0.00`, `Add credits`, and no disabled Auto Reload save button.
  - Add Credits opened the popup with `$15`, Auto Reload checked on, card setup inside the popup, and `Continue to $15.00 checkout`.
