# Bluey Server — Production Deployment Runbook

> **Scope:** Stand up `bluey-server` on a single DigitalOcean droplet (or any single Linux host) behind Caddy with auto-TLS. Single binary + SQLite + reverse proxy.
> **Audience:** Operator with sudo access to the deployment host.
> **Estimated time:** 30-45 minutes from a fresh Ubuntu 24.04 droplet.

## 0. Prerequisites checklist

Before starting, you must already have:

- [ ] The **`bluey.sh` domain** with DNS A/AAAA records pointed at the droplet's public IPv4/IPv6.
- [ ] A **DigitalOcean droplet** (or equivalent) running Ubuntu 24.04 with at least 2 GB RAM, 2 vCPU, 25 GB SSD. SSH key set up.
- [ ] **Square production + sandbox application credentials**, location IDs, and webhook signature keys from the Square dashboard. Keep these in a password manager — never commit.
- [ ] **Upstream provider keys** (OpenAI, Anthropic, Deepgram) for the managed lanes.
- [ ] **Resend API key** for transactional email. Bluey uses the Resend HTTPS
  API path because many cloud hosts block outbound SMTP ports.
- [ ] A **64-character JWT secret** generated via `openssl rand -hex 32`.
- [ ] (Optional) **Backup destination** — S3-compatible bucket or off-host SFTP target.

## 1. One-time host setup

```bash
# As root on the fresh droplet.
apt-get update && apt-get -y upgrade
apt-get -y install ufw curl ca-certificates rsync sqlite3 jq gnupg

# Firewall: only 22 (SSH), 80, 443 open.
ufw default deny incoming
ufw default allow outgoing
ufw allow 22/tcp
ufw allow 80/tcp
ufw allow 443/tcp
ufw --force enable

# Dedicated unprivileged user for the daemon.
useradd --system --create-home --home-dir /opt/bluey-api --shell /usr/sbin/nologin bluey
mkdir -p /opt/bluey-api /var/log/bluey-api /var/backups/bluey-api
chown -R bluey:bluey /opt/bluey-api /var/log/bluey-api /var/backups/bluey-api
```

## 2. Install Caddy (auto-TLS via Let's Encrypt)

```bash
apt-get -y install debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | tee /etc/apt/sources.list.d/caddy-stable.list
apt-get update && apt-get -y install caddy
```

Caddy auto-renews certificates from Let's Encrypt; no cron needed.

## 3. Drop in the Caddyfile

Copy `ops/Caddyfile.example` from this repo to `/etc/caddy/Caddyfile`, then:

```bash
caddy validate --config /etc/caddy/Caddyfile
systemctl reload caddy
```

## 4. Build + install the server binary

On a build host (your laptop or a CI runner) with the rust toolchain:

```bash
cd /path/to/cue/server
cargo build --release
scp target/release/bluey-server root@<droplet>:/usr/local/bin/bluey-server
ssh root@<droplet> 'chmod 755 /usr/local/bin/bluey-server && chown root:root /usr/local/bin/bluey-server'
```

(Future: replace with a CI-built artifact + `apt`-installable package.)

## 5. Configure environment

Create `/etc/bluey-api/bluey-api.env` with mode 0640 owned by `root:bluey`.
The service runs as the `bluey` group, so group-read is required:

