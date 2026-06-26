# Round 155 - Trial Abuse And RAG Cost Guards - 2026-06-23

## What changed

- Added server-side trial grant tracking with hashed email, IP, device, and user-agent signals.
- Added `trial_grants` and `trial_abuse_events` tables for SQLite and the Postgres runtime schema.
- Added configurable trial limits:
  - `BLUEY_TRIAL_MAX_PER_EMAIL`
  - `BLUEY_TRIAL_MAX_PER_EMAIL_DOMAIN_PER_DAY`
  - `BLUEY_TRIAL_MAX_PER_DEVICE`
  - `BLUEY_TRIAL_MAX_PER_IP_PER_DAY`
  - `BLUEY_TRIAL_MAX_PER_IP_USER_AGENT_PER_DAY`
- Added optional Turnstile signup protection through `BLUEY_TURNSTILE_SITE_KEY` and `BLUEY_TURNSTILE_SECRET_KEY`.
- Added `BLUEY_REQUIRE_TURNSTILE` so production can fail closed when Turnstile is required but not fully configured.
- Added browser signup wiring so the web page asks `/auth/captcha/config`, renders Turnstile only when configured, and sends a stable local device fingerprint with signup requests.
- Added admin visibility at `/admin/trial-abuse` and a compact admin-only dashboard section for recent trial-denial and CAPTCHA-failure events.
- Added hashed email-domain velocity checks so throwaway aliases on one domain cannot create unlimited trials.
- Closed the managed RAG embedding trial loophole by consuming trial seconds for successful `/router/embed` calls instead of letting trial accounts embed unlimited chunks for free.
- Closed the chunked transcription trial loophole by consuming trial seconds for successful `/router/transcribe` calls.
- Extended cloud preflight so Turnstile partial configuration fails early, and production/required Turnstile deployments fail when keys are missing.

## Behavior

- Normal paid users are unchanged.
- Trial users can still use Bluey, but embeddings and chunked STT now spend trial quota.
- Repeated signup attempts from the same email, device fingerprint, IP, or IP plus user agent are denied before a new trial is granted.
- Repeated signup attempts from the same email domain are rate-limited per day.
- Abuse identifiers are stored as one-way hashes, not raw email, IP, or device strings.
- If Turnstile is not configured, signup still works with the velocity and fingerprint rules.
- If Turnstile is configured, signup requires a valid Turnstile token.
- If `BLUEY_REQUIRE_TURNSTILE=1`, signup fails closed until both Turnstile keys are present.

## Production knobs

- Start with Turnstile enabled before increasing free trial minutes or marketing traffic.
- Keep the defaults conservative:
  - One trial per email.
  - Twenty-five trials per email domain per day.
  - One trial per device fingerprint.
  - Three trials per IP per day.
  - Five trials per IP plus user agent per day.
- Watch the admin abuse section for denied trial spikes, repeated devices, repeated IP hashes, and CAPTCHA failures.

## Verification

- `cargo test --manifest-path server/Cargo.toml`
- `cargo test --manifest-path server/Cargo.toml router_embed_consumes_trial_seconds_and_records_bluey_cost --test integration_e2e -- --nocapture`
- `node --check web/assets/bluey-site.js`
- `bash -n scripts/bluey-cloud-preflight.sh`

## Latest verification

- `cargo test trial_abuse -- --nocapture` from `server/`
- `cargo test signup_start_requires_turnstile_when_flagged --test integration_e2e -- --nocapture` from `server/`
- `cargo check` from `server/`
- `bash -n scripts/bluey-cloud-preflight.sh`
