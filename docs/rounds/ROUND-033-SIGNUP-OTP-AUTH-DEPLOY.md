# Round 033 - Signup OTP Auth Deploy — 2026-06-06

## Objective

Make browser account creation require email OTP verification before an account is created or desktop tokens can be issued.

## Changes

- Added the two-step signup API:
  - `POST /auth/signup/start`
  - `POST /auth/signup/confirm`
- Added `signup_otps` storage for pending signup attempts.
- Email OTPs expire after 10 minutes.
- OTP confirmation is capped at 5 failed attempts per pending signup.
- OTP hashes use HMAC-SHA256 with constant-time comparison.
- Passwords are bcrypt-hashed before being stored in pending signup state.
- Confirmed accounts are created with `email_verified_at` already set.
- Retired the legacy direct `POST /auth/signup` path with `410 Gone` so account creation cannot bypass OTP.
- Updated the web create-account UI to request and verify a code.

## Security Notes

- Raw OTP codes are never stored.
- Logs include hashed email prefixes only.
- Secrets are not committed; live provider and mail keys remain in the droplet environment file.
- The legacy direct signup path remains routed only to give old clients an explicit migration error.

## Verification

Local verification before deploy:

```bash
cargo fmt --all --check
cargo clippy --all-targets -- -D warnings
cargo build --all-targets
cargo test --all-targets
cd server && cargo test
cd crates/cue-dashboard/ui && npm run build
git diff --check
```

Focused auth coverage:

```bash
cd server && cargo test --test integration_e2e signup_otp_email_confirms_and_marks_email_verified
cd server && cargo test --test integration_e2e configured_admin_email_signup_gets_admin_access
cd server && cargo test --test integration_e2e legacy_signup_endpoint_is_retired
```

Live smoke after deploy:

```bash
curl -fsS https://bluey.sh/health
curl -sS -o /tmp/bluey-legacy-signup.json -w '%{http_code}' \
  -X POST https://bluey.sh/auth/signup \
  -H 'content-type: application/json' \
  --data '{"email":"legacy-smoke@example.invalid","password":"BlueySmokePass123"}'
curl -fsS \
  -X POST https://bluey.sh/auth/signup/start \
  -H 'content-type: application/json' \
  --data '{"email":"internal-otp-smoke@example.invalid","password":"BlueySmokePass123"}'
```

Observed:

- `/health` reported commit `3f59212`.
- Legacy `/auth/signup` returned `410`.
- `/auth/signup/start` returned `200` with `expires_in_secs: 600`.
- Server journal showed `signup OTP sent` with an `email_hash`, not the raw address.

## Deployed

- Host: `bluey.sh`
- API service: `bluey-api.service`
- Live binary: `/usr/local/bin/bluey-server`
- Static web root: `/var/www/bluey`
- Deployed code commit: `3f59212`

## Follow-Ups

- Add a periodic cleanup job for expired `signup_otps` rows if pending rows become noisy.
- Add customer-facing copy for "code expired" and "too many attempts" in the web form.
