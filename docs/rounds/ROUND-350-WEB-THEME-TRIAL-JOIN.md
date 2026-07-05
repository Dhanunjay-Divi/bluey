# Round 350 - Web Theme, Trial, and Join Flow

## Trigger

The owner asked to make Bluey's homepage match Pinky's element set while keeping Bluey's colors, add a real white/dark theme changer, make Try Us create a 24-hour temporary login with 15 free minutes, and keep the session-code Join flow connected to the local desktop link path.

## Root Cause/Fix

- Made the homepage theme toggle functional with persistent dark/light Bluey themes.
- Kept the Pinky-style product elements on Bluey: `Get Started`, `Try Us`, session-code `Join`, terminal preview, and three bottom action cards.
- Added a Try Us modal that can show the generated temporary username/password, expiration, dashboard/download actions, and beginner-safe unavailable/limit copy.
- Wired Try Us to new auth endpoints:
  - `POST /auth/trial/start`
  - `POST /auth/trial/convert/start`
  - `POST /auth/trial/convert/confirm`
- Temporary accounts now:
  - last 24 hours unless converted,
  - receive 15 minutes of trial usage,
  - are rejected after expiration for auth, refresh, device polling, and link exchange,
  - cannot add credits until saved as a regular account.
- Added dashboard UI to save a temporary trial by verifying a real email address.
- Preserved the existing device-code join path by normalizing the homepage session code into `/login?user_code=...`.
- Added account/schema support for temporary account state:
  - `accounts.is_temporary`
  - `accounts.temporary_expires_at`
  - `signup_otps.account_id`
- Updated the Postgres runtime compatibility schema so production startup migrations can self-heal the added columns.

## Verification

- `node --check web/assets/bluey-site.js`
- `git diff --check`
- `cargo check --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml trial`
- `cargo test --manifest-path server/Cargo.toml temporary`
- `cargo test --manifest-path server/Cargo.toml db::signup_otps::tests::upsert_resets_attempts_and_updates_hashes`
- Local static server: `python3 -m http.server 8787 --directory web`
- In-app browser local smoke:
  - confirmed homepage elements: Download/Login nav, theme toggle, `Get Started`, `Try Us`, `session code`, `Join`, terminal preview, and Start free/Add credits/Teams cards.
  - confirmed light theme uses a bright Bluey surface with readable root text.
  - confirmed dark theme returns to the black Pinky-style stage with Bluey cyan accents.
  - confirmed Try Us modal opens and uses friendly unavailable copy when the local static server cannot serve the auth API.

## Current State

- The branch contains the web UI changes plus the minimum server auth/account/schema changes needed for a strict 24-hour temporary trial.
- No native overlay, audio, or desktop runtime files were changed.
- The unrelated untracked review handoff file was left untouched.

## Remaining QA/Gates

- Production Try Us requires both static web deploy and API binary rollout so `/auth/trial/start` is available on `bluey.sh`.
- After API rollout, live-smoke a real temporary account from `https://bluey.sh/`, verify the dashboard save-account flow, and verify an expired temporary account is rejected.
