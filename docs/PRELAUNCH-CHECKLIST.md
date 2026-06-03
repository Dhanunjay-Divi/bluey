# Bluey v0.2 — Production Pre-Launch Checklist

> **Owner:** product + ops jointly
> **Status:** living doc — tick as items complete
> **Companion:** `docs/PRODUCTION-DEPLOY-RUNBOOK.md` and
> `docs/deploy/BLUEY-SH-LAUNCH.md`

This is the master gate before public alpha. Every item must be ticked or explicitly waived (with rationale) before announcing the product.

## Code-complete (already shipped on `feat/phase-3-round-12`)

- [x] Server money path: `/router/{complete,embed,transcribe}` + `/router/complete/stream`
- [x] Auth: signup / login / refresh / device flow / deep-link flow
- [x] Auth: email-verify start+confirm with SMTP
- [x] Auth: password-reset start+confirm with SMTP
- [x] Billing: Square Checkout + webhook crediting (Stripe compatibility path retained)
- [x] GDPR: `/account/delete` (cascade + webhook scrub) + `/account/export`
- [x] Admin: `/admin/customers`, `/admin/metrics`, `/admin/health`
- [x] Per-IP rate limiting on auth + `/router/complete` w/ XFF behind trusted proxy
- [x] Daemon: BlueyManagedProvider + ManagedPolicy + LocalFallbackPolicy
- [x] Daemon: BalanceWatch poll + Stage 19 cost label plumbing
- [x] CLI product entrypoint: `bluey on` / `bluey off`; `bluey on` opens sign-in when needed
- [x] Support/admin CLI: usage / credits / logout / portal / export / delete-account
- [x] Onboarding: deep-link wizard + tray Invisible toggle + F19 hotkey
- [x] Disguise: 4-mode picker + tray submenu + auto-disguise heuristic + persisted prefs
- [x] Pipeline at tip: fmt + clippy -D + tests + builds across cue + server

### Stage 25: managed streaming + cost metadata + cloud sync + STT auth (post-`9babb20`)

- [x] Synthesized SSE on `/router/complete/stream` (true upstream streaming deferred to v0.2.x)
- [x] Cost + artifact metadata threaded server → cue-llm → daemon → SQLite → overlay
- [x] Native overlay UX: ChatGPT-style chat, side canvas, auto-routing artifact types, opacity capsule, hide/close split
- [x] Server cloud-sync schema (migration 0012) + `/sync/batch`, `/sync/sessions`, `/rag/query`
- [x] STT relay: `/stt/session` + `/stt/relay` (Bluey-scoped session token, server-held provider key, single-claim)
- [x] Customer chunked STT defaults to managed `/router/transcribe` when logged in (no desktop provider key required)
- [x] CLI: `bluey cloud sync/sessions/show/rag`
- [x] Daemon `CloudSyncNow` does real upload (was a stub)

### Security hardening + Pinky leak-review parity (post-`9babb20`)

- [x] Local file/directory permissions: 0700 dirs, 0600 files (best-effort on system-owned parents per B-1 fix)
- [x] Release profile: `strip = "symbols"`, `lto = "thin"`, `codegen-units = 1`
- [x] Overlay helper SHA-256 sidecar verification
- [x] Billing webhook signature regression tests + constant-time match
- [x] Cloud-client log redaction: JSON-aware recursive redactor on tokens, secrets, codes, urls
- [x] Auth verification/reset URLs gated behind `BLUEY_DEV_LOG_AUTH_LINKS=1`
- [x] Deep-link parse failure suppresses raw `bluey://` URL
- [x] Billing upstream error redaction (url/client_secret/payment_method/token)
- [x] `docs/SECURITY-HARDENING.md` reflects managed-cloud auth model + honest "what we cannot make impossible" section
- [x] No "unbacktraceable" / "undetectable" wording in customer-facing copy

### Observability Round (in progress)

- [x] Phase 1: shared `cue-core::observability` (ObserveFields, account_id_hash_prefix, header constants, sanitize_observability_id) — codex `9cd66d4` 🟢 kiro
- [x] Phase 4: `bluey doctor` + `bluey logs export --redact` — kiro `8b9c24a` 🟢 codex
- [x] Phase 4 followup: real macOS permission probes for doctor (Accessibility, Microphone, Screen Recording) — kiro `cef8b77`
- [x] Phase 6: standard field migration sweep (21 account_id → account_id_hash, 10 email drops) — kiro `60ff7fd` 🟢 codex
- [x] Phase 6 tooling: `analyze-tracing-calls.py --check-only` CI gate — kiro `98fe051`
- [ ] Phase 2: daemon + dashboard log rotation (tracing-appender) — codex in flight
- [ ] Phase 3: overlay lifecycle emits + frontend error capture — codex queued
- [ ] Phase 5: trace propagation through Tauri invoke + IPC — codex queued
- [ ] Phase 6 followup: rename `cue-daemon/src/app.rs:6996` `session = %session_id` → `session_id = %session_id` (deferred until codex Phase 2 commits)

