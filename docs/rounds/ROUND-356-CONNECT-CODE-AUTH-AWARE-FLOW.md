# Round 356 - Connect Code Auth-Aware Flow

## Trigger

The owner asked to change the landing helper copy to "Have a code from Bluey desktop? Enter it here to connect your account." and asked what should happen after clicking Connect when the visitor is logged in or logged out.

## Fix

- Updated the landing helper copy to say `connect your account`.
- Added a compact `aria-live` status line under the connect-code form.
- Kept signed-out behavior as a login flow:
  - the form still submits `user_code` to `/login`,
  - the code is normalized and remembered,
  - the login page shows the pending desktop-connect hint.
- Added signed-in behavior:
  - if the browser already has an account token, the landing `Connect` button now calls `/auth/device/approve` directly,
  - success shows `Connected. Return to Bluey desktop; it will finish automatically.`,
  - if auth is stale and refresh clears the token, the code is preserved and the user is sent to `/login?user_code=...`.
- Added dark/light status colors and disabled button styling while connecting.
- Bumped static web cache keys to `2026070413`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Local route-aware preview server on `127.0.0.1:4178`.
- In-app browser smoke:
  - landing shows `connect code`, `Connect`, and `Have a code from Bluey desktop? Enter it here to connect your account.`,
  - empty status line is hidden,
  - submitting `ab12 cd34` while signed out navigates to `/login?user_code=AB12CD34`,
  - login page shows the pending desktop connect hint for `AB12CD34`.
- Static live deploy:
  - `rsync -av --delete ... web/ root@165.227.77.152:/var/www/bluey/`
  - excluded `/install.sh`, `/install.ps1`, `/latest.json`, `/latest.json.sig`, `/releases/***`, and `/backups/***`.
- Live checks:
  - `https://bluey.sh/` serves `bluey-site.css?v=2026070413`, `bluey-site.js?v=2026070413`, `connect code`, `Connect`, and `connect your account`,
  - live `bluey-site.js?v=2026070413` contains `approveProductConnectCode`, `/auth/device/approve`, and the stale-auth `/login?user_code=...` fallback,
  - live `bluey-site.css?v=2026070413` contains the connect-status and disabled-button styles,
  - `https://bluey.sh/install.sh` and `https://bluey.sh/latest.json` still return the protected installer/release responses.

## Current State

- Only static web UI files and this round doc changed.
- Static web is live on `bluey.sh`.
- No native overlay, audio, desktop runtime, or backend runtime files were changed.
- The unrelated untracked review handoff file remains untouched.

## Remaining QA/Gates

- Owner visual QA on the live connect-code form.
