# Round 365 - Infra Storage State Audit

## Trigger

The owner asked whether Bluey is actually using Redis/Valkey, Postgres, and online storage, or whether the product is still mostly local.

Continuity anchor: backup thread id `019e133e-d92a-7830-8df0-3a050a4e22f6`.

## Production State Checked

Checked production on July 5, 2026.

- `bluey-api.service` is active behind Caddy.
- `bluey-api.service` loads:
  - `/etc/bluey-api/bluey-api.env`
  - `/etc/bluey-api/bluey-valkey.env`
  - `/etc/bluey-api/bluey-postgres.env`
- Postgres is configured as the live app database:
  - `BLUEY_SERVER_DB_BACKEND=postgres`
  - database name observed: `defaultdb`
  - account rows observed: `25`
  - usage event rows observed: `1697`
- Redis/Valkey-compatible rate limiting is configured:
  - `BLUEY_REDIS_URL` is set
  - `BLUEY_RATE_LIMIT_REDIS_STRICT=1`
  - Redis responded with `PONG`
  - Redis DB size was `0` at audit time, which is expected when short-lived limiter keys have expired or traffic is idle.
- Object storage is configured:
  - bucket and endpoint are set
  - key prefix: `bluey-cloud`
  - retention days: `365`
  - max object size: `26214400` bytes

## What Lives Where

- Postgres is the online source of truth for accounts, auth, credits, ledger/usage, saved-session metadata, sync/RAG metadata, trial abuse, admin state, and operational records.
- Redis/Valkey is a shared online rate-limit/capacity guard for multi-process safety. It is not the durable app database.
- R2/S3-compatible object storage is configured for original artifact bytes such as attached documents or images when object sync is used.
- Desktop Bluey still has local runtime state for the overlay, local capture/session work, and offline/local client behavior, but paid/cloud/account state is server-side.
- The old `/opt/bluey-api/bluey.db` file exists on the droplet, but the live service is configured for Postgres.

## Follow-Up

- Keep Redis strict mode enabled for production.
- Keep Postgres as the production source of truth.
- Add an admin-facing infra health panel that shows database backend, Redis reachability, object storage configured, backup freshness, and restore-drill freshness without exposing secrets.