```ini
# Required
BLUEY_PORT=8080
BLUEY_DB_PATH=/opt/bluey-api/bluey.db
BLUEY_JWT_SECRET=<openssl rand -hex 32 output>
BLUEY_PUBLIC_URL=https://bluey.sh

# Square billing. Preprod uses SQUARE_ENVIRONMENT=sandbox; prod uses production.
BLUEY_BILLING_PROVIDER=square
SQUARE_ENVIRONMENT=production
SQUARE_PRODUCTION_APPLICATION_ID=sq0idp_xxxxxxxx
SQUARE_PRODUCTION_ACCESS_TOKEN=EAAA_xxxxxxxx
SQUARE_PRODUCTION_LOCATION_ID=<Square production location id>
SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY=<Square production webhook signature key>

# Optional: keep sandbox values on the host so a preprod env file can switch
# by changing only SQUARE_ENVIRONMENT=sandbox.
SQUARE_SANDBOX_APPLICATION_ID=sandbox-sq0idb_xxxxxxxx
SQUARE_SANDBOX_ACCESS_TOKEN=EAAA_sandbox_xxxxxxxx
SQUARE_SANDBOX_LOCATION_ID=<Square sandbox location id>
SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY=<Square sandbox webhook signature key>

# Upstream providers (Bluey owns these; customers pay Bluey)
OPENAI_API_KEY=sk-xxxxxxxx
ANTHROPIC_API_KEY=sk-ant-xxxxxxxx
DEEPGRAM_API_KEY=xxxxxxxx

# Transactional mail for verify + reset emails. Resend is sent over HTTPS
# by bluey-server because many cloud hosts block outbound SMTP ports.
BLUEY_SMTP_HOST=smtp.resend.com
BLUEY_SMTP_PORT=587
BLUEY_SMTP_USERNAME=resend
BLUEY_SMTP_PASSWORD=<resend api key>
BLUEY_SMTP_FROM=Bluey <hello@bluey.sh>
BLUEY_SMTP_STARTTLS=true

# Rate-limit XFF trust — when behind Caddy on the same host this is loopback.
BLUEY_TRUSTED_PROXIES=127.0.0.1,::1
```

```bash
chmod 0640 /etc/bluey-api/bluey-api.env
chown root:bluey /etc/bluey-api/bluey-api.env
```

## 6. Install the systemd unit

Copy `ops/bluey-api.service.example` from this repo to `/etc/systemd/system/bluey-api.service`, then:

```bash
systemctl daemon-reload
systemctl enable --now bluey-api.service
systemctl status bluey-api.service
journalctl -u bluey-api.service --since "5 min ago"
```

## 7. Verify

```bash
curl -fsS https://bluey.sh/admin/health
# {"status":"ok","version":"...","commit":"..."}

curl -fsS https://bluey.sh/pricing/tiers | jq .
# {"reload_amount_cents":3000,"minimum_cue_cents":1,"tiers":[...]}
```

If both succeed, the server is reachable, TLS is live, and the public router responds.

## 8. Configure Square webhook

In the Square dashboard → Developer → Webhooks → Add subscription:

- **URL:** `https://bluey.sh/billing/square/webhook`
- **Events:** at minimum `order.updated`.
- After saving, copy the **Signature key** and update `SQUARE_PRODUCTION_WEBHOOK_SIGNATURE_KEY` in `/etc/bluey-api/bluey-api.env`. `systemctl restart bluey-api.service`.

Run a Square sandbox checkout and verify `journalctl -u bluey-api.service` shows the `order.updated` event processed and the account balance credited.

## 9. Backups

Install the backup script + cron:

```bash
cp ops/backup-bluey-db.sh /usr/local/sbin/backup-bluey-db.sh
chmod 750 /usr/local/sbin/backup-bluey-db.sh
chown root:root /usr/local/sbin/backup-bluey-db.sh

# Cron entry: hourly snapshots; script keeps 14 hourly + 14 daily snapshots.
cat > /etc/cron.d/bluey-api-backup <<'EOF'
0 * * * * root /usr/local/sbin/backup-bluey-db.sh
EOF
```

The script lives at `ops/backup-bluey-db.sh` in this repo. It uses SQLite's online backup API (safe with the daemon running) and rotates the last 14 hourly + 14 daily snapshots locally; off-host shipping to S3/SFTP is configured by editing the `OFFSITE_DESTINATION` variable.

## 10. Monitoring

The server exposes `/admin/metrics` in Prometheus exposition format (admin-only). To scrape:

1. Create an admin account: `curl -X POST https://bluey.sh/auth/signup ...`, then `UPDATE accounts SET is_admin=1 WHERE email='ops@bluey.sh';` directly in the DB (or a future `bluey ops promote` CLI).
2. Mint a long-lived bearer for monitoring; store in your Prometheus auth config.
3. Scrape with `Authorization: Bearer ...` header.

