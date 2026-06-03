# Phase 3: bluey-server Test Deploy + Square/Provider-Key Staging

> **Status:** durable doc; promoted from /tmp staging when the
> Observability Round was at 5/6 phases done. Use this as the operator
> playbook for the staging server deploy.


**Trigger:** Phase 2 (Mac smoke) all-green.
**Owner:** operator (kiro can guide remotely).
**Time budget:** ~90 minutes for deploy + ~30 minutes for Square/provider staging smoke.

This is the FIRST production-shaped deploy of `bluey-server`. The host is a
DigitalOcean droplet. Everything below is per
`docs/PRODUCTION-DEPLOY-RUNBOOK.md`; this is the abridged, ordered version.

---

## 3.1 Provision the test droplet

Use a DROPLET tier appropriate for test traffic:
- **Region:** SFO3 / NYC1 (closest to your test-Mac latency-wise)
- **Image:** Ubuntu 24.04 LTS x86_64
- **Size:** s-1vcpu-1gb ($6/mo) is fine for test; bump for real traffic
- **Auth:** SSH key only, no password
- **Hostname:** `bluey-test` or `bluey-staging`

```bash
# After droplet boots, from your Mac:
ssh root@<droplet_ip>
adduser bluey                      # create non-root user
usermod -aG sudo bluey
mkdir -p /home/bluey/.ssh
cp ~/.ssh/authorized_keys /home/bluey/.ssh/
chown -R bluey:bluey /home/bluey/.ssh
chmod 700 /home/bluey/.ssh && chmod 600 /home/bluey/.ssh/authorized_keys

# Lock root SSH:
sed -i 's/^PermitRootLogin .*/PermitRootLogin no/' /etc/ssh/sshd_config
systemctl restart sshd
exit

# Re-login as bluey:
ssh bluey@<droplet_ip>
```

---

## 3.2 Install Caddy + sqlite + minimal toolchain

```bash
sudo apt update && sudo apt upgrade -y
sudo apt install -y curl ca-certificates sqlite3 jq

# Caddy 2 from official repo:
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update && sudo apt install -y caddy
```

Verify Caddy is up:
```bash
sudo systemctl status caddy
curl -I http://<droplet_ip>  # should hit Caddy default
```

---

## 3.3 Build + ship the bluey-server binary

On uno (Mac), cross-compile for Linux:
```bash
cd /Users/uno/Downloads/cue/server

# Install cross if not already:
cargo install cross --git https://github.com/cross-rs/cross

# Build for x86_64-unknown-linux-gnu (statically-linked-ish musl is also fine):
cross build --release --target x86_64-unknown-linux-gnu

# Resulting binary:
ls -la target/x86_64-unknown-linux-gnu/release/bluey-server
```

SCP to droplet:
```bash
scp target/x86_64-unknown-linux-gnu/release/bluey-server bluey@<droplet_ip>:/tmp/bluey-server
```

On droplet:
```bash
sudo mkdir -p /opt/bluey
sudo mv /tmp/bluey-server /opt/bluey/bluey-server
sudo chmod 755 /opt/bluey/bluey-server
sudo chown root:root /opt/bluey/bluey-server
```

---

## 3.4 Provision env file + secrets

```bash
sudo mkdir -p /etc/bluey /var/lib/bluey /var/log/bluey
sudo chown bluey:bluey /var/lib/bluey /var/log/bluey
sudo chmod 700 /var/lib/bluey

sudo nano /etc/bluey/bluey-server.env
```

