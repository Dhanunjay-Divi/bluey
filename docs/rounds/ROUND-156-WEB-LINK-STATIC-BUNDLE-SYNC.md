# Round 156 - Web Link Static Bundle Sync

Date: 2026-06-23

## Issue

Login and Download links looked broken after the checkout new-tab change.

The live site first had a mismatched static bundle:

- `/var/www/bluey/index.html` was still the older June 21 file.
- `/var/www/bluey/assets/bluey-site.js` had been copied from the newer local
  build on June 23.

That meant the current JavaScript was looking for newer dashboard/profile and
device DOM nodes that were not present in the live HTML.

Follow-up browser testing found the page was still effectively broken after the
bundle sync because `index.html` carried stale Subresource Integrity hashes for
same-origin CSS/JS. The browser blocked `bluey-site.css`, which left the page
unstyled and moved the Login/Download links into normal document flow.

## Fix

Synced the static web bundle together instead of copying one JS file:

- `web/index.html`
- `web/assets/bluey-site.css`
- `web/assets/bluey-site.js`

The sync preserved release artifacts and installer manifests.

Then:

- Removed same-origin SRI attributes from `web/index.html`.
- Added version query strings to the CSS/JS asset URLs.
- Added `Cache-Control: no-cache` for HTML routes in Caddy so browsers recheck
  the shell page after deploys.

## Verification

```bash
curl -fsSI https://bluey.sh/login
curl -fsSI https://bluey.sh/download
curl -fsS https://bluey.sh/ | rg -n "accountProfile|linkedDevicesList|downloadApp"
curl -fsS https://bluey.sh/assets/bluey-site.js | rg -n "window.open\\(checkout.checkout_url|downloadRoutes|accountRoutes"
curl -fsS https://bluey.sh/health
```

All routes returned 200, the live HTML now contains the current dashboard and
download DOM, the live JS still opens checkout in a new tab, and the API health
check returned `ok`.

Browser verification:

- `/` loads one stylesheet from `bluey-site.css?v=2026062301`.
- Top-nav Login click navigates to `/login`.
- `/login` renders the `Sign in` form and email field.
- Top-nav Download click navigates to `/download`.
- `/download` renders the Download surface and macOS install card.