### Observability acceptance gate

- [ ] `python3 scripts/analyze-tracing-calls.py --check-only` exits 0 (no transitional / PII / alias findings)
- [ ] One F19 question end-to-end: same `trace_id` appears in daemon, cloud-client, server, provider log lines
- [ ] `bluey doctor` permissions section reports actual probe results (not generic guidance)
- [ ] `bluey logs export --redact` produces a zip; `unzip + grep -i "key|secret|token"` returns ZERO matches in the redacted contents

## Operational infrastructure

### Domains + DNS

- [ ] **`bluey.sh`** registered, A/AAAA records pointed at the bluey-server droplet's public IP
- [ ] `www.bluey.sh` points at the same host (CNAME to `bluey.sh` or A/AAAA to the same IP)
- [ ] DNSSEC enabled
- [ ] CAA records restricting cert issuance to Let's Encrypt
- [ ] Email DNS for `noreply@bluey.sh`: SPF, DKIM, DMARC records published

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

### Square Billing

- [ ] Square production application created and active
- [ ] Square sandbox application created and active
- [ ] `BLUEY_BILLING_PROVIDER=square` on the server
- [ ] Preprod uses `SQUARE_ENVIRONMENT=sandbox`; production uses `SQUARE_ENVIRONMENT=production`
- [ ] Matching `SQUARE_*_APPLICATION_ID`, `SQUARE_*_ACCESS_TOKEN`, and `SQUARE_*_LOCATION_ID` values set in `/etc/bluey-api/bluey-api.env`
- [ ] Webhook endpoint registered: `https://bluey.sh/billing/square/webhook`
- [ ] Webhook events subscribed: `order.updated`, `payment.updated`
- [ ] Matching `SQUARE_*_WEBHOOK_SIGNATURE_KEY` set in `/etc/bluey-api/bluey-api.env`
- [ ] Sandbox test: $30 reload through Square hosted checkout → balance credited within 30s
- [ ] Production test: real $30 reload through Square hosted checkout → balance credited within 30s
- [ ] Auto-topup/card-on-file is explicitly deferred until Square saved-card flow is wired; manual reload must be clear in `/account`
- [x] Customer-facing copy says account credits are non-transferable, have no cash value, and are not a stored-value/gift-card product
- [ ] Refund/support/privacy/terms pages are published before production payments
- [ ] Square credentials rotated if any production credential was pasted into a non-secret channel during setup

### SMTP

- [x] SMTP provider account selected (Resend)
- [x] `BLUEY_SMTP_*` env vars set in `/etc/bluey-api/bluey-api.env`
- [x] Sender domain (`noreply@bluey.sh`) DKIM-signed and verified at provider
- [ ] Live test: `/auth/verify-email/start` → email arrives in <30s, link opens
- [ ] Live test: `/auth/password-reset/start` → email arrives in <30s, link opens
- [x] Server-side smoke: verify/reset endpoints return `202` and log `sent`
- [ ] Bounce handling configured (provider dashboard → forward to ops@bluey.sh)

### Web pages on `bluey.sh`

These can be served from `/var/www/bluey` behind the same Caddy origin as the
API. The static starter in `web/` is enough for early internal testing; these
routes must exist before public alpha:

- [x] `/` — landing page with download button
- [x] `/link` — OAuth-style landing for the `bluey://` deep-link flow (signup/signin form, calls `/auth/link/mint` after auth, redirects browser to `bluey://link?code=...`)
- [x] `/login` — web sign-in alias for the same account shell
- [x] `/reload` — Square Checkout redirect target (after pay → returns to `/account?reload=success`)
- [x] `/account` — user-facing balance + usage + sign-out (calls `/account/me`, `/account/usage`, `/billing/checkout`)
- [x] `/verify-email` — email verification token confirmation page
- [x] `/password-reset` — password-reset request + token-confirmation page
- [x] `/docs/disguise` — explainer page that the dashboard "Why?" link points at
- [x] `/docs/privacy` — alpha privacy policy
- [x] `/docs/terms` — alpha terms of use
- [ ] OG / favicon assets

### macOS terminal distribution (Pinky-style: NO Apple Developer ID required for v0.2 alpha)

The current alpha ships as a terminal bundle: `bluey`, `bluey-daemon`,
and native helper binaries. The installer ad-hoc signs the helper
binaries and strips quarantine. A `.app` bundle can be added later, but
do not advertise it until a real `Bluey.app` artifact exists.

