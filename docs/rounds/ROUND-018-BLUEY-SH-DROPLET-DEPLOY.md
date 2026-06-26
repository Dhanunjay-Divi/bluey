# Round 018 - Bluey.sh Droplet Deploy - 2026-06-01

## Summary

Bluey Cloud is now running on the DigitalOcean droplet behind `https://bluey.sh`.

This pass deployed the current `bluey-server` to the droplet, wired Square sandbox
and production credentials into the server environment, created Square webhook
subscriptions for both environments, deployed the current static web UI, and
smoke-tested public auth/account/checkout routes.

## Infrastructure State

- Domain: `bluey.sh`
- Public IPv4: `165.227.77.152`
- Server host: `bluey-brain`
- OS: Ubuntu 24.04 LTS, x86_64
- Reverse proxy: Caddy
- App service: `bluey-api.service`
- App binary: `/usr/local/bin/bluey-server`
- App data: `/opt/bluey-api/bluey.db`
- App env: `/etc/bluey-api/bluey-api.env`
- Static web root: `/var/www/bluey`

DNS is resolving:

- `bluey.sh` A -> `165.227.77.152`
- `www.bluey.sh` CNAME -> `bluey.sh`

Firewall is active with only SSH, HTTP, and HTTPS open publicly.

## What Changed

### Server Deploy

Built `bluey-server` on the droplet from the current repository source and
installed it to `/usr/local/bin/bluey-server`.

The service restarted cleanly and `/health` reports healthy through the public
domain.

### Square Billing

Configured Square billing in sandbox mode first. Both sandbox and production
credentials are present in the server env so promotion is an environment switch,
not a code change.

Created Square webhook subscriptions for:

- `order.updated`
- `payment.updated`

Webhook destination:

- `https://bluey.sh/billing/square/webhook`

Secrets are intentionally not recorded in this document.

### Square Compatibility Fix

Square rejects `order.reference_id` values longer than 40 characters. The prior
payload used `bluey_reload:<account_uuid>`, which is 49 characters.

The server now uses a compact Square-safe reference id:

- `br_<32 hex account id>`

The full account id remains in Square order metadata:

- `metadata.bluey_account_id`
- `metadata.bluey_amount_cents`

Webhook extraction still supports the old `bluey_reload:<id>` reference format
for backward compatibility.

### Web Deploy

Copied `web/` to `/var/www/bluey` and removed macOS AppleDouble sidecar files.

Public routes verified:

- `/`
- `/link`
- `/account`
- `/reload`

## Verification

Commands run locally and on the droplet:

```bash
curl -fsS https://bluey.sh/health
curl -fsS https://bluey.sh/pricing/tiers
curl -fsS -X POST https://bluey.sh/auth/signup ...
curl -fsS https://bluey.sh/account/me ...
curl -fsS -X POST https://bluey.sh/billing/checkout ...
```

Results:

- Health endpoint returns `status=ok`.
- Pricing endpoint returns the configured tier table.
- Throwaway signup returns access and refresh tokens.
- `GET /account/me` works with the access token.
- `POST /billing/checkout` returns a Square sandbox checkout URL.
- Caddy config validates.
- `bluey-api.service` is active.

Focused local tests:

```bash
cargo fmt --all --check
cd server && cargo test square_ --quiet
```

Focused droplet tests:

```bash
cargo fmt --all --check
cargo test --manifest-path server/Cargo.toml square_ --quiet
cargo build --manifest-path server/Cargo.toml --release
```

## Still Missing Before Full Production AI Testing

The server is alive and billing/auth are smoke-tested, but production AI answers
still need provider and messaging configuration:

- OpenAI key pool
- Anthropic key pool
- Deepgram key pool
- Optional Gemini/Cerebras/Groq keys if we route to them
- SMTP provider credentials for email verification and password reset
- Redis, if we want shared provider-capacity state before multi-instance scale

Current server env has Square and core Bluey settings only.

## Next Step

Provide provider keys and SMTP settings, then configure the droplet env and run
the end-to-end desktop login -> managed answer -> balance decrement smoke.
