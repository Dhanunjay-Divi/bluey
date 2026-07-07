# Round 403 - Add Credits Modal Polish

Date: 2026-07-07
Branch: `codex/bluey-web-ui-parallel-20260704`

## Trigger

The Add Credits modal felt too large and exposed a raw Square error when the card form was not ready. It also allowed `$5` to sit in the custom amount field even though Bluey requires a `$15` minimum reload.

## Changes

- Tightened the Add Credits modal dimensions, panel spacing, close button, footer, and notice styling.
- Stopped auto-opening the Square card form when the dialog opens; Auto Reload can stay on, but the card form appears only when needed.
- Disabled checkout while the credit amount is below the `$15` minimum.
- Normalized reload amount fields back to `$15` on blur when users enter less than the minimum.
- Added a Square-card readiness guard so internal attach/tokenize errors become a human message.
- Bumped static web assets to `2026070705`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Live static deploy checks:
  - `https://bluey.sh/` references `bluey-site.css?v=2026070705`
  - `https://bluey.sh/` references `bluey-site.js?v=2026070705`
  - live HTML contains the shorter Add Credits intro and `Add card` action
  - live JS contains the amount normalization and Square card readiness guard
  - live CSS contains the narrower `680px` modal, non-stretched setup grid, and modal notice styling

## Current State

The credits dialog now behaves like a tighter product checkout: amount first, optional Auto Reload second, and no raw Square internals in the UI.

## Remaining QA/Gates

- Smoke the real Square card-save flow on a signed-in account.
