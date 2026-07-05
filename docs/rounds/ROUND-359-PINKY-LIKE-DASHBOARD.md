# Round 359 - Pinky-Like Dashboard

## Trigger

Owner asked for the Bluey dashboard to use the same simple structure as Pinky: a compact top account banner, clearer balance and Auto Reload state, computers below, Session History for previous chats, and Summary for balance and usage details.

## Root Cause/Fix

The dashboard still led with three large KPI cards, so the page felt oversized and less beginner-friendly than Pinky's plan banner plus tabs.

This round reshaped the web dashboard only:

- Replaced the large top KPI grid with a compact `Balance remaining` overview.
- Moved Auto Reload into the top overview so users can immediately see whether automatic credits are on.
- Added dashboard tabs for `Computers`, `Session History`, `Summary`, and `Billing`.
- Renamed the linked-device section to `My Computers` and clarified that it shows computers and browsers linked by Bluey code or desktop sign-in.
- Renamed saved sessions to `Session History` and positioned it as previous Bluey chats and saved work sessions.
- Added compact Summary cards for credits balance, paid answers, and tier, plus the existing usage breakdown.
- Kept Billing focused on manual credit reload.
- Removed a duplicate `accountEmailLabel` id by adding a dashboard-specific account label id.
- Bumped static web cache keys to `2026070416`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Route-aware local preview on `127.0.0.1:4180`.
- In-app browser DOM checks:
  - `/account#billing` activates the Billing tab.
  - Dashboard overview exists.
  - Auto Reload is inside the top overview.
  - Computers, Session History, Summary, and Billing panels exist.
  - Summary mirrors exist for balance, paid answers, and tier.
  - No duplicate ids are present.
  - `/account#summary` activates the Summary tab and hides Billing.
- Static live deploy:
  - `rsync -av --delete ... web/ root@165.227.77.152:/var/www/bluey/`
  - excluded `/install.sh`, `/install.ps1`, `/latest.json`, `/latest.json.sig`, `/releases/***`, and `/backups/***`.
- Live checks:
  - `https://bluey.sh/account` serves `bluey-site.css?v=2026070416` and `bluey-site.js?v=2026070416`.
  - Live account HTML contains the overview, dashboard tabs, Summary mirrors, and dashboard-specific account label.
  - Live CSS contains the Round 359 dashboard layout block.
  - Live JS contains the dashboard tab controller and Summary mirror updates.
  - Live account HTML has no duplicate ids.
  - `https://bluey.sh/install.sh` and `https://bluey.sh/latest.json` still return the protected installer/release responses.

## Current State

- Only static web UI files and this round doc changed.
- No native overlay, audio, desktop runtime, or backend runtime files were changed.

## Remaining QA/Gates

- Signed-in visual QA on live `bluey.sh/account` is still needed because local browser testing cannot use a real authenticated Bluey account.
