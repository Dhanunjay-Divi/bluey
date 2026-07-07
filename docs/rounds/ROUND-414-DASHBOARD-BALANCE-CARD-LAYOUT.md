# ROUND-414-DASHBOARD-BALANCE-CARD-LAYOUT

Date: 2026-07-07
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Issue

The account dashboard balance card could render with overlapping text and controls. In the reported screenshot, `Remaining balance`, `Auto Reload`, `$15.00`, the add-balance button, and the internal/test-account warning visually collided.

## Root Cause

The dashboard balance card had several later CSS overrides. The final winning rule used a compact flex/grid mix that could collapse badly in blocked-billing states, and the Auto Reload side panel was visible in initial markup before account state hid it.

Internal/admin/test accounts also still exposed paid reload entry points long enough for the UI to show paid controls before explaining that paid checkout was unavailable.

## Changes

- Hid the Auto Reload side panel in initial HTML until account state explicitly enables it.
- Added `paidBillingBlocked()` handling for temporary/admin/restricted accounts.
- Hid and disabled Add Balance and Auto Reload controls when paid billing is blocked.
- Added clear balance hint copy for internal/test accounts.
- Blocked reload modal and checkout actions for paid-billing-blocked accounts.
- Reworked the final dashboard overview CSS so the balance and Auto Reload regions use stable desktop columns, stack cleanly on small screens, and never overlap hidden/visible panels.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Local browser fixture rendered normal paid-account and internal/test-account card states using the real site CSS.
- Fixture layout measurements showed no `title/value` or `main/side` overlaps.

## Deploy Notes

Deploy the web assets after this round so `https://bluey.sh/account` uses the corrected balance card layout.
