# Round 143 - Dashboard Checkout Cleanup

## Why

The account dashboard made reloads feel confusing because the reload route could auto-open checkout while the Add credits button also opened a new tab. The manual reload amount also showed `$15`, even though the product CTA should default to a `$30` reload while keeping `$15` as the minimum.

The Auto Reload card also showed a blank Square card input with vague payment-method copy, and the access list said web/browser even though this surface is really for host login activity.

## Changed

- Manual Add credits now requests a `$30` checkout.
- The landing Add credits card points to `/reload` without auto-starting checkout.
- `/reload?checkout=1` no longer auto-starts checkout on page load.
- Checkout opens in a new tab only. If the browser blocks the popup, the dashboard stays put and tells the user to allow popups.
- Stripe Checkout sessions now pass Bluey-specific branding settings with the hosted Bluey checkout wordmark PNG.
- Auto Reload card copy now reads as a setup step for saving a card.
- Host access wording replaces linked device/browser wording on the dashboard.

## Verified

- `node --check web/assets/bluey-site.js`
- `cargo test api::billing --lib` from `server/`
