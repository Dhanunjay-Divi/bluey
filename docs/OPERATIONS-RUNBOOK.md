# Bluey Operations Runbook

> **Codex preflight:** Load `$bluey-ops` from
> the branch's `.agents/skills/bluey-ops/SKILL.md`, inspect current repository and
> production state, and treat this runbook as authority where memory differs.
> Record ownership and remaining tasks using [the round checklist](work/BLUEY-AGENT-ROUND-CHECKLIST.md).

> **Secrets, server layout, smoke tests, daily ops.**
> Mirrors Pinky's `OPERATIONS-RUNBOOK.md` shape but Bluey-only.
>
> Last updated: 2026-07-02.

This is the ops playbook. If something is on fire, this is the doc to
reach for.

---

## 1. Production state

| Layer | Status | Where |
|---|---|---|
| Layer 1 (desktop client) | production beta, signed auto-update | end-user Macs; Windows parity where built |
| Layer 2 (distribution) | live | `https://bluey.sh`, static installers, signed `latest.json`, versioned release artifacts |
| Layer 3 (product/API server) | live production beta | `https://bluey.sh` API/admin/billing/auth paths on the Bluey API host |
| Layer 4 (storage) | production beta | primary DB, hourly backups, optional private R2 object/release mirrors |

Production deploys must follow [`docs/RELEASE-RUNBOOK.md`](./RELEASE-RUNBOOK.md).
Do not manually patch production except for owner-approved emergency hotfixes,
and record every exception in a numbered round doc.

---

## 2. Secrets inventory (target — none in production yet)

| Secret | Where it lives | Used by |
|---|---|---|
| User's Anthropic API key | developer/BYOK keyring path only | local daemon, `cue-llm` dev mode |
| User's OpenAI API key | developer/BYOK keyring path only | local daemon, `cue-llm` dev mode |
| Bluey account access/refresh token | private local account profile by default | desktop cloud auth |
| `BLUEY_OVERLAY_SESSION_TOKEN` | env var, generated per-spawn | overlay IPC handshake |
| `BLUEY_LOCAL_ONLY` | env var, opt-in | force Auto Router to Local lane |
| `BLUEY_SPECULATIVE_ROUTING` | env var, default ON | toggle speculative draft+final |
| Stripe API keys (future, Layer 3) | server env file `/opt/bluey-api/env` | bluey-server |
| `BLUEY_JWT_SECRET` (future, Layer 3) | server env file | bluey-server auth |
| `BLUEY_SMTP_HOST` / `BLUEY_SMTP_PORT` | server env file | verification + password reset email |
| `BLUEY_SMTP_USERNAME` / `BLUEY_SMTP_PASSWORD` | server env file | SMTP auth |
| `BLUEY_SMTP_FROM` / `BLUEY_SMTP_STARTTLS` | server env file | SMTP sender + transport mode |

**Rule:** never commit secret values to this repo. The `secrets`
module reads developer keys from keyring for client-side dev paths and env files
for server-side provider keys.
Only the names of secrets appear in code or docs.

---

## 3. Smoke tests

### Release artifact integrity

Run after every desktop release publish:

```bash
BLUEY_RELEASE_PUBKEY_FILE=/secure/off-repo/bluey-release-ed25519.pub.pem \
  scripts/bluey-release-live-verify.sh <version>
```

If only the signing key is available to the release operator:

```bash
BLUEY_RELEASE_SIGNING_KEY_FILE=/secure/off-repo/bluey-release-ed25519.pem \
  scripts/bluey-release-live-verify.sh <version>
```

### Client (run on uno or any test Mac)

```bash
cd /Users/uno/Downloads/cue
bash scripts/smoke-test.sh
# expected: "Bluey smoke test passed: daemon, overlay, transcript,
#            instructions, context, memory, audio scaffold, AI routing
#            scaffold, cloud scaffold, ask, action-items, recap, and
#            archive all worked."
```

### Installed-path (after `scripts/install.sh`)

```bash
tmp=$(mktemp -d)
BLUEY_ARCHIVE=dist/bluey-0.1.0-darwin-universal.tar.gz \
  BLUEY_INSTALL_DIR="$tmp/bluey" \
  BLUEY_BIN_DIR="$tmp/bin" \
  bash scripts/install.sh
"$tmp/bin/bluey" on
sleep 2
pgrep -fl "bluey-daemon|bluey-overlay" | head
"$tmp/bin/bluey" off
sleep 1
pgrep -fl "bluey-daemon|bluey-overlay" || echo "(none — clean stop)"
rm -rf "$tmp"
```

### Cue request end-to-end (with provider keys configured)

