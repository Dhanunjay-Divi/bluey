# Round 031 - Web Simplification Pass — 2026-06-04

## Goal

Make `bluey.sh` feel closer to Pinky's simple product entry: clear brand,
simple login/account flow, direct install path, and less explanatory product
copy on the public homepage.

## Changed

- Simplified the public homepage navigation to `Account`, `Privacy`, and
  `Install`.
- Reworked hero copy around one clear path: run `bluey on`, sign in only when
  needed, and use credits without a default monthly subscription.
- Replaced the heavier product-preview copy with a compact account/desktop
  status preview.
- Reduced the homepage highlight strip to three simple actions: Account,
  Install, and Context.
- Simplified account-page copy and tightened the dashboard/account layout so it
  reads like a control surface instead of an admin console.

## Verification

- Static diff reviewed.
- `git diff --check` clean.
- Browser visual verification was attempted, but the in-app browser rejected
  both localhost and local-file preview URLs for this session. A real browser
  visual pass is still recommended after deployment.
