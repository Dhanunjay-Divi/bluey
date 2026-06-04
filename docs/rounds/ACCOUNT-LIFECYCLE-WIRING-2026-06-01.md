# Account Lifecycle Wiring - 2026-06-01

## Goal

Make the first customer path coherent from install to active paid use:

1. User installs Bluey.
2. User runs `bluey on` and can use local overlay/session features immediately.
3. When managed cloud features are needed, the user signs in or creates an account in the browser.
4. Desktop receives tokens through a one-time handoff and stores them in the OS keychain.
5. Balance/account/billing/logout/delete actions work from the dashboard and CLI.

## What Changed

- `bluey login` now uses the server device-flow endpoints instead of the stale callback URL.
  - Starts `POST /auth/device/start`.
  - Opens `https://bluey.sh/login?user_code=XXXX-XXXX`.
  - Polls `POST /auth/device/poll` for up to 10 minutes.
  - Runs the polling client with an in-memory token store, then saves the returned access/refresh tokens through the existing keyring path. This keeps first login from depending on keyring availability before tokens exist.
- Server device polling now consumes an approved device code before issuing tokens, so parallel retries cannot mint multiple token pairs from the same approval.
- Server device approval now only applies to pending codes, so a second signed-in account cannot overwrite an already-approved code before the original CLI poll completes.
- Server device-flow verification URI now points at `/login`, which is the product account page customers see first. `/link` remains a compatibility alias for older desktop/browser links.
- `web/index.html` now supports two browser-to-desktop handoff modes:
  - `/login?user_code=...`: approve a terminal/device login after sign-in or account creation.
  - `/login`: mint a one-time `bluey://link?code=...` deep link for the Tauri onboarding flow.
- Dashboard Settings now shows an explicit sign-in/create-account action when no keyring token exists, instead of leaving the Account card in a permanent loading-looking state.

## Product Flow

### CLI terminal flow

```text
bluey on
  -> starts overlay/session
  -> if not signed in, opens https://bluey.sh/login and prints the same URL as fallback

bluey login
  -> prints a short user code
  -> opens /login?user_code=...
  -> browser sign-in/create-account approves the code
  -> CLI stores tokens in keyring

bluey usage / bluey credits / managed answers
  -> use keyring token
```

### Dashboard onboarding flow

```text
Dashboard Sign in with browser
  -> opens /login
  -> browser sign-in/create-account mints a one-time auth link
  -> browser redirects to bluey://link?code=...
  -> dashboard exchanges code and stores tokens in keyring
```

## Verification

- `cargo fmt --all --check`
- `cargo check -p cue-cli -p cue-dashboard -p cue-cloud-client`
- `cargo test -p cue-cli device_login_url -- --nocapture`
- `cd server && cargo test auth_device_ --test integration_e2e`
- `cd crates/cue-dashboard/ui && npm run build`

## Remaining Before Production Account Smoke

- Deploy `bluey-server` behind `https://bluey.sh` so `/auth/*`, `/account/*`, `/billing/*`, `/sync/*`, and `/rag/*` are live behind TLS.
- Configure Square production/sandbox credentials on the server only.
- Configure SMTP for verify/reset emails.
- Run the real smoke:
  - create account on `/login`
  - `bluey login` device approval
  - managed answer debit
  - reload credits
  - cloud sync/RAG round trip
  - logout/delete/export paths
