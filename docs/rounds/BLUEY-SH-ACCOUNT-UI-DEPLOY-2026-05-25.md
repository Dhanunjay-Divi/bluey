# Bluey.sh Account UI + Droplet Deploy - 2026-05-25

## Scope

This pass turns the static `bluey.sh` fallback into a usable account/reload surface and deploys the current server/web stack to the DigitalOcean droplet.

## Product Flow

- `/` remains the public product landing page.
- `/account`, `/reload`, and `/link` render a focused account UI from `web/index.html`.
- The account UI supports:
  - Sign in with email/password via `/auth/login`.
  - Create account via `/auth/signup`.
  - Token storage in browser `localStorage` for the alpha account page.
  - Balance from `/account/me`.
  - Last-7-day usage from `/account/usage`.
  - $30 reload via `/billing/checkout`, redirected to provider-hosted checkout.
- The UI copy is provider-neutral where possible, with Square called out only on the hosted-checkout button.

## Security Notes

- No Square secrets are committed.
- No pasted production/sandbox Square token substrings are present in the repo.
- `BLUEY_HOST_OVERLAY_CAPTURE_VISIBLE=1` remains documented as local smoke-test-only and must not be set in production.
- Browser token storage is acceptable for the alpha account page; production should migrate to an httpOnly-cookie session or short-lived device approval flow.

## Verification

- `cargo fmt --all --check`
- `cargo clippy --all-targets -- -D warnings`
- `cd server && cargo clippy --all-targets -- -D warnings`
- `cargo test -p cue-cli`
- `cd server && cargo test square -- --nocapture`
- `git diff --check`
- Local browser preview of `http://127.0.0.1:8900/account`

## Operator Inputs Still Needed

Square checkout cannot go live until these are configured on the droplet:

- `SQUARE_SANDBOX_LOCATION_ID`
- `SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY`
- `SQUARE_PRODUCTION_LOCATION_ID`
- `SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY`

The previously supplied Square app IDs and access tokens are intentionally not written into repository files.

## Deployment Target

- Domain: `bluey.sh`
- Droplet IPv4: `165.227.77.152`
- Service: `bluey-api.service`
- Static root: `/var/www/bluey`
- API env: `/etc/bluey-api/bluey-api.env`

