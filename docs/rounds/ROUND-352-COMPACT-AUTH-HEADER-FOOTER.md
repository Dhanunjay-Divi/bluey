# Round 352 - Compact Auth Header and Footer

## Trigger

The owner said the Bluey auth UI felt too large and asked to restore the header and footer that were removed from the guest login page.

## Root Cause/Fix

- Restored the guest account header on `/login` and related auth states.
- Moved the sun/moon theme toggle back into the account header instead of using a standalone floating pill.
- Reduced the guest auth card scale for a more premium feel:
  - narrower card,
  - smaller logo and wordmark,
  - shorter inputs and primary button,
  - tighter form, link, and footer-note spacing,
  - smaller auth helper text and message panels.
- Kept the footer visible in the first desktop viewport and preserved the Pinky-like footer copy.
- Tightened the mobile auth header so Bluey logo, theme, Download, and Login fit on one row.
- Bumped static web cache keys to `2026070409`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Local route-aware preview server on `127.0.0.1:4177`.
- In-app browser smoke:
  - desktop dark login shows the restored fixed header, compact card, and footer in the viewport,
  - desktop light login keeps the theme knob inside the header toggle and uses readable contrast,
  - mobile `390x844` login has no horizontal overflow and shows header, compact card, and footer.
- Static live deploy:
  - `rsync -av --delete ... web/ root@165.227.77.152:/var/www/bluey/`
  - excluded `/install.sh`, `/install.ps1`, `/latest.json`, `/latest.json.sig`, `/releases/***`, and `/backups/***`.
- Live checks:
  - `https://bluey.sh/login` serves `bluey-site.css?v=2026070409` and `bluey-site.js?v=2026070409`,
  - live login visual smoke shows the restored account header, hidden guest hero, compact card, and visible footer,
  - `https://bluey.sh/install.sh` and `https://bluey.sh/latest.json` still return the protected installer/release responses.

## Current State

- Only static web UI files and this round doc changed.
- Static web is live on `bluey.sh`.
- No native overlay, audio, desktop runtime, or backend runtime files were changed.
- The unrelated untracked review handoff file remains untouched.

## Remaining QA/Gates

- Owner visual QA on the live auth page.
