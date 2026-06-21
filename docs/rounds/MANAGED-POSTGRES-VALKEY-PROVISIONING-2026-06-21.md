# Managed Postgres / Valkey Provisioning - 2026-06-21

## Goal

Provision the production-shape cloud services for Bluey without asking desktop
users to install databases, provider keys, or queue infrastructure.

Target architecture:

- user laptop: Bluey UI, local files, local SQLite history, local RAG cache
- Bluey server: auth, billing, managed AI/STT/vision, sync, RAG, provider keys
- managed Postgres + pgvector: durable server source of truth
- managed Valkey/Redis: shared provider capacity, cooldowns, and rate-limit
  state
- R2/S3-compatible object storage: backups, releases, support zips, exports,
  and future synced raw artifacts

## What Changed

### Managed Valkey

DigitalOcean Managed Valkey is provisioned in the same region/VPC as the
`bluey-brain` droplet.

Runtime wiring:

- `/etc/bluey-api/bluey-valkey.env`
  - contains `BLUEY_REDIS_URL`
  - sets `BLUEY_RATE_LIMIT_REDIS_STRICT=1`
  - sets `BLUEY_REDIS_NAMESPACE=bluey-prod`
- `/etc/systemd/system/bluey-api.service.d/20-managed-valkey.conf`
  - loads the Valkey env file into `bluey-api`

Live production now uses managed Valkey for the shared capacity/rate-limit
ledger. This is the safe scale step we can enable before the database runtime
cutover.

### Redis TLS Support

`server/Cargo.toml` now enables Redis TLS through:

```toml
redis = { version = "0.27", default-features = false, features = ["aio", "tokio-rustls-comp", "script"] }
```

This lets `BLUEY_REDIS_URL=rediss://...` work with managed Valkey.

### Managed Postgres

DigitalOcean Managed PostgreSQL is provisioned in the same region/VPC as the
droplet. The server-runtime Postgres migrations were applied successfully with
pgvector enabled.

Postgres runtime env is staged but not loaded by the live systemd service:

- `/etc/bluey-api/bluey-postgres.env`
  - contains `BLUEY_SERVER_DB_BACKEND=postgres`
  - contains `BLUEY_DATABASE_URL`
  - contains `BLUEY_POSTGRES_CA_CERT_PATH=/etc/bluey-api/postgres-ca.pem`
  - uses `BLUEY_POSTGRES_ACCEPT_INVALID_HOSTNAMES=1` for the DigitalOcean
    private VPC hostname/certificate-name mismatch

The Postgres Project CA was installed at:

```text
/etc/bluey-api/postgres-ca.pem
```

The CA is pinned; invalid certificates are not accepted.

### Postgres TLS Support

`server/src/db/mod.rs` now builds the Postgres pool with
`postgres-native-tls`/`native-tls` instead of `NoTls`.

Supported env knobs:

- `BLUEY_POSTGRES_CA_CERT_PATH`
- `BLUEY_POSTGRES_ACCEPT_INVALID_HOSTNAMES=1`
- `BLUEY_POSTGRES_ACCEPT_INVALID_CERTS=1` for local/dev only

## Important Production Truth

Production is still SQLite-backed right now.

This was intentional. A staging boot with:

```bash
BLUEY_SERVER_DB_BACKEND=postgres
BLUEY_DATABASE_URL=...
BLUEY_POSTGRES_CA_CERT_PATH=/etc/bluey-api/postgres-ca.pem
```

failed before the API could safely serve traffic:

```text
Cannot start a runtime from within a runtime.
This happens because a function attempted to block the current thread while
the thread is being used to drive asynchronous tasks.
```

Root cause:

- the current Postgres foundation uses the synchronous `postgres` crate through
  `r2d2_postgres`
- that crate spins/blocks on its own runtime behavior
- Bluey server is already running inside Axum/Tokio
- starting the synchronous Postgres-backed pool inside the async server path can
  panic

That means `BLUEY_SERVER_DB_BACKEND=postgres` is not safe to flip in production
yet, even though the managed database and schema are ready.

## Current Live State

Verified on the droplet after deploy:

```text
bluey-api: active
health endpoint: ok
db_backend: Sqlite
database_url_configured: false
managed Valkey env file: loaded by systemd
R2 backup destination: reachable
signed update manifest: reachable
```

Live service log confirms:

```text
bluey-server starting port=8080 db_backend=Sqlite db_path=/opt/bluey-api/bluey.db database_url_configured=false
```

This is the desired interim state:

- managed Valkey is live
- R2 backups remain live
- Postgres is provisioned/migrated/TLS-ready
- runtime DB remains SQLite until the adapter cutover is corrected and smoked

## Verification Commands

Local server compile:

```bash
cargo check --manifest-path server/Cargo.toml
```

Postgres/Valkey cutover preflight, using staged env only:

```bash
set -a
. /etc/bluey-api/bluey-api.env
. /etc/bluey-api/bluey-postgres.env
. /etc/bluey-api/bluey-valkey.env
set +a

BLUEY_PREFLIGHT_PROFILE=postgres-cutover \
BLUEY_REQUIRE_POSTGRES=1 \
BLUEY_REQUIRE_MANAGED_REDIS=1 \
scripts/bluey-cloud-preflight.sh
```

Result:

```text
preflight passed: 0 warning(s)
```

Live profile with Valkey required:

```bash
set -a
. /etc/bluey-api/bluey-api.env
. /etc/bluey-api/bluey-valkey.env
set +a

BLUEY_REQUIRE_MANAGED_REDIS=1 \
scripts/bluey-cloud-preflight.sh
```

Result:

```text
Redis/Valkey ping succeeded
Redis strict mode enabled
R2/S3 backup destination reachable
health endpoint reachable
signed update manifest files reachable
preflight passed with the expected SQLite warnings
```

## Next Required Code Work

Postgres cutover is blocked on the runtime adapter, not on cloud provisioning.

Recommended fix:

1. Replace the synchronous Postgres runtime path with an async Postgres pool
   (`tokio-postgres`/`deadpool-postgres`) or move every Postgres DB operation
   behind a strict `spawn_blocking` boundary that is proven not to create a
   nested runtime.
2. Add a Postgres-mode boot test that starts the Axum server inside a Tokio test
   with `BLUEY_SERVER_DB_BACKEND=postgres`.
3. Run parity tests for auth, billing, STT reservations, idempotency, sessions,
   cloud RAG, export, delete, and admin reads.
4. Run a staging paid smoke against Postgres:
   - sign up / sign in
   - add credits through Square sandbox and low-dollar production
   - Listen live captions
   - Answer streaming
   - Screen/vision
   - docs attach/remove/drag-drop
   - old session load/delete/RAG recall
   - low-balance and dispute/refund limits
5. Only then add `/etc/bluey-api/bluey-postgres.env` to the live systemd
   service and restart production.

## Operator Notes

- Do not remove `/etc/bluey-api/bluey-valkey.env`; it is live.
- Do not load `/etc/bluey-api/bluey-postgres.env` into production yet.
- Do not delete the managed Postgres cluster; it is the correct destination.
- Do not claim production is Postgres-backed until `/health`/logs show
  `db_backend=Postgres` and paid smoke passes.
- Keep `bluey-dev.db` untracked and untouched.

