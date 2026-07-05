# Round 383 - Desktop Balance Account Refresh

## Goal

Fix the real balance mismatch where the web dashboard and local overlay can show different balances after a desktop is moved or re-logged into another Bluey account.

## Problem

The dashboard balance and overlay balance both come from `/account/me.balance_cents`, but the local daemon could keep using an old account source:

- `build_cloud_client` preferred env access tokens before the saved Bluey account profile.
- The daemon balance poller kept an in-memory client; after `bluey login` saved a new account, the old poller could continue polling the previous account because that token was still valid.
- `CloudStatus` and post-login refresh paths refreshed UI state without first restarting the balance poll loop.

That made the web account show one balance while the overlay continued showing the previous desktop account balance.

## Changes

- Made the saved desktop Bluey account the default source of truth for cloud clients.
- Kept env-token mode as a fallback when no saved account exists.
- Added explicit dev override via `BLUEY_PREFER_ENV_CLOUD_TOKEN=1` or `CUE_PREFER_ENV_CLOUD_TOKEN=1`.
- Made live STT/managed cloud token selection follow the same saved-account-first rule.
- Restarted the daemon balance poll loop on `CloudStatus` and after background desktop login so old in-memory clients cannot overwrite the overlay with stale-account balance snapshots.
- Made cloud status metadata prefer the saved account unless explicit env-token override is enabled.

## Verification

- `cargo check -p cue-daemon`
- `git diff --check -- crates/cue-daemon/src/app.rs`

## Notes

The web page cannot directly rewrite a running desktop profile by itself. The desktop still needs to complete the device-code login/move flow or `bluey login` so the new account tokens are saved locally. After that, the daemon now reloads the account source and refreshes the overlay balance correctly.
