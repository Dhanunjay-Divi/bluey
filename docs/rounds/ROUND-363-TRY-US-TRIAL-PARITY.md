# Round 363 - Try Us Trial Parity

## Trigger

The owner reported that the landing-page Try Us button was not working as expected and asked to compare Pinky's implementation in `/Users/uno/Downloads/pinky-git`.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Root Cause

- Pinky has a complete Try Us path: frontend human-check/status UX plus a backend temporary account endpoint.
- Bluey's current website had Try Us UI copy, but production `POST /auth/trial/start` returned `404`.
- Bluey's web Try Us button also did not handle the Pinky-style human-check flow before posting to the server.

## Fix

- Ported the temporary-trial backend shape into the runtime branch:
  - `/auth/trial/start`
  - `/auth/trial/convert/start`
  - `/auth/trial/convert/confirm`
  - temporary account fields and expiry checks
  - trial abuse grant recording
- Kept trial creation controlled by existing hashed trial-abuse signals and rate limits.
- Updated web Try Us UX to match the Pinky pattern:
  - desktop-required message on phones
  - active/signed-in status instead of minting another trial
  - Turnstile human-check modal before trial creation when configured
  - trial credentials remain visible with copy action after creation
- Updated CSP example to allow Cloudflare Turnstile script/frame/connect when enabled.

## Verification

Commands run:

```bash
node --check web/assets/bluey-site.js
cargo check
cargo test --test integration_e2e trial -- --nocapture
curl -i -sS -X POST https://bluey.sh/auth/trial/start -H 'Content-Type: application/json' --data '{"device_fingerprint":"codex-round354-smoke-20260705"}'
```

Results:

- Web JS syntax check passed.
- Server `cargo check` passed.
- Trial integration tests passed:
  - `trial_start_creates_temporary_account_with_fifteen_minutes`
  - `temporary_trial_converts_to_verified_account`
  - `router_embed_consumes_trial_seconds_and_records_bluey_cost`
- Pre-fix live smoke confirmed the production endpoint returned `404`, explaining why Try Us could not work end to end before deployment.

## Mac And Windows Parity

Try Us is a website/account/API flow. It applies equally to Mac and Windows downloads because the generated account tokens and device-code linking are platform-neutral.

## Deployment Gate

Deploy both together:

- production API from `/Users/uno/Downloads/cue-runtime-stream-attachments`
- production static website from `/Users/uno/Downloads/cue`
- live Caddy CSP with Cloudflare Turnstile allowances

Post-deploy smoke should confirm:

- `/auth/trial/start` no longer returns `404`
- Try Us creates a temporary account or gives a clear trial-limit/human-check message
- `/auth/captcha/config` remains compatible when Turnstile is off
