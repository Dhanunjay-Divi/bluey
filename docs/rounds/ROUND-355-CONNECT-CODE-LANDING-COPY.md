# Round 355 - Connect Code Landing Copy

## Trigger

The owner pointed at the landing-page `session code` / `Join` control and asked whether it should be clearer as a connect code.

## Fix

- Renamed the landing-page placeholder from `session code` to `connect code`.
- Renamed the action from `Join` to `Connect`.
- Added accessible labels for the form and input:
  - `Connect a Bluey desktop code`
  - `Bluey connect code`
- Added helper copy: `Have a code from Bluey desktop? Enter it here to connect this browser.`
- Reduced the feature note under the form to one clearer line.
- Kept the existing `user_code` field name and `/login` target so the desktop-link flow remains unchanged.
- Bumped static web cache keys to `2026070412`.

## Verification

- `git diff --check`
- Local route-aware preview server on `127.0.0.1:4178`.
- In-app browser smoke:
  - desktop landing shows `connect code`, `Connect`, the new helper line, and cache key `2026070412`,
  - desktop form remains `386px` wide and `43px` tall,
  - mobile landing shows `connect code`, `Connect`, centered helper text, and no horizontal overflow.
- Static live deploy:
  - `rsync -av --delete ... web/ root@165.227.77.152:/var/www/bluey/`
  - excluded `/install.sh`, `/install.ps1`, `/latest.json`, `/latest.json.sig`, `/releases/***`, and `/backups/***`.
- Live checks:
  - `https://bluey.sh/` serves `bluey-site.css?v=2026070412`, `bluey-site.js?v=2026070412`, `connect code`, `Connect`, and the new helper line,
  - live `bluey-site.css?v=2026070412` contains the `connect-help` styles,
  - `https://bluey.sh/install.sh` and `https://bluey.sh/latest.json` still return the protected installer/release responses.

## Current State

- Only static web UI files and this round doc changed.
- Static web is live on `bluey.sh`.
- No native overlay, audio, desktop runtime, or backend runtime files were changed.
- The unrelated untracked review handoff file remains untouched.

## Remaining QA/Gates

- Owner visual QA on the live landing connect-code form.
