# Cloud Infra Architecture Contract - 2026-06-21

## Goal

Convert the storage/global release discussion into concrete operator artifacts
without pretending the current SQLite bluey-server can switch to Postgres by
setting one environment variable.

## What Changed

- Added `docs/deploy/PRODUCTION-CLOUD-ARCHITECTURE.md`
  - Defines the final customer/server/provider boundary.
  - States that users install only Bluey; no Redis, Postgres, pgvector, Docker,
    R2, or provider keys live on the laptop.
  - Captures local uninstall/retention behavior: keep lightweight history by
    default, purge only on explicit user action, prune heavy captures/docs by
    age/size.
  - Defines R2/S3 as blob/object storage only, never the ledger/source of truth.
  - Defines Redis/Valkey as shared provider capacity before multi-server.
  - Defines Postgres + pgvector as the server source of truth after the SQLite
    alpha cutover.
- Added `scripts/bluey-cloud-preflight.sh`
  - Validates the environment contract for paid-alpha ops.
  - Checks JWT, DB path, Square mode, provider key pools, SMTP, Redis, R2 backup
    destination, health endpoint, and signed update manifest reachability.
  - Does not print secrets.
  - Warns if `BLUEY_DATABASE_URL` is set because the current runtime is still
    SQLite-backed.
- Updated `ops/bluey-api.env.example`
  - Added Gemini provider key pool.
  - Added Redis/Valkey settings.
  - Added R2/S3 backup/object-storage placeholders.
  - Added a commented future Postgres URL with an explicit warning not to set it
    for the current SQLite binary.
- Updated deploy/infra docs
  - Linked the production architecture from the scaling and infra docs.
  - Replaced stale Phase 3 environment names with the current
    `ops/bluey-api.env.example` contract.
  - Added `scripts/bluey-cloud-preflight.sh` to first-100 and launch checks.
- Expanded `infra/migrations/001_initial_cloud_schema.sql`
  - Added explicit Postgres target tables for wallets, billing profiles,
    credit batches, processor events, reload attempts, idempotency, usage, and
    STT reservation/settlement.
  - This is schema only; runtime SQLite remains unchanged.

## Current Runtime Truth

- Desktop: SQLite + local files + local RAG cache.
- Server runtime today: SQLite.
- Server scaling hooks today: Redis/Valkey provider capacity if
  `BLUEY_REDIS_URL` is set.
- Server object storage today: backup script supports R2/S3-compatible upload.
- Future server DB: Postgres + pgvector schema exists in `infra/migrations`.

## What This Does Not Claim

- It does not migrate the running server DB layer to Postgres.
- It does not upload synced artifacts to R2 yet.
- It does not add multi-region gateways.
- It does not change the desktop release boundary.

## Verification

Run:

```bash
bash -n scripts/bluey-cloud-preflight.sh
BLUEY_PREFLIGHT_STRICT=0 scripts/bluey-cloud-preflight.sh ops/bluey-api.env.example
git diff --check
```

The example preflight is expected to fail on placeholders; the useful check is
that it fails cleanly without leaking values.

## Next Required Implementation

1. Add a real Postgres backend adapter for `server/src/db` and migrate handlers
   away from direct `rusqlite` assumptions.
2. Add a SQLite -> Postgres backfill and ledger-total comparison.
3. Add R2 object APIs for synced raw artifacts, support zips, and cloud exports.
4. Run staging with Postgres + Valkey + R2 before replacing the SQLite droplet.
