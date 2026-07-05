# Round 361 - Pinky-Like Header Tightening

## Trigger

The owner compared the signed-in Bluey header with Pinky's dashboard header and said the Bluey version still looked ugly and oversized. The requested direction was to match Pinky's elements and proportions while keeping Bluey's colors.

## Root Cause/Fix

- The signed-in account header had the right elements, but the connect-code input, Join button, profile icon, and dropdown used brighter Bluey borders and heavier visual weight than Pinky's reference.
- Tightened the signed-in header controls to Pinky's sizing rhythm:
  - 90px monospace code field.
  - compact Join button with a softer Bluey outline.
  - neutral gray profile icon by default, Bluey only on hover/open.
  - smaller, darker account dropdown matching Pinky's 260px menu shape.
- Matched the theme toggle proportions more closely to Pinky's compact pill while keeping Bluey gradients and light/dark behavior.
- Bumped the web cache key to `2026070418`.

## Verification

- `git diff --check`
- `node --check web/assets/bluey-site.js`
- Local browser sanity on `/account` confirmed cache `2026070418` and signed-in control CSS:
  - 90px code field.
  - 34px Join button.
  - 32px neutral profile button.
  - 260px account dropdown.
- Live `/account` serves `bluey-site.css?v=2026070418` and `bluey-site.js?v=2026070418`.
- Live CSS contains the tightened header values for the theme toggle, connect-code field, profile icon, and dropdown.
- Live protected release endpoints still respond:
  - `/install.sh` as `application/x-shellscript`.
  - `/latest.json.sig` as `application/pgp-signature`.

## Current State

The change is limited to web UI assets and this round note. No native overlay, audio, or backend runtime files were touched.

## Remaining QA/Gates

- Owner visual pass on the signed-in live header.