Key metrics to alert on:
- `bluey_mark_complete_failures_estimated` — proxy for billed-but-uncached requests. Alert if >0 sustained 10 minutes.
- `bluey_request_idempotency_in_progress` — alert if growing without bound (deadlocked requests).
- `bluey_balance_cents_sum` — sanity check; sudden negative deltas suggest a billing bug.
- HTTP 5xx rate on Caddy access logs.

## 11. Rolling out a new server version

```bash
# Build the new binary on your build host.
cd cue/server && cargo build --release

# Push.
scp target/release/bluey-server root@<droplet>:/usr/local/bin/bluey-server.new
ssh root@<droplet> '
  set -e
  chmod 755 /usr/local/bin/bluey-server.new
  mkdir -p /var/backups/bluey-api/bin
  if [ -x /usr/local/bin/bluey-server ]; then
    cp -f /usr/local/bin/bluey-server /var/backups/bluey-api/bin/bluey-server.previous
  fi
  mv /usr/local/bin/bluey-server.new /usr/local/bin/bluey-server
  chown root:root /usr/local/bin/bluey-server
  systemctl restart bluey-api.service
  sleep 2
  curl -fsS https://bluey.sh/admin/health
'
```

If `/admin/health` fails, roll back:

```bash
ssh root@<droplet> 'cp /var/backups/bluey-api/bin/bluey-server.previous /usr/local/bin/bluey-server && systemctl restart bluey-api.service'
```

The rollout command copies the old binary to `/var/backups/bluey-api/bin/bluey-server.previous` before replacing it. Keep that step before the `mv`; doing it from `ExecStartPre` would copy the newly deployed binary and make rollback useless.

## 12. Pre-launch sign-off checklist

Before flipping DNS or announcing the product:

- [ ] `/admin/health` returns 200 over HTTPS with a valid TLS cert.
- [ ] Square webhook fires successfully on a sandbox purchase first, then on a real production purchase.
- [ ] SMTP emails arrive in <30 seconds for both `/auth/verify-email/start` and `/auth/password-reset/start`.
- [ ] `/admin/metrics` is reachable with a bearer + the metrics look sane (accounts >= 1, no in_progress > 0).
- [ ] Backup script runs successfully via `sudo -u root /usr/local/sbin/backup-bluey-db.sh` and produces a file in `/var/backups/bluey-api/`.
- [ ] Off-host backup destination receives the snapshot.
- [ ] At least one full money-path smoke: signup → trial → reload via Square → cue dispatch → balance debited → cue response.
- [ ] First `bluey on` from a clean Mac opens browser sign-in and successfully completes the deep-link flow against the production server.
- [ ] systemd unit restarts cleanly on `systemctl restart bluey-api.service` (no orphan PIDs).
- [ ] Caddy auto-TLS renewal log entries visible (`journalctl -u caddy --since "1 hour ago" | grep -i renew`).

## 13. Disaster recovery

If the droplet is destroyed:

1. Provision a new droplet (any region with the same Ubuntu version).
2. Re-run sections 1, 2, 4, 5, 6.
3. Restore the most recent backup: `cp /tmp/<latest-snapshot>.db /opt/bluey-api/bluey.db && chown bluey:bluey /opt/bluey-api/bluey.db`.
4. Repoint DNS A/AAAA records.
5. Verify section 7.

RPO is 1 hour (cron interval). RTO is roughly the time to provision + restore = ~15 minutes if you have the backup handy.

## 14. Things explicitly NOT in this runbook

- **Multi-host / load-balanced deployment.** Today's binary uses in-memory rate-limit state; horizontal scaling needs a Redis-backed limiter swap. Single-host is the supported v0.2 shape.
- **Database replication.** SQLite + hourly backup is the v0.2 RPO. PostgreSQL migration is queued for v0.3 if multi-region matters.
- **Auto-update server for the macOS app.** A separate distribution server (`R14.8` in `FUTURE-IMPLEMENTATIONS.md`) hosts the signed `.dmg` + `latest.json`.
- **Full web app polish on `bluey.sh`.** The same origin should host landing,
  install, link, reload, account, and docs pages. This repo includes the API
  and static landing starter; production page polish can stay in a separate
  web codebase as long as it publishes into `/var/www/bluey`.
