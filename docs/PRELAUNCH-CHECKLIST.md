# Bluey v0.2 — Production Pre-Launch Checklist

> **Owner:** product + ops jointly
> **Status:** living doc — tick as items complete
> **Companion:** `docs/PRODUCTION-DEPLOY-RUNBOOK.md`

This is the master gate before public alpha. Every item must be ticked or explicitly waived (with rationale) before announcing the product.

## Code-complete (already shipped on `feat/phase-3-round-12`)

- [x] Server money path: `/router/{complete,embed,transcribe}` + `/router/complete/stream`
- [x] Auth: signup / login / refresh / device flow / deep-link flow
- [x] Auth: email-verify start+confirm with SMTP
- [x] Auth: password-reset start+confirm with SMTP
- [x] Billing: Stripe Checkout + webhook + Customer Portal + auto-topup
- [x] GDPR: `/account/delete` (cascade + webhook scrub) + `/account/export`
- [x] Admin: `/admin/customers`, `/admin/metrics`, `/admin/health`
- [x] Per-IP rate limiting on auth + `/router/complete` w/ XFF behind trusted proxy
- [x] Daemon: BlueyManagedProvider + ManagedPolicy + LocalFallbackPolicy
- [x] Daemon: BalanceWatch poll + Stage 19 cost label plumbing
- [x] CLI: `bluey login` / `usage` / `credits` / `logout` / `portal` / `export` / `delete-account`
- [x] Onboarding: deep-link wizard + tray Invisible toggle + F19 hotkey
- [x] Disguise: 4-mode picker + tray submenu + auto-disguise heuristic + persisted prefs
- [x] Pipeline at tip: fmt + clippy -D + tests + builds across cue + server

## Operational infrastructure

### Domains + DNS

- [ ] **`bluey.sh`** registered, A/AAAA records pointed at the bluey-server droplet's public IP
- [ ] **`bluey.dev`** registered, A/AAAA records pointed at the marketing site host
- [ ] DNSSEC enabled on both
- [ ] CAA records restricting cert issuance to Let's Encrypt
- [ ] Email DNS for `noreply@bluey.dev`: SPF, DKIM, DMARC records published

### Server host

- [ ] DigitalOcean droplet provisioned per `docs/PRODUCTION-DEPLOY-RUNBOOK.md` §1
- [ ] Caddy installed + Caddyfile from `ops/Caddyfile.example` deployed
- [ ] `bluey-server` binary built `--release` and installed to `/usr/local/bin/`
- [ ] `/etc/bluey-api/bluey-api.env` populated with all 14 required env vars (mode 0600)
- [ ] systemd unit from `ops/bluey-api.service.example` installed + enabled
- [ ] `/admin/health` returns 200 over HTTPS with valid TLS cert
- [ ] `/pricing/tiers` returns the canonical tier numbers over HTTPS
- [ ] `journalctl -u bluey-api.service --since "10 min ago"` shows no error-level logs
- [ ] Firewall: only 22, 80, 443 open
- [ ] Backup script installed at `/usr/local/sbin/backup-bluey-db.sh` + cron entry verified
- [ ] **At least one** off-host backup destination configured (S3 or rsync target)
- [ ] First backup completed successfully + checksum verified

### Stripe

- [ ] Stripe account in live mode
- [ ] `STRIPE_SECRET_KEY` (sk_live_...) on the server matches the live account
- [ ] Webhook endpoint registered: `https://bluey.sh/billing/webhook`
- [ ] Webhook events subscribed: `checkout.session.completed` + `payment_intent.succeeded`
- [ ] `STRIPE_WEBHOOK_SECRET` (whsec_...) on the server matches the registered webhook
- [ ] Stripe live test: real $30 reload from a test card → balance credited within 30s
- [ ] Stripe live test: auto-topup fires when balance drops below threshold + saved PaymentMethod
- [ ] Stripe Customer Portal session URL works end-to-end (`POST /billing/portal` → portal opens → cancel auto-topup → confirmed disabled)

### SMTP

- [ ] SMTP provider account live (Postmark / SendGrid / SES)
- [ ] `BLUEY_SMTP_*` env vars set in `/etc/bluey-api/bluey-api.env`
- [ ] Sender domain (`noreply@bluey.dev`) DKIM-signed and verified at provider
- [ ] Live test: `/auth/verify-email/start` → email arrives in <30s, link opens
- [ ] Live test: `/auth/password-reset/start` → email arrives in <30s, link opens
- [ ] Bounce handling configured (provider dashboard → forward to ops@bluey.dev)

### Web pages on `bluey.dev`

These are NOT in this repo (separate web codebase). Must exist before public alpha:

- [ ] `/` — landing page with download button
- [ ] `/link` — OAuth-style landing for the `bluey://` deep-link flow (signup/signin form, calls `/auth/link/mint` after auth, redirects browser to `bluey://link?code=...`)
- [ ] `/reload` — Stripe Checkout redirect target (after pay → returns to `/account?reload=success`)
- [ ] `/account` — user-facing balance + usage + sign-out (calls `/account/me`, `/account/usage`, `/billing/portal`)
- [ ] `/docs/disguise` — explainer page that the dashboard "Why?" link points at
- [ ] `/docs/privacy` — privacy policy
- [ ] `/docs/terms` — terms of service
- [ ] OG / favicon assets

