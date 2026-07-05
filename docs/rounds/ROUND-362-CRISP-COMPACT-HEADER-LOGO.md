# Round 362 - Crisp Compact Header Logo

## Trigger

The owner showed the Bluey signed-in header beside Pinky and said the Bluey version still looked too loose and the logo looked blurry.

## Root Cause/Fix

- Bluey was applying glow twice: the SVG wordmark had an internal blur filter, and the nav CSS added stronger external drop-shadows on top.
- Reduced the Bluey wordmark SVG glow so the letter edges render sharper.
- Reduced logo/icon CSS shadows across product, account, download, policy, and auth surfaces.
- Tightened the signed-in account header:
  - lower fixed-header padding,
  - 88px connect-code field,
  - 32px connect controls,
  - 30px profile button,
  - slightly tighter action gaps.
- Added `2026070419` cache keys for CSS, JS, and logo image URLs so a normal refresh picks up the sharper assets.

## Verification

- `git diff --check`
- `node --check web/assets/bluey-site.js`
- Local browser sanity on `/account` confirmed:
  - `bluey-site.css?v=2026070419`.
  - fixed account header computes to 56px high.
  - theme toggle computes to 48px x 24px.
  - connect-code input computes to 88px x 32px.
  - profile button computes to 30px x 30px.
  - header logo and wordmark use `?v=2026070419`.
  - wordmark CSS shadow is reduced to a 4px Bluey drop-shadow.
- Live `/account` browser sanity confirmed the same 56px header, `2026070419` CSS, cache-busted logo URLs, and no `#trialTurnstile` markup from the unrelated local worktree changes.
- Live `/` and `/account` serve `bluey-site.css?v=2026070419`, `bluey-site.js?v=2026070419`, and `bluey-logo.svg?v=2026070419` / `bluey-wordmark.svg?v=2026070419`.
- Live `bluey-wordmark.svg?v=2026070419` contains the reduced `stdDeviation="1.25"` and `stdDeviation="4"` wordmark glow values.
- Live protected release endpoints still respond:
  - `/install.sh` as `application/x-shellscript`.
  - `/latest.json.sig` as `application/pgp-signature`.

## Current State

The change is limited to web UI assets and this round note. No native overlay, audio, or backend runtime files were touched.

## Remaining QA/Gates

- Owner visual pass on the signed-in live header.
