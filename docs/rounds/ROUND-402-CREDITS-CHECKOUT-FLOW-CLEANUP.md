# Round 402 - Credits Checkout Flow Cleanup

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Trigger

The Billing tab and Add Credits dialog read like internal billing settings: manual reload, payment method, and Auto Reload looked disconnected and confusing.

## Changes

- Reframed Billing as `Credits` with one account-wallet flow and Auto Reload as an optional backup.
- Reworked the Add Credits dialog into a clear two-step flow:
  - choose a credit amount
  - keep Auto Reload on and add/use a saved card, or turn it off for one-time checkout
- Added quick amount buttons for `$15`, `$30`, and `$50`.
- Moved the Square checkout button into a single footer action with clearer payment-confirmation copy.
- Made Auto Reload behavior stricter in the dialog: if Auto Reload is on and no card is saved, Bluey asks for a card in the same dialog before continuing.
- Bumped static web assets to `2026070704`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Live static deploy checks:
  - `https://bluey.sh/` references `bluey-site.css?v=2026070704`
  - `https://bluey.sh/` references `bluey-site.js?v=2026070704`
  - live HTML contains the `Credit amount`, `Auto Reload card`, and `Continue to Square checkout` copy
  - live JS contains the stricter Auto Reload card prompt and preset wiring
  - live CSS contains the `reload-amount-presets`, `reload-setup-footer`, and `billing-credits-grid` styles

## Current State

The credits flow should now feel like a wallet checkout instead of two unrelated billing cards.

## Remaining QA/Gates

- Smoke the real Square checkout/card path with a signed-in account.
