# Round 139 - Checkout New Tab And Logo

Date: 2026-06-23

## Request

Keep Bluey checkout separate from the account page by opening checkout in a new
tab, and prefer the Bluey logo in the Square checkout header instead of plain
`Bluey` text.

## Changes

- Updated `web/assets/bluey-site.js` so `startReload()` opens the checkout URL
  with `window.open(..., "_blank", "noopener,noreferrer")`.
- Kept a same-tab fallback when a browser blocks the popup.
- Deployed the updated static JS to `/var/www/bluey/assets/bluey-site.js` on
  the droplet.
- Added a guard to `scripts/bluey-square-branding.sh`: if
  `BLUEY_SQUARE_HEADER_TYPE=FRAMED_LOGO` is requested, the script now verifies
  that the Square location has `logo_url` or `full_format_logo_url` before
  applying or passing the check.

## Logo Status

The new Bluey Square production location `L72F51P6X3PSQ` does not currently
return a `logo_url` or `full_format_logo_url`.

Square documents those location logo fields as read-only and says the logo is
configured in Seller Dashboard under the Receipts section. So the safe path is:

- Upload the Bluey logo to the Bluey Square location in Square Dashboard.
- Re-run:

```bash
BLUEY_SQUARE_HEADER_TYPE=FRAMED_LOGO \
  scripts/bluey-square-branding.sh /etc/bluey-api/bluey-api.env
```

Until the logo exists on the Square location, Bluey checkout should stay on
`BUSINESS_NAME` so customers do not see a broken or empty header.

## Verification

```bash
curl -fsS https://bluey.sh/assets/bluey-site.js | rg -n "window.open\\(checkout.checkout_url|noopener,noreferrer"
bash -n scripts/bluey-square-branding.sh
```

Live static asset contains:

```text
const opened = window.open(checkout.checkout_url, '_blank', 'noopener,noreferrer');
```
