# Round 059 - First 100 Paid Users Readiness — 2026-06-19

## Why This Round Exists

Bluey was drifting between "make the first paid alpha work" and "design the
worldwide scale architecture." This round narrows the operating target to the
first 100 paid users while preserving the same architecture contract for later
scale.

## Decision

Run the first 100 paid users on the current production-shaped stack:

- one `bluey.sh` origin,
- one DigitalOcean droplet,
- Caddy + `bluey-server`,
- SQLite for alpha server state,
- Square for manual credit reloads,
- managed provider keys only on the server,
- local desktop sessions and local RAG for speed,
- R2/S3-compatible storage for releases, backups, support zips, and synced raw
  artifacts.

Do not add Redis, Postgres, pgvector, Kubernetes, or multi-region API servers
until metrics hit the upgrade triggers in
`docs/deploy/FIRST-100-PAID-USERS.md`.

## What Changed

- Added `docs/deploy/FIRST-100-PAID-USERS.md`.
- Updated `docs/deploy/BLUEY-SH-LAUNCH.md` to state that one droplet is fine
  for the first 100 controlled paid users, not a global ultra-low-latency
  promise.
- Added a first-100 paid user gate to `docs/PRELAUNCH-CHECKLIST.md`.

## What This Does Not Claim

- It does not claim global ultra-low latency from a single droplet.
- It does not remove the need for live Square/provider/Mac smoke tests.
- It does not put provider keys on customer laptops.
- It does not require customers to install Redis, Postgres, Docker, or pgvector.

## Verification

```bash
bash scripts/release-hygiene-scan.sh
git diff --check
scripts/observability-acceptance-smoke.sh
curl -fsS https://bluey.sh/health
curl -fsS https://bluey.sh/pricing/tiers
bluey doctor --json
bluey logs export
```

The full paid-user proof remains `docs/deploy/PAID-ALPHA-SMOKE.md`.

## Dependencies On Operator

- Provider accounts funded and capped/alerted.
- Square sandbox and production webhook tests green.
- Off-host backup destination configured.
- Clean-Mac paid-alpha smoke run with real credits.
- Support/refund/dispute owner assigned.