Contents (replace placeholders):
```
# ── Bind ─────────────────────────────────────────────────────────
BLUEY_BIND=127.0.0.1:8081

# ── Database ────────────────────────────────────────────────────
BLUEY_DB_URL=/var/lib/bluey/bluey.db
BLUEY_DB_MIGRATE=true

# ── Auth (CHANGE THESE!) ────────────────────────────────────────
BLUEY_JWT_SECRET=<generate via: openssl rand -hex 64>
BLUEY_PUBLIC_URL=https://api-test.bluey.dev

# ── Square SANDBOX mode ────────────────────────────────────────
BLUEY_BILLING_PROVIDER=square
SQUARE_ENVIRONMENT=sandbox
SQUARE_SANDBOX_APPLICATION_ID=sandbox-sq0idb_xxxxxxxxx
SQUARE_SANDBOX_ACCESS_TOKEN=EAAA_sandbox_xxxxxxxxx
SQUARE_SANDBOX_LOCATION_ID=<Square sandbox location id>
SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY=<Square sandbox webhook signature key>

# ── Upstream provider keys (TEST/STAGING ACCOUNTS) ─────────────
BLUEY_OPENAI_API_KEY=sk-test-xxxxxxxxxxxxxx
BLUEY_ANTHROPIC_API_KEY=sk-ant-test-xxxxxxxxxxxxxx
BLUEY_DEEPGRAM_API_KEY=test_xxxxxxxxxxxxxx

# ── SMTP (use Mailhog or test SMTP for staging) ─────────────────
BLUEY_SMTP_URL=smtp://user:pass@smtp.test:587
BLUEY_SMTP_FROM=hello@api-test.bluey.dev

# ── Pricing markup (test values) ────────────────────────────────
BLUEY_MARKUP_PCT=20
BLUEY_FREE_TRIAL_SECONDS=300

# ── Logging ─────────────────────────────────────────────────────
RUST_LOG=info,bluey_server=debug,tower_http=info
```

Lock down:
```bash
sudo chmod 600 /etc/bluey/bluey-server.env
sudo chown root:root /etc/bluey/bluey-server.env
```

---

## 3.5 systemd unit

Copy from `ops/bluey-api.service.example`:
```bash
sudo cp /Users/uno/Downloads/cue/ops/bluey-api.service.example /etc/systemd/system/bluey-api.service
# Or scp it from uno:
# scp /Users/uno/Downloads/cue/ops/bluey-api.service.example bluey@<droplet_ip>:/tmp/
# sudo mv /tmp/bluey-api.service.example /etc/systemd/system/bluey-api.service

sudo nano /etc/systemd/system/bluey-api.service  # adjust paths if needed
sudo systemctl daemon-reload
sudo systemctl enable bluey-api
sudo systemctl start bluey-api
sudo systemctl status bluey-api
```

Tail logs:
```bash
sudo journalctl -u bluey-api -f
```

Should see "listening on 127.0.0.1:8081" and migration completion.

---

## 3.6 Caddy reverse proxy

Copy `ops/Caddyfile.example`:
```bash
sudo cp /etc/caddy/Caddyfile /etc/caddy/Caddyfile.bak
sudo nano /etc/caddy/Caddyfile
```

Replace contents with the example, edit:
- Domain: `api-test.bluey.dev` (you need DNS A record pointing here BEFORE Caddy can get LetsEncrypt cert)
- Backend: `127.0.0.1:8081`

```bash
sudo caddy validate --config /etc/caddy/Caddyfile
sudo systemctl reload caddy
```

---

## 3.7 DNS

Set:
```
api-test.bluey.dev  A   <droplet_ip>   TTL 300
```

Wait for propagation (~1-5 min), then:
```bash
dig api-test.bluey.dev +short  # should return <droplet_ip>
curl -I https://api-test.bluey.dev/health  # should 200 with Caddy serving cert
```

---

## 3.8 Smoke from uno

```bash
# On uno, create a test account against the staging server:
export BLUEY_CLOUD_API_URL=https://api-test.bluey.dev
bluey on  # opens sign-in against staging if no token is present

# Verify endpoints:
bluey usage   # returns balance (0 minus free trial)
curl -X POST https://api-test.bluey.dev/auth/register \
    -H "Content-Type: application/json" \
    -d '{"email":"smoke@kiro.test","password":"xxxxxxxx"}'
```

---

## 3.9 Square sandbox webhook smoke

```bash
# Register this endpoint in the Square sandbox app dashboard:
# https://api-test.bluey.dev/billing/square/webhook
# Subscribe at minimum to order.updated, then paste the signature key
# into SQUARE_SANDBOX_WEBHOOK_SIGNATURE_KEY.

# On droplet, watch logs:
sudo journalctl -u bluey-api -f | grep -i square
# Complete a sandbox checkout and confirm balance crediting.
```

