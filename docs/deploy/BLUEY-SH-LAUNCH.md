# Bluey.sh Launch Path

Canonical production origin: `https://bluey.sh`.

Release golden rule: build once, store the artifact, deploy that exact
artifact to preprod, smoke preprod, then promote the same stored artifact
to prod. If anything changes after smoke, cut a new artifact and repeat
preprod. Never rebuild on prod.

For v0.2, do not split API onto `api.bluey.sh`. The desktop, installer,
landing page, deep-link flow, billing redirects, and managed API all use the
same origin:

- Web/install/static pages: `/`, `/install.sh`, `/releases/*`, `/login`,
  `/link` compatibility alias, `/reload`, `/account`, `/docs/*`
- Managed API: `/auth/*`, `/router/*`, `/billing/*`, `/sync/*`, `/rag/*`,
  `/stt/*`, `/usage/*`, `/pricing/*`, `/admin/*`, `/health`

Caddy routes the API paths to `bluey-server` and serves everything else from
`/var/www/bluey`.

## What To Prepare

1. Domain access
   - Registrar: Namecheap
   - Domain: `bluey.sh`
   - DNS records needed once the droplet exists:
     - `A @ <droplet_ipv4>`
     - optional `AAAA @ <droplet_ipv6>`
     - `CNAME www @`
     - CAA permitting Let's Encrypt
     - SPF, DKIM, DMARC for `hello@bluey.sh` after the email provider is chosen

2. Server host
   - DigitalOcean Ubuntu 24.04 droplet
   - Recommended initial size: 2 vCPU / 2 GB RAM / 50 GB SSD
   - Region: nearest the first users; one region is fine for the first 100
     controlled paid users, but it is not a worldwide ultra-low-latency claim.
     See `docs/deploy/FIRST-100-PAID-USERS.md` for upgrade triggers.
   - Firewall: SSH, HTTP, HTTPS only

3. Secrets
   - `BLUEY_JWT_SECRET`: `openssl rand -hex 32`
   - `BLUEY_BILLING_PROVIDER=square`
   - `SQUARE_ENVIRONMENT=sandbox` for preprod, `production` for prod
   - `SQUARE_SANDBOX_APPLICATION_ID`
   - `SQUARE_SANDBOX_ACCESS_TOKEN`
   - `SQUARE_SANDBOX_LOCATION_ID`
   - `SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY`
   - `SQUARE_PRODUCTION_APPLICATION_ID`
   - `SQUARE_PRODUCTION_ACCESS_TOKEN`
   - `SQUARE_PRODUCTION_LOCATION_ID`
   - `SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY`
   - `OPENAI_API_KEY`
   - `ANTHROPIC_API_KEY`
   - `DEEPGRAM_API_KEY`
   - SMTP credentials for `hello@bluey.sh`

   See `docs/deploy/SQUARE-BILLING.md` for the exact Square sandbox/production
   switching contract.

4. Release artifacts
   - `/var/www/bluey/install.sh`
   - `/var/www/bluey/releases/v0.1.0/bluey-0.1.0-darwin-arm64.tar.gz`
   - `/var/www/bluey/releases/v0.1.0/SHA256SUMS.txt`
   - static pages copied from `web/` or a separate polished web build

## First Deploy Order

1. Create the droplet and install Caddy.
2. Copy `ops/Caddyfile.example` to `/etc/caddy/Caddyfile`.
3. Copy `ops/bluey-api.service.example` to
   `/etc/systemd/system/bluey-api.service`.
4. Copy `ops/bluey-api.env.example` to
   `/etc/bluey-api/bluey-api.env`, fill real secrets, and set mode `0600`.
5. Build and copy `bluey-server` to `/usr/local/bin/bluey-server`.
6. Point `bluey.sh` DNS at the droplet.
7. Start Caddy and `bluey-api.service`.
8. Verify:

```bash
curl -fsS https://bluey.sh/health
curl -fsS https://bluey.sh/pricing/tiers | jq .
curl -fsSL https://bluey.sh/install.sh | bash
```

## Chrome/Admin Dashboard Use

Use logged-in Chrome only for actions that require account dashboards:

- Namecheap DNS
- DigitalOcean droplet/networking
- Square webhook setup for `/billing/square/webhook`
- SMTP provider domain verification

Stop before paid creation, DNS publish, Square production changes, or API-key
creation unless the operator explicitly confirms that action.