#### Path A: One-line installer (`curl ... | bash`)
- [ ] `ops/install/install.sh` hosted at `https://bluey.sh/install.sh`
- [ ] Release tarball hosted at `https://bluey.sh/releases/v0.1.0/bluey-0.1.0-darwin-arm64.tar.gz`
- [ ] Each release tarball contains top-level `bin/bluey` plus helper binaries
- [ ] `SHA256SUMS.txt` hosted next to the tarballs, and `install.sh` verifies it
- [ ] Smoke on a clean Mac: `curl -fsSL https://bluey.sh/install.sh | bash` finishes cleanly
- [ ] `bluey` CLI is on `$PATH` after install

#### Path B: Homebrew cask (`brew install --cask bluey`)
- [ ] Replace cask with a terminal formula or publish a real `.app` artifact before enabling this path
- [ ] `bluey-dev/homebrew-bluey` GitHub repo created
- [ ] `ops/Casks/bluey.rb` published in that tap
- [ ] `brew tap bluey-dev/bluey` + `brew install --cask bluey` succeeds on a clean Mac
- [ ] Postflight ad-hoc sign + quarantine strip runs cleanly
- [ ] `bluey` CLI is on `$PATH` after install (binary stanza)

#### Optional (v1.0 GA polish, NOT v0.2 gate)

Bluey is terminal-installed (`curl ... | bash`; Homebrew waits for either
a formula or a real `.app` artifact).
Apple Developer ID + notarization are NOT required. The install script
ad-hoc signs the helper binaries and clears quarantine, which is sufficient for
Gatekeeper to allow first launch. If a paid Developer ID becomes
available later, the install path can be upgraded transparently
without breaking existing customers (the bundle id stays the same).

- [ ] (optional) Migrate to signed bundle via Apple Developer Program when convenient — non-blocking for v0.2 alpha

### Monitoring

- [ ] Prometheus or equivalent scraping `/admin/metrics` with admin bearer
- [ ] Alert: `bluey_mark_complete_failures_estimated > 0` for 10+ minutes → page on-call
- [ ] Alert: `bluey_request_idempotency_in_progress > 100` sustained → page
- [ ] Alert: HTTP 5xx rate > 1% over 5 min on Caddy access log → page
- [ ] Alert: SSH login outside maintenance window → notify
- [ ] Uptime monitor (UptimeRobot / BetterStack) hitting `/admin/health` every 60s
- [ ] On-call rotation defined + ack channel (PagerDuty / Slack)

### Legal + compliance

- [x] Terms of Use published at `bluey.sh/docs/terms` (alpha copy; final legal review still needed before broad launch)
- [x] Privacy Policy published at `bluey.sh/docs/privacy` (alpha copy; final legal review still needed before broad launch)
- [x] Account creation/sign-in surface links to both before account creation
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
echo "Open $CHECKOUT_URL in browser, complete with Square sandbox card details..."
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

## Honest gate definition: what "production-ready" actually means

Three product shapes, each with a concrete gate. Use these terms
explicitly when describing the product state — avoid the unqualified
phrase "production-ready," which has burned us before.

### A. Internal-test-build shape (we are HERE at tip post-Phase-6)

- All code-complete items in this checklist marked ✅
- Pipeline GREEN at tip (fmt + clippy -D warnings + tests + builds)
- Internal smoke against test keys / test accounts / test droplet works end-to-end
- Codebase can be tarballed, ad-hoc signed, and distributed to a small
  internal test group without breaking
- **Where we are now**

### B. Closed-alpha shape

- Observability acceptance gate passes (analyzer --check-only,
  end-to-end trace_id, doctor probes, redactor zero-leak)
- Real-Mac smoke (deploy track Phase 2) green on at least 2 macOS
  versions
- Server staging deploy (deploy track Phase 3) green: Square sandbox mode,
  test SMTP, test provider keys
- All operator-side items below ticked OR explicitly deferred with
  rationale
- Closed-list invitation-only access; explicit alpha framing in copy
- **Estimated: 1-2 weeks from current tip**

### C. Paying-customer shape

- Square production keys
- Production SMTP
- `bluey.sh` DNS + production droplet
- Marketing/legal/privacy pages
- Monitoring (Carnaval-equivalent or external)
- Signed release manifest (ed25519 over `latest.json`) for safe auto-update
- Local DB encryption (SQLCipher / envelope) for malware-resistance polish
- Dependency audit CI (`cargo deny` / `cargo audit`)
- **Estimated: 4-6 weeks from current tip**

### D. GA shape

- Postgres + pgvector migration off SQLite
- Multi-region STT relay
- Windows parity (overlay + dashboard + CLI on Windows)
- Certificate pinning
- 24/7 ops rotation
- Customer-success runbook + status page
- **Estimated: ~3 months from current tip**

The mistake we want to avoid: declaring tip A "production-ready"
because the round-N scope closed, then having to rediscover at C
that the product needs more work to ship to paying customers.

## Sign-off

When every checkbox above is ticked, sign here:

- Operator: ____________________ Date: __________
- Product:  ____________________ Date: __________

Once signed: announce. Don't announce before.
