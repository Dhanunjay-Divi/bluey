# Postgres Runtime Cutover - 2026-06-21

## Why

Bluey had provisioned DigitalOcean Managed Postgres with pgvector, but production still ran on SQLite because the Rust DB layer could open a synchronous Postgres client from async handlers. This round closed that runtime blocker and cut production over to Postgres without losing the existing alpha accounts, balances, usage, auth, STT, or billing rows.

## What Changed

- Added an explicit `run_blocking_db` boundary in `server/src/db/mod.rs`.
- Wrapped DB-layer public operations that receive `DbPool` so synchronous Postgres work runs inside the blocking boundary.
- Hardened `DbPool::get_pg()` to reject direct Postgres connection use from a Tokio multithread runtime unless the caller is inside the blocking DB boundary.
- Updated scalable-readiness checks to enforce the Postgres blocking-boundary guard.
- Added `scripts/bluey-sqlite-to-postgres-backfill.sh` for one-time SQLite-to-Postgres runtime data migration.
- Updated `ops/bluey-api.env.example` to mark the Postgres adapter as live behind the blocking boundary.

## Production Cutover

Host: `bluey-brain` (`165.227.77.152`)

Production was cut over manually without GitHub Actions.

Steps performed:

1. Synced the local worktree to `/opt/bluey-build`, excluding `.git`, `bluey-dev.db`, and build outputs.
2. Built the release server binary on the droplet.
3. Stopped `bluey-api.service`.
4. Checkpointed and backed up SQLite.
5. Applied Postgres runtime migrations.
6. Backfilled SQLite runtime rows into Postgres in one transaction.
7. Installed the new `bluey-server` binary.
8. Added `/etc/systemd/system/bluey-api.service.d/30-managed-postgres.conf`.
9. Fixed Postgres CA certificate readability for the `bluey` service user.
10. Restarted `bluey-api.service`.

Backup directory:

```text
/var/backups/bluey-api/postgres-cutover-20260622T023302Z
```

## Backfill Parity

Backfill source and Postgres target matched exactly:

```text
accounts: 20
credit_batches: 8
refresh_tokens: 341
device_codes: 1
usage_events: 752
stripe_webhook_events: 43
request_idempotency: 702
email_verification_tokens: 3
password_reset_tokens: 2
auth_link_codes: 11
cloud_sessions: 0
cloud_transcript_segments: 0
cloud_cue_responses: 0
cloud_context_artifacts: 0
cloud_rag_chunks: 0
signup_otps: 2
stt_sessions: 50
```

## Verification

Local:

```text
cargo check --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml --lib
cargo test --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml db::blocking_boundary_tests
scripts/check-server-sqlite-boundary.sh
scripts/bluey-scalable-readiness.sh
bash -n scripts/bluey-sqlite-to-postgres-backfill.sh
git diff --check
```

Production:

```text
BLUEY_PREFLIGHT_PROFILE=postgres-cutover scripts/bluey-cloud-preflight.sh
systemctl status bluey-api.service
curl -fsS https://bluey.sh/health
curl -fsS https://bluey.sh/pricing/tiers
```

Production preflight passed with `0 warning(s)`.

Confirmed live process environment:

```text
BLUEY_SERVER_DB_BACKEND=postgres
BLUEY_DATABASE_URL=<set>
```

Current service state:

```text
active (running)
```

Current post-restart warning log:

```text
-- No entries --
```

## Rollback

If Postgres runtime misbehaves:

1. Stop `bluey-api.service`.
2. Remove `/etc/systemd/system/bluey-api.service.d/30-managed-postgres.conf`.
3. Restore `/usr/local/bin/bluey-server` from `bluey-server.previous` in the backup directory.
4. Restore `/opt/bluey-api/bluey.db` from the backup directory if needed.
5. Run `systemctl daemon-reload`.
6. Start `bluey-api.service`.
7. Verify `/health`.

## Areas Most Likely Wrong

- Some non-DB Rust files were reformatted by `cargo fmt` as part of the server test pass; review should confirm there is no unintended behavior change outside the DB layer.
- The current Postgres adapter uses a safe blocking boundary rather than a full async query rewrite. This is production-safe for the current single API server shape, but high-concurrency scale should eventually move hot paths to async Postgres clients or a smaller blocking pool.
- Backfill was one-time into an empty Postgres target. Re-running against a non-empty target intentionally fails unless `BLUEY_BACKFILL_ALLOW_NONEMPTY=1` is set after manual reconciliation.
- The service health endpoint reports server version `0.1.5`; release/version metadata should be cleaned up separately so ops output matches product release versions.
