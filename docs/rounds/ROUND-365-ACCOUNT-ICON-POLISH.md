# Round 365 - Account Icon Polish

## Trigger

The owner compared Pinky's header icons against Bluey's and said Bluey's icons looked bad, especially the account icon.

## Root Cause/Fix

- Bluey's account button still used a CSS-drawn person glyph, which looked rough beside the polished theme toggle, input, and button.
- Replaced the CSS-drawn account glyph with a clean inline SVG user icon matching Pinky's account-menu shape.
- Removed the pseudo-element account glyph drawing rules and added direct SVG stroke styling.
- Bumped the web cache key to `2026070422`.

## Verification

- `git diff --check`
- `node --check web/assets/bluey-site.js`
- Live `/account` serves `bluey-site.css?v=2026070422` and `bluey-site.js?v=2026070422`.
- Live `/account` contains `<svg class="profile-icon" viewBox="0 0 24 24" ...>`.
- Live CSS contains `#accountApp .profile-icon` with rounded SVG stroke styling.
- Live CSS no longer contains the old `profile-glyph` pseudo-element drawing rules.
- Live protected release endpoints still respond:
  - `/install.sh` as `application/x-shellscript`.
  - `/latest.json.sig` as `application/pgp-signature`.

## Current State

The change is limited to web UI icon markup/CSS, the web cache key, and this round note. No native overlay, audio, or backend runtime files were touched.

## Remaining QA/Gates

- Owner visual pass on the live signed-in header icon.
