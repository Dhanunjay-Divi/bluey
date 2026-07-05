# Round 364 - Connect Label Header

## Trigger

The owner compared the Bluey account header against Pinky and asked for the Bluey code action to say `Connect`.

## Root Cause/Fix

- The landing-page connect form already used `Connect`, but the signed-in account header still used `Join`.
- Changed the signed-in account header code form aria label from `Join a Bluey desktop code` to `Connect a Bluey desktop code`.
- Changed the signed-in account header button text from `Join` to `Connect`.
- Bumped the web cache key to `2026070421`.

## Verification

- `git diff --check`
- `node --check web/assets/bluey-site.js`
- Live `/account` serves `bluey-site.css?v=2026070421` and `bluey-site.js?v=2026070421`.
- Live signed-in account header form uses `aria-label="Connect a Bluey desktop code"`.
- Live signed-in account header button text is `Connect`.
- Live landing connect form still says `Connect`.
- Live protected release endpoints still respond:
  - `/install.sh` as `application/x-shellscript`.
  - `/latest.json.sig` as `application/pgp-signature`.

## Current State

The change is limited to web UI copy/cache key and this round note. No native overlay, audio, or backend runtime files were touched.

## Remaining QA/Gates

- Owner visual pass on the live signed-in header.
