# Round 131 - Square Checkout Bluey Branding

Date: 2026-06-22

## Issue

Production Square hosted checkout showed Bluey credits but rendered the old
Pinky logo at the top of the payment page. The Square production location was
also still named `Pinky`, and checkout branding used `FRAMED_LOGO` with a pink
button color.

Follow-up finding: Bluey and Pinky had different Square application IDs, but
both production env files pointed at the same Square location ID,
`LRV41T44TT30E`. Square hosted checkout branding is location-scoped, so changing
that location for Bluey also changed Pinky's checkout.

## Live Fix Applied

Using the production Square API credentials from the droplet env:

- Restored the shared `LRV41T44TT30E` location to Pinky:
  - `name=Pinky`
  - `business_name=Pinky`
  - `website_url=https://pinky.sh`
  - `header_type=FRAMED_LOGO`
  - `button_color=#ff4fa3`
  - `button_shape=ROUNDED`
- Created a dedicated Bluey Square production location:
  - `SQUARE_PRODUCTION_LOCATION_ID=L72F51P6X3PSQ`
  - `name=Bluey`
  - `business_name=Bluey`
  - `website_url=https://bluey.sh`
- Set Bluey online-checkout branding on the Bluey-only location:
  - `header_type=BUSINESS_NAME`
  - `button_color=#20c7ff`
  - `button_shape=ROUNDED`
- Updated `/etc/bluey-api/bluey-api.env` on the droplet so Bluey uses
  `L72F51P6X3PSQ` instead of the Pinky location.

This separates the checkout pages. Bluey now has its own Square hosted checkout
location, while Pinky's checkout remains on Pinky's location and branding.

## Guardrails

- Added `scripts/bluey-square-branding.sh` to apply or verify Square checkout
  branding from the same env file used by the server.
- Wired `scripts/bluey-cloud-preflight.sh` to fail when Square checkout branding
  drifts away from Bluey.
- The Bluey branding script now refuses to run unless the active Square
  application ID is the Bluey production app ID,
  `sq0idp-uumlvxMyu_PWr54YIEHf-w`.
- The Bluey branding script now refuses the known Pinky production location ID
  by default, so a future deploy cannot silently rebrand Pinky's checkout.

## Verification

```bash
bash -n scripts/bluey-square-branding.sh
bash -n scripts/bluey-cloud-preflight.sh
ssh root@165.227.77.152 'bash /tmp/bluey-square-branding.sh /etc/bluey-api/bluey-api.env --check'
curl -fsS https://bluey.sh/health
```

The live checks returned:

```text
ok: Square checkout branding verified: name=Bluey, header=BUSINESS_NAME, button=#20c7ff/ROUNDED
env=production
app=sq0idp-uumlvxMyu_PWr54YIEHf-w
loc=L72F51P6X3PSQ
provider=square
Pinky location: name=Pinky, header=FRAMED_LOGO, button=#ff4fa3/ROUNDED
Bluey location: name=Bluey, header=BUSINESS_NAME, button=#20c7ff/ROUNDED
{"status":"ok","version":"0.1.5","commit":"unknown","platform":"linux-x86_64","server_time_ms":1782187815603}
```
