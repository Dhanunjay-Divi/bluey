# Round 407 - Billing Number Guards

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Goal

Prevent Add Credits and Auto Reload fields from visibly accepting negative values or number formats that do not make sense for dollar amounts.

## Changes

- Added shared billing money-input guards for manual reload, modal reload, dashboard Auto Reload, and modal Auto Reload fields.
- Blocked `-`, `+`, and scientific notation characters (`e`/`E`) at the input level.
- Cleaned pasted values before they enter the field.
- Added blur normalization so values clamp to the allowed range:
  - Manual reload: `$15` to `$500`.
  - Auto Reload threshold: `$1` to `$50`.
  - Auto Reload amount: `$15` to `$500`.
- Added a manual reload `$500` max validation to match the UI field limit.
- Bumped the web asset cache key to `2026070709`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Static web deploy to `bluey.sh` with cache key `2026070709`.
- Live `https://bluey.sh/` references `bluey-site.css?v=2026070709` and `bluey-site.js?v=2026070709`.
- Live JavaScript contains `installBillingMoneyInputGuards`, `cleanDollarInput`, and the manual reload max validation.
- Live HTML no longer contains the stale screenshot strings `Save Auto Reload`, `Continue to Square checkout`, or the old amount confirmation helper.
- Live JavaScript for `v=2026070709` passes `node --check`.