### macOS app distribution (Pinky-style: NO Apple Developer ID required for v0.2 alpha)

We ship the `.app` bundle without a paid Apple Developer ID. The
installer ad-hoc signs the bundle and strips the quarantine bit so
first-launch is clean. Two distribution paths, ship both:

#### Path A: One-line installer (`curl ... | bash`)
- [ ] `ops/install/install.sh` hosted at `https://bluey.dev/install.sh`
- [ ] Release tarballs hosted at `https://bluey.dev/releases/v0.2.0/Bluey-aarch64.tar.gz` and `Bluey-x86_64.tar.gz`
- [ ] Smoke on a clean Mac: `curl -fsSL https://bluey.dev/install.sh | bash` finishes cleanly
- [ ] Bluey.app launches from /Applications without a Gatekeeper hard-block
- [ ] `bluey://` URL scheme registers (verify `lsregister -dump | grep bluey`)
- [ ] First-run onboarding deep-link flow works end-to-end

#### Path B: Homebrew cask (`brew install --cask bluey`)
- [ ] `bluey-dev/homebrew-bluey` GitHub repo created
- [ ] `ops/Casks/bluey.rb` published in that tap
- [ ] `brew tap bluey-dev/bluey` + `brew install --cask bluey` succeeds on a clean Mac
- [ ] Postflight ad-hoc sign + quarantine strip runs cleanly
- [ ] `bluey` CLI is on `$PATH` after install (binary stanza)

#### Optional (v1.0 GA polish, NOT v0.2 gate)
- [ ] Apple Developer Program ($99/yr) + Developer ID Application cert
- [ ] Notarized DMG via `notarytool submit` + `stapler staple`
- [ ] Replaces ad-hoc-signed alpha distribution path

### Monitoring

- [ ] Prometheus or equivalent scraping `/admin/metrics` with admin bearer
- [ ] Alert: `bluey_mark_complete_failures_estimated > 0` for 10+ minutes → page on-call
- [ ] Alert: `bluey_request_idempotency_in_progress > 100` sustained → page
- [ ] Alert: HTTP 5xx rate > 1% over 5 min on Caddy access log → page
- [ ] Alert: SSH login outside maintenance window → notify
- [ ] Uptime monitor (UptimeRobot / BetterStack) hitting `/admin/health` every 60s
- [ ] On-call rotation defined + ack channel (PagerDuty / Slack)

### Legal + compliance

- [ ] Terms of Service published at `bluey.dev/docs/terms`
- [ ] Privacy Policy published at `bluey.dev/docs/privacy`
- [ ] Onboarding flow links to both before account creation
- [ ] GDPR-compatible data export tested (`bluey export` produces a valid JSON bundle)
- [ ] GDPR-compatible account deletion tested (`bluey delete-account --force` → all DB rows scrubbed including stripe_webhook_events)
- [ ] DPA template available for B2B customers who request one
- [ ] Data residency disclosed (server region, retention)

## Smoke test script

Run this end-to-end against production before announcing:

```bash
# 1. Sign up a test account.
EMAIL="smoke-$(date +%s)@example.com"
PW="testpassword123"
TOKEN=$(curl -fsS -X POST https://bluey.sh/auth/signup \
  -H "content-type: application/json" \
  -d "{\"email\":\"$EMAIL\",\"password\":\"$PW\"}" | jq -r .access_token)

# 2. Verify trial seconds initialised.
curl -fsS https://bluey.sh/account/me -H "Authorization: Bearer $TOKEN" | jq '.trial_seconds_remaining'
# Expect: 600

# 3. Reload via Checkout.
CHECKOUT_URL=$(curl -fsS -X POST https://bluey.sh/billing/checkout \
  -H "Authorization: Bearer $TOKEN" -H "content-type: application/json" \
  -d '{"amount_cents":3000}' | jq -r .checkout_url)
echo "Open $CHECKOUT_URL in browser, complete with Stripe test card 4242..."
read -p "Once paid, press Enter."

# 4. Verify balance.
BAL=$(curl -fsS https://bluey.sh/account/me -H "Authorization: Bearer $TOKEN" | jq '.balance_cents')
echo "Balance: $BAL cents"
[ "$BAL" -ge 3000 ] || { echo "FAIL: balance not credited"; exit 1; }

# 5. Run a cue.
curl -fsS -X POST https://bluey.sh/router/complete \
  -H "Authorization: Bearer $TOKEN" -H "content-type: application/json" \
  -d "{\"request_id\":\"smoke-$(uuidgen)\",\"system\":\"\",\"user\":\"hi\",\"lane\":\"instant\"}" \
  | jq '.text, .cost_cents, .balance_cents_after'

# 6. Delete account.
curl -fsS -X POST https://bluey.sh/account/delete -H "Authorization: Bearer $TOKEN"

# 7. Verify cleanup.
curl -fsS https://bluey.sh/account/me -H "Authorization: Bearer $TOKEN"
# Expect: 401 Unauthorized (account no longer exists)

echo "✅ Smoke pass"
```

## Sign-off

When every checkbox above is ticked, sign here:

- Operator: ____________________ Date: __________
- Product:  ____________________ Date: __________

Once signed: announce. Don't announce before.
