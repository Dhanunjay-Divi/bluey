# Round 358 - Pinky-Like Account Header

## Trigger

The owner compared Bluey's account header with Pinky's header and asked to make Bluey more like Pinky, remove `Product`, and make the human/profile icon nicer.

## Fix

- Removed `Product` from the account-route header.
- Added a compact signed-in account header connect form:
  - placeholder `Code`,
  - button `Join`,
  - reuses the existing desktop `user_code` connect flow.
- Generalized the landing connect-code form initializer so both landing and account header forms normalize and remember codes.
- For signed-in users, the header `Join` form can approve `/auth/device/approve` directly; if auth is stale, it falls back to `/login?user_code=...`.
- Restyled the account profile button:
  - `38px` -> `34px`,
  - smaller human glyph,
  - lighter border/glow,
  - matching light-theme styling.
- Made the profile menu closer to Pinky:
  - `Change Password`,
  - `Delete Account`,
  - `Logout`,
  - tighter row heights and menu width.
- Hid the compact header code form on small screens to avoid wrapping.
- Bumped static web cache keys to `2026070415`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Local route-aware preview server on `127.0.0.1:4178`.
- In-app browser smoke:
  - guest `/login` header shows `Download` and `Login` only,
  - `Product` is gone from the account header,
  - signed-in-only header code form exists with `Code` and `Join`,
  - profile button CSS resolves to `34px` by `34px`,
  - profile menu order is `Change Password`, `Delete Account`, `Logout`.
- Static live deploy:
  - `rsync -av --delete ... web/ root@165.227.77.152:/var/www/bluey/`
  - excluded `/install.sh`, `/install.ps1`, `/latest.json`, `/latest.json.sig`, `/releases/***`, and `/backups/***`.
- Live checks:
  - `https://bluey.sh/login` serves `bluey-site.css?v=2026070415` and `bluey-site.js?v=2026070415`,
  - live account-header slice has no `Product` link and includes `Download`, `Login`/`Dashboard`, `Code`, `Join`, and the profile menu,
  - live profile menu order is `Change Password`, `Delete Account`, `Logout`,
  - live `bluey-site.css?v=2026070415` contains the compact header connect form and `34px` profile button styles,
  - live `bluey-site.js?v=2026070415` contains the generalized `data-connect-code-form` behavior,
  - `https://bluey.sh/install.sh` and `https://bluey.sh/latest.json` still return the protected installer/release responses.

## Current State

- Only static web UI files and this round doc changed.
- Static web is live on `bluey.sh`.
- No native overlay, audio, desktop runtime, or backend runtime files were changed.
- The unrelated untracked review handoff file remains untouched.

## Remaining QA/Gates

- Owner visual QA on a signed-in live account header.
