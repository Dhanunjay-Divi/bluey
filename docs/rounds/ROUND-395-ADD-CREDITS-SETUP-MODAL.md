# Round 395 - Add Credits Setup Modal

Date: 2026-07-06
Branch: `codex/bluey-web-ui-parallel-20260704`

## Goal

Make Bluey's first credit reload flow feel intentional and beginner-friendly without weakening the payment safety model.

## Changes

- Added an `Add Bluey credits` modal launched from both dashboard balance and Billing `Add credits` buttons.
- Kept manual reload at the Bluey default of `$15`, with copy that credits are added only after Square confirms payment.
- Added Auto Reload setup inside the modal:
  - toggle
  - `When below`
  - `Reload`
  - save Auto Reload button
- Reused the existing Square Web Payments SDK card-save flow inside the modal for saving a card-on-file for future Auto Reload.
- Refactored the Square card helper so the SDK card element can attach in either the Billing tab or the Add Credits modal.
- Added light/dark modal styling that matches the compact Pinky-inspired dashboard proportions.

## Product Note

The modal does not credit the account directly from a client-side card token. One-time credit purchase still opens Square checkout and credits only after processor confirmation/webhook evidence. A true in-modal first payment would need a backend Square payment endpoint, idempotent crediting, and tests.

## Verification

- `node --check web/assets/bluey-site.js`