```bash
bluey on --title "smoke"
bluey listen --speaker user "what is the meaning of life?"
bluey ask "answer my question"
# expect a streaming response in the overlay + a cue_response_chunk event
bluey off
```

---

## 4. Restart / inspect / kill

### Local daemon stuck

```bash
# Find:
pgrep -fl "bluey-daemon|bluey-overlay"

# Stop cleanly:
bluey off

# Force-kill if `bluey off` hangs:
pkill -f bluey-daemon
pkill -f bluey-overlay-macos

# Inspect state file:
cat ~/.local/share/bluey/state.json   # macOS
# OR
cat ~/Library/Application\ Support/bluey/state.json
```

### Overlay shows but pill is unresponsive

1. Check `pgrep -fl bluey-overlay-macos` — if there are multiple, kill
   the stale ones.
2. `bluey off; bluey on` resets state.
3. If pill never appears after `bluey on`: check the daemon log under
   `~/Library/Application Support/bluey/logs/`. The daemon emits
   `discover_overlay_bin` errors clearly.

### Distribution server stuck

```bash
ssh <distribution-host>
curl -fsS https://bluey.sh/health
curl -fsS https://bluey.sh/latest.json
curl -fsSI https://bluey.sh/install.sh
curl -fsSI https://bluey.sh/install.ps1
sudo systemctl status caddy
sudo journalctl -u caddy --since "15 min ago" --no-pager
```

Do not edit `latest.json` by hand. Re-publish the last known good signed
release or promote a verified stored artifact.

### API server stuck

```bash
ssh <api-host>
sudo systemctl status bluey-api.service
sudo journalctl -u bluey-api.service --since "15 min ago" --no-pager
curl -fsS https://bluey.sh/admin/health
```

Before restarts that may affect billing, run or confirm backup health. For disk
or storage symptoms, use
[`docs/ops/DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md`](./ops/DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md).
For refund/dispute/account-delete billing symptoms, use
[`docs/ops/DISPUTE-EVIDENCE-RUNBOOK.md`](./ops/DISPUTE-EVIDENCE-RUNBOOK.md).

---

## 5. Common failure modes

| Symptom | Likely cause | Fix |
|---|---|---|
| `bluey on` hangs | overlay binary missing from install dir | reinstall via `scripts/install.sh` |
| `cue_response` never arrives | no LLM provider configured | `bluey set-stt-api-key openai sk-...` (or env var) |
| Pill appears but feed never opens on click | overlay process old, stale build | `bluey off`, rebuild, reinstall |
| `Refused to spawn overlay` in logs | binary verification failed | check `BLUEY_OVERLAY_BIN` env, see `crates/cue-daemon/src/overlay.rs::verify_overlay_binary` |
| Speculative dispatch never fires | env var disables it OR no providers configured | `BLUEY_SPECULATIVE_ROUTING=1` (default) + at least one provider key |
| `curl https://bluey.sh/install.sh | bash` returns HTML | artifact path unreadable or Caddy fell through to SPA | run `scripts/bluey-release-live-verify.sh <version>`, fix file permissions, republish signed release |
| Balance drops unexpectedly | stale UI balance, duplicated STT/listen rows, or billing settlement lag | inspect usage/ledger rows, session id, and use `docs/ops/DISPUTE-EVIDENCE-RUNBOOK.md` if this is a money-path complaint |

---

## 6. Building from source on a fresh Mac

```bash
# 1. Clone
git clone <repo-url> bluey
cd bluey

# 2. Toolchain
xcode-select --install   # if not present
brew install rustup-init && rustup-init -y
rustup target add aarch64-apple-darwin x86_64-apple-darwin
brew install node       # for the dashboard UI

# 3. Build
make package-darwin-arm64       # arm64-only
make package-darwin-universal   # arm64 + x86_64 lipo

# 4. Smoke
bash scripts/smoke-test.sh
```

First build is slow (~3-5 min). Subsequent builds use the cargo cache.

---

## 7. Where to look when something is wrong

1. `docs/PRODUCTION-READINESS.md` — what's actually shipping.
2. `DECISIONS.md` — historical decisions; the bug may be deliberate.
3. `docs/rounds/PHASE-3-ROUND-N-PLAN.md` — current-round work in progress.
4. `~/Library/Application Support/bluey/logs/` — daemon logs.
5. `docs/rounds/BLUEY-COMPACTION-HANDOFF-2026-06-25.md` — current long-running Codex handoff.
6. `docs/ops/DEPLOY-DISK-STORAGE-CHECK-RUNBOOK.md` — deploy/storage incidents.
7. `docs/ops/DISPUTE-EVIDENCE-RUNBOOK.md` — refunds, disputes, chargebacks, unexpected credit loss.
