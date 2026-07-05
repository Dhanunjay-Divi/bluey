# Round 392 - Add Credits Checkout Launch

## Goal

Make the Bluey Add credits button reliably open the payment checkout and explain real checkout failures.

## Findings

- The old flow waited for `/billing/checkout` before calling `window.open`, so browser popup blockers could stop checkout because the open was no longer tied directly to the click.
- The error copy always appended "Billing may not be fully configured yet," even when the server returned a specific account restriction such as internal/test/admin billing being blocked.

## Changes

- Open a lightweight checkout placeholder tab immediately from the Add credits click.
- Navigate that placeholder to the Square/Stripe checkout URL when `/billing/checkout` returns.
- Disable the Add credits buttons while checkout is being created.
- Close the placeholder tab and show the exact server error when checkout cannot be created.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/assets/bluey-site.js docs/rounds/ROUND-392-ADD-CREDITS-CHECKOUT-LAUNCH.md`
- `awk '/[ \t]$/{print FILENAME ":" FNR ": trailing whitespace"; bad=1} END{exit bad}' docs/rounds/ROUND-392-ADD-CREDITS-CHECKOUT-LAUNCH.md`
