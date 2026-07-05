# Round 353 - Password Reset and Landing Copy

## Trigger

The owner asked whether Bluey should use a Pinky-style reset code and then asked for the landing headline to say "Stay present, Stay unseen".

## Root Cause/Fix

- Confirmed Bluey's current backend reset flow is a secure emailed reset link with a 24-hour token, not Pinky's short reset-code flow.
- Kept the request screen truthful: it asks for email and says Bluey sends a secure password reset link.
- Made the opened reset-link screen Pinky-like without changing backend auth:
  - branded Bluey logo and wordmark,
  - title changes to "New password" when `token` is present,
  - new-password input includes the eye toggle,
  - primary action says "Reset password",
  - Home, Download, and Pricing links are restored inside the card.
- Added success/error message tone styling for recovery messages in dark and light themes.
- Updated the landing headline to "Stay present, stay unseen."
- Bumped static web cache keys to `2026070410`.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- Local route-aware preview server on `127.0.0.1:4178`.
- In-app browser smoke:
  - `/password-reset?check=2026070410` shows a compact 330px reset-link request card, header, footer, and Home/Download/Pricing recovery links,
  - `/password-reset?token=fake-token&check=2026070410` shows "New password", password eye, "Reset password", and the restored links,
  - local light-theme reset page keeps readable contrast and the compact card,
  - `/` shows `Stay present, stay unseen.` and serves cache key `2026070410`.
- Static live deploy:
  - `rsync -av --delete ... web/ root@165.227.77.152:/var/www/bluey/`
  - excluded `/install.sh`, `/install.ps1`, `/latest.json`, `/latest.json.sig`, `/releases/***`, and `/backups/***`.
- Live checks:
  - `https://bluey.sh/` serves `bluey-site.css?v=2026070410`, `bluey-site.js?v=2026070410`, and the new headline,
  - live `bluey-site.js?v=2026070410` contains the "New password" token state and secure reset-link request copy,
  - live `bluey-site.css?v=2026070410` contains the recovery-card and recovery-message styles,
  - `https://bluey.sh/install.sh` and `https://bluey.sh/latest.json` still return the protected installer/release responses.

## Current State

- Only static web UI files and this round doc changed.
- Static web is live on `bluey.sh`.
- No native overlay, audio, desktop runtime, or backend runtime files were changed.
- The unrelated untracked review handoff file remains untouched.

## Remaining QA/Gates

- Owner visual QA on the live landing and password-reset pages.
