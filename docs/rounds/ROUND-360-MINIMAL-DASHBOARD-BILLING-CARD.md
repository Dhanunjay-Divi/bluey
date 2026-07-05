# Round 360 - Minimal Dashboard Billing Card

## Trigger

Owner showed the signed-in dashboard still felt too large and busy. The top account area included too much copy, usage detail, a full Auto Reload card, and the Square card form. Owner asked for a minimal balance-first dashboard, small Auto Reload `when below` and `add` fields, and a Billing option to change the saved card like Pinky.

## Root Cause/Fix

Round 359 improved structure but still left payment setup in the top overview. The authenticated hero also added a large `Bluey account` intro before the dashboard.

This round keeps the work web-only:

- Hid the authenticated dashboard hero so the dashboard starts sooner.
- Simplified the top overview to `Balance`, balance hint, amount, and a small Auto Reload rule.
- Removed paid-answer/tier summary details from the top overview.
- Removed top-level `Add credits` and the Square card setup from the top overview.
- Kept Auto Reload controls small: `When below`, `Add`, toggle, and one short rule line.
- Moved the desktop-code hint into the Computers tab so it no longer blocks the dashboard before the tabs.
- Added a Billing `Payment method` card with `Save card` / `Change card`, following Pinky's billing placement.
- Changed the Square card form to open only after the Billing card action is clicked.
- Bumped static web cache keys to `2026070417`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Static duplicate-id scan for `web/index.html`.
- Route-aware local preview on `127.0.0.1:4181`.
- In-app browser DOM checks:
  - `/account#billing` activates the Billing tab.
  - Top overview title is `Balance`.
  - Top overview has no usage meta.
  - Top overview has no `Add credits` button.
  - Top overview has no Square card form.
  - Billing panel contains `changeSquareCardButton`, `billingProviderLabel`, and `squareCardSetup`.
  - Desktop-code hint is inside the Computers panel after the tabs.
  - Authenticated hero hide rule exists.
  - No duplicate ids are present.
- Static live deploy:
  - `rsync -av --delete ... web/ root@165.227.77.152:/var/www/bluey/`
  - excluded `/install.sh`, `/install.ps1`, `/latest.json`, `/latest.json.sig`, `/releases/***`, and `/backups/***`.
- Live checks:
  - `https://bluey.sh/account` serves `bluey-site.css?v=2026070417` and `bluey-site.js?v=2026070417`.
  - Live account HTML has `Balance`, no dashboard overview meta, Billing card controls, and the desktop-code hint inside Computers.
  - Live CSS contains the Round 360 tightening block.
  - Live JS contains the Billing `Change card` action.
  - Live account HTML has no duplicate ids.
  - `https://bluey.sh/install.sh` and `https://bluey.sh/latest.json` still return the protected installer/release responses.

## Current State

- Only static web UI files and this round doc changed.
- No native overlay, audio, desktop runtime, or backend runtime files were changed.

## Remaining QA/Gates

- Signed-in visual QA on live `bluey.sh/account`, especially Billing card flow with a real Square-enabled account.
