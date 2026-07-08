# Round 419 - Balance Card Collapse Fix

## Trigger

The account dashboard balance card could visually collapse, overlapping `Remaining balance`, the dollar amount, Auto Reload fields, and the Add balance button.

## Root Cause/Fix

- Later dashboard CSS let the balance row flex and wrap inside a two-column card without enough reserved space for the Auto Reload panel.
- Admin/internal/test accounts could still render paid checkout controls even though the server marks those flows unavailable.

Fixes:

- Added a final balance-card grid override so the main balance content and Auto Reload panel reserve their own columns.
- Added a mobile stack override so the balance amount aligns cleanly on small screens.
- Hide Add balance and Auto Reload controls for temporary, admin, billing-restricted, or internal/test accounts.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/assets/bluey-site.js web/assets/bluey-site.css web/index.html`

## Current State

Only web UI files changed. Native overlay, audio, and backend runtime files were not touched.

## Remaining QA/Gates

- Live account check should confirm the restricted/internal account no longer shows overlapping paid controls.
- Live normal account check should confirm the balance card still shows Add balance and Auto Reload without overlap.
