# Bluey First 100 Paid Users Plan

This plan narrows the launch target to the first 100 paid users without
changing the long-term architecture. The goal is to run a controlled paid
alpha on the simplest production-shaped stack, prove the money/caption/answer
path, and add scale components only when measured pressure requires them.

## Verdict

One DigitalOcean droplet is enough for the first 100 paid users if those users
are part of a controlled alpha and we monitor the real bottlenecks. It is not
enough to promise ultra-low latency worldwide.

For the first 100 users, provider latency and provider limits will usually
matter more than the droplet. For worldwide low-latency usage, the same
architecture can grow into regional API/STT relays, Cloudflare-fronted static
assets, shared provider capacity state, and Postgres/pgvector without changing
the desktop contract.

## Target Scope

- 100 paid users total.
- Expected active concurrency: 5-15 users.
- Burst concurrency to plan for: 20-30 active sessions.
- Supported client for the first paid alpha: macOS terminal-installed Bluey,
  unless `docs/deploy/WINDOWS-PAID-ALPHA-READINESS.md` passes its P0 gate on a
  clean Windows 10/11 machine.
- Billing: manual Square credit reloads.
- Provider keys: server-side only. No customer provider keys in release builds.
- User laptop installs only the Bluey desktop client, helper binaries, local
  cache, and local session/RAG files. Users do not install Redis, Postgres,
  Docker, pgvector, or any cloud infrastructure.
- Windows, when enabled, uses the same cloud and billing architecture. It does
  not get a separate backend.

## First 100 Architecture

```text
Mac desktop
  - overlay / pill UI
  - local transcript/session store
  - local document text cache and local RAG index
  - no provider secrets
        |
        | HTTPS / WebSocket
        v
bluey.sh on one DigitalOcean droplet
  - Caddy TLS + static web/install routes
  - bluey-server
  - SQLite app database
  - Square checkout + webhooks
  - SMTP
  - managed provider routing
        |
        v
Providers
  - OpenAI / Anthropic / Gemini as configured for answers and vision
  - Deepgram for live STT
  - R2/S3-compatible storage for releases, backups, support zips, and synced artifacts
```

The desktop keeps latency-sensitive local UX local: overlay rendering,
session navigation, local transcript history, and local retrieval against the
on-device index. Paid provider work goes through Bluey server so provider keys,
model choices, rate/capacity policy, and billing stay controlled.

## What Is Not Needed For 100 Users

- Kubernetes.
- Redis or Valkey.
- Postgres.
- pgvector in the cloud.
- Multi-region API servers.
- Customer BYOK or local LLM release mode.
- A separate `api.bluey.sh` host.
- Windows public support before the Windows paid-alpha readiness gate passes.

These are upgrade steps, not launch blockers.

## Latency Reality

One droplet gives good controlled-alpha latency, not global ultra-low latency.

- US users near the droplet should feel fine.
- Europe/India/Asia users add network round trips to the droplet before the
  provider request starts.
- LLM first-token latency is usually dominated by the selected provider/model,
  route fallback, and prompt/context size.
- Live captions are more sensitive because audio packets travel desktop ->
  Bluey server -> STT provider -> Bluey server -> desktop.

For first 100 paid users, pick the droplet region nearest the initial customer
set and the provider regions we use most. If the first users are mostly US, a
US-East droplet is acceptable.

## Required Gates Before Inviting The First 100

- `docs/deploy/PAID-ALPHA-SMOKE.md` passes on a clean Mac with a real linked
  account and real credits.
- Square sandbox and one low-dollar production reload credit the account
  balance within 30 seconds.
- Square failed-webhook/dispute notification mailbox is monitored.
- Provider accounts are funded and have dashboard alerts/caps where available.
- Deepgram live captions work with real microphone and system audio.
- OpenAI/Anthropic/Gemini answer and vision routes are live-smoked as configured.
- Signed `latest.json` and `latest.json.sig` are published and verified by the
  installer/update path.
- `curl -fsSL https://bluey.sh/install.sh | bash` installs a working CLI on a
  clean Mac.
- Normal `bluey on` has no mock transcript and no dev overlay flags.
- Off-host backups are configured and one restore/checksum verification has
  passed.
- Logs/support zip include enough trace IDs to debug no-caption, no-answer, and
  no-credit incidents.
- Abuse, refund, and chargeback playbook has an assigned human owner.

## Operational Limits For The First 100

- Invite manually or use a small allowlist.
- Keep manual reloads first; saved-card Auto Reload is opt-in only after a
  successful Square card-save flow.
- Keep first reload amount simple, currently `$15`.
- Review spend, failed provider calls, refunds, and disputes daily.
- Keep the Bluey-side upstream spend guard enabled while real usage patterns
  are unknown.
- Keep auto-update in signed check-and-notify mode until rollback confidence is
  proven.
- Do not expose provider keys, BYOK flags, local LLM modes, or dev capture flags
  in release artifacts.

## Upgrade Triggers

These are the points where we grow the stack without changing the product
contract.

- **More CPU or memory:** p95 API latency rises while provider latency is normal,
  or the droplet is consistently above 70% CPU or memory.
  Upgrade the droplet first.
- **SQLite contention:** sustained database lock waits, DB size over 5-10 GB, or
  admin/usage queries slow down.
  Move server state to managed Postgres.
- **Provider capacity across more than one server:** more than one bluey-server
  instance needs shared key health/cooldown state.
  Add Redis/Valkey for provider capacity state.
- **Cloud memory/search grows:** users expect cross-device/global memory search,
  or synced artifact retrieval becomes heavy.
  Add Postgres + pgvector for cloud memory while keeping desktop local RAG.
- **Worldwide latency complaints:** users outside the droplet region see slow
  captions or slow answer starts.
  Put static/release assets behind Cloudflare/R2 and add regional API/STT relays.
- **Support load grows:** manual incident triage becomes slow.
  Add admin workflows for disputes, refunds, failed webhooks, and provider
  capacity incidents.

## Commands To Run Before First Paid Invites

```bash
bash scripts/release-hygiene-scan.sh
git diff --check
scripts/observability-acceptance-smoke.sh
curl -fsS https://bluey.sh/health
curl -fsS https://bluey.sh/pricing/tiers
curl -fsSL https://bluey.sh/install.sh | bash
bluey doctor --json
bluey logs export
```

Then run the full user path in `docs/deploy/PAID-ALPHA-SMOKE.md`.

## Operator Inputs Still Needed

- Fund the answer/vision provider accounts enough for live smoke and the first
  paid users.
- Confirm Square sandbox and production webhooks are green.
- Configure and verify the off-host backup destination.
- Confirm support/refund/dispute mailbox ownership.
- Run the clean-Mac paid-alpha smoke with real credits.
- If Windows is included in the first 100 users, run the Windows paid-alpha gate
  in `docs/deploy/WINDOWS-PAID-ALPHA-READINESS.md`.
