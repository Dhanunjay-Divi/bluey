# Round 363 - Logo Optical Alignment

## Trigger

The owner showed the Bluey header logo pair and noted the icon and wordmark were not aligned properly.

## Root Cause/Fix

- The image boxes were centered, but the Bluey wordmark's visible letter mass sits slightly high inside its SVG.
- Added a small optical `translateY(2px)` nudge to header/nav wordmarks across product, account, download, policy, and rail brand surfaces.
- Bumped the web cache key to `2026070420`.

## Verification

- `git diff --check`
- `node --check web/assets/bluey-site.js`
- Local browser sanity on `/account` confirmed:
  - `bluey-site.css?v=2026070420`
  - `bluey-site.js?v=2026070420`
  - brand row still uses `align-items: center`
  - account header wordmark computes to `matrix(1, 0, 0, 1, 0, 2)`
- Live `/account` serves `bluey-site.css?v=2026070420` and `bluey-site.js?v=2026070420`.
- Live CSS contains the two `transform: translateY(2px)` brand wordmark rules.
- Live browser sanity confirmed the account header wordmark computes to `matrix(1, 0, 0, 1, 0, 2)` while the logo icon remains untransformed.
- Live protected release endpoints still respond:
  - `/install.sh` as `application/x-shellscript`.
  - `/latest.json.sig` as `application/pgp-signature`.

## Current State

The change is limited to web UI CSS, the web cache key, and this round note. No native overlay, audio, or backend runtime files were touched.

## Remaining QA/Gates

- Owner visual pass on the live header alignment.
