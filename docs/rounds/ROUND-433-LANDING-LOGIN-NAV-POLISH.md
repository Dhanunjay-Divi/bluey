# ROUND-433 Landing Login Nav Polish

Date: 2026-07-08
Backup thread id: 019e133e-d92a-7830-8df0-3a050a4e22f6
Branch: codex/bluey-web-ui-parallel-20260704

## Goal

Fix the landing nav guest login action so it reads like Pinky's polished `Log in` affordance instead of a generic oversized `Login` button.

## Changes

- Changed guest navigation copy from `Login` to `Log in` across landing, account, download, and policy shells.
- Tightened the landing header CTA height, radius, font weight, border, and hover treatment.
- Bumped the site CSS asset version in `web/index.html` so the nav polish is not hidden by stale CSS.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check -- web/index.html web/assets/bluey-site.css docs/rounds/ROUND-433-LANDING-LOGIN-NAV-POLISH.md`
