# ROUND-449 Delete Re-Signup No-Trial

Date: 2026-07-09
Branch: codex/bluey-web-ui-parallel-20260704
Backup thread: 019e133e-d92a-7830-8df0-3a050a4e22f6

## Problem

After an account was deleted, creating a new account with the same email showed the raw error `email_trial_already_used`.

That mixed two separate rules:

- The email should not receive another free trial.
- The user should still be able to recreate the account after deletion.

## Change

- Kept trial abuse protection intact.
- Allowed signup to continue when the only denial reason is `email_trial_already_used`.
- Created the replacement account with `trial_seconds_remaining = 0`.
- Added `trial_seconds` and `no_trial_reason` to `/auth/signup/start` so the web UI can explain the state.
- Added a friendly web message instead of showing raw backend codes.
- Added an integration test that signs up, deletes the account, signs up with the same email again, confirms OTP, and verifies the recreated account has no fresh trial.

## User Rule

Deleting an account does not reset trial eligibility. Recreating with the same email is allowed, but it starts without free trial minutes.

## Verification

- `cargo fmt --manifest-path server/Cargo.toml`
- `node --check web/assets/bluey-site.js`
- `cargo test --manifest-path server/Cargo.toml signup_after_account_delete_reuses_email_without_new_trial --quiet`
- `cargo test --manifest-path server/Cargo.toml signup_otp_email_confirms_and_marks_email_verified --quiet`
- `cargo test --manifest-path server/Cargo.toml configured_admin_email_signup_gets_admin_access --quiet`

## Deploy

Not deployed in this round. User asked not to deploy unless explicitly requested.