---

## 3.10 STT relay smoke

```bash
# On uno:
bluey on                              # starts session and opens sign-in against staging if needed
# Click "Start Bluey", speak for ~30s, click Stop
# Watch droplet logs:
sudo journalctl -u bluey-api -f | grep -E "stt|/router/transcribe"

# Verify on droplet:
sqlite3 /var/lib/bluey/bluey.db "SELECT account_id, provider, model, consumed_seconds, ended_at_ms FROM stt_sessions ORDER BY created_at_ms DESC LIMIT 5;"
```

Should show your session with `consumed_seconds > 0` and `ended_at_ms` set.

---

## 3.11 Provider-key staging acceptance

Endpoints to validate against staging:
- `POST /auth/register` + email verification (uses staging SMTP)
- `POST /auth/login` → access + refresh
- `POST /router/complete` → OpenAI staging key
- `POST /router/complete/stream` → OpenAI staging key (synthesized SSE)
- `POST /router/transcribe` → Deepgram staging key, OpenAI fallback key
- `POST /sync/batch` → roundtrip
- `POST /rag/query` → roundtrip
- `POST /stt/session` + `WS /stt/relay` → Deepgram WS proxy

A quick smoke runner script is at `scripts/smoke-staging.sh` (write if not present; should hit every endpoint above with curl + asserts).

---

## 3.12 Observability for staging (read-only quick wins)

While the observability round (queued in `docs/rounds/OBSERVABILITY-ROUND-PLAN.md`)
is not yet implemented, you can get partial visibility now:

```bash
# Tail server logs:
sudo journalctl -u bluey-api -f

# Filter to errors:
sudo journalctl -u bluey-api -p err -f

# Tail Caddy access logs:
sudo tail -f /var/log/caddy/access.log

# Quick rate-of-error metric:
sudo journalctl -u bluey-api --since "1 hour ago" | grep -c "ERROR"
```

---

## 3.13 Backups

Copy + run the backup script:
```bash
sudo cp /Users/uno/Downloads/cue/ops/backup-bluey-db.sh /usr/local/bin/
sudo chmod 755 /usr/local/bin/backup-bluey-db.sh

# Test backup:
sudo /usr/local/bin/backup-bluey-db.sh
ls -la /var/backups/bluey/

# Schedule hourly + daily:
sudo nano /etc/cron.d/bluey-backup
```

Cron contents:
```
0 * * * * root /usr/local/bin/backup-bluey-db.sh hourly
0 3 * * * root /usr/local/bin/backup-bluey-db.sh daily
```

---

## 3.14 Pass criteria

Phase 3 is GREEN when:

- [ ] HTTPS to `https://api-test.bluey.dev/health` returns 200
- [ ] First `bluey on` against staging succeeds with magic-link or device flow
- [ ] `bluey usage` shows the trial balance
- [ ] An LLM completion via `/router/complete/stream` succeeds with cost metadata
- [ ] A Deepgram STT relay session via `/stt/session` + `/stt/relay` records `consumed_seconds`
- [ ] A Square sandbox `order.updated` event applies reload credit to balance
- [ ] `/sync/batch` round-trips: upload local data, list cloud sessions, get one back
- [ ] `/rag/query` returns chunks
- [ ] systemd restarts the server cleanly: `sudo systemctl restart bluey-api && sudo systemctl status bluey-api`
- [ ] Backup script runs without error and produces a `.backup` file

---

## What stays out of Phase 3

- Production DNS for `bluey.dev` (use `api-test.bluey.dev` only)
- Square production keys (sandbox only in preprod)
- Production SMTP (Mailhog or test smtp is fine for staging)
- Carnaval / monitoring dashboards
- Postgres migration (SQLite is fine for staging)
- Multi-region relay
- Public install script hosting (`bluey.dev/install.sh`)

These are Phase 4 (production) gates. Phase 3 = "the server works end-to-end against test keys/accounts/providers".
