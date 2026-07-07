# Round 404 - Checkout Copy Simplification

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Trigger

The Add Credits action said `Continue to Square checkout`, which exposed the payment processor instead of speaking like a product flow.

## Changes

- Changed the Add Credits primary action to `Continue to checkout`.
- Changed transient checkout messages from `Square checkout` to plain `checkout`.
- Changed the Billing provider pill from `Square checkout` to `Secure checkout`.
- Bumped static web assets to `2026070706`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Live static deploy checks:
  - `https://bluey.sh/` references `bluey-site.css?v=2026070706`
  - `https://bluey.sh/` references `bluey-site.js?v=2026070706`
  - live HTML contains `Continue to checkout`
  - live JS contains `Preparing checkout`, `Opening checkout`, and `Secure checkout`

## Current State

The credits flow now keeps processor details out of the main customer-facing action.

## Remaining QA/Gates

- Smoke the real checkout link from a signed-in account.
