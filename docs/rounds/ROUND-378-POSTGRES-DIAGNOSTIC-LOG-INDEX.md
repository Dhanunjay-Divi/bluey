# ROUND-378 - Postgres Diagnostic Log Index

Date: 2026-07-05  
Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Why

The owner asked to copy Pinky's diagnostic-log pattern, but adapt it correctly
for Bluey's production database. Pinky indexes diagnostic log chunks in SQLite;
Bluey should use Postgres as the main source of truth and R2 only as durable
blob storage.

The goal is traceability without unsafe logging:

- keep only a short hot local log cache on the droplet
- archive durable diagnostics to private R2
- index those objects in the database
- avoid storing raw prompts, transcripts, screenshots, document text, or answer
  bodies in the diagnostic index
- preserve account/session/kind lookup for support

## What Changed

Added a `diagnostic_log_chunks` table to the active runtime schema:

- SQLite startup migrations
- Postgres runtime compatibility schema
- long-term normalized cloud schema outline

Runtime table fields:

- `account_id`
- `workspace_id`
- `session_id`
- `session_code`
- `kind`
- `storage`
- `object_key`
- `local_path`
- `bytes`
- `sha256`
- `created_at_ms`
- `expires_at_ms`
- `metadata_json`

Added `server/src/db/diagnostic_logs.rs`:

- `record_chunk`
- `recent_for_account`
- `object_refs_for_account`
- `delete_expired_before`
- SQLite and Postgres adapters
- unit tests for account-scoped metadata indexing and expired-row pruning

Added server config for diagnostic log storage:

- `BLUEY_LOG_STORAGE=r2`
- `BLUEY_LOG_R2_ENDPOINT_URL`
- `BLUEY_LOG_R2_ACCESS_KEY_ID`
- `BLUEY_LOG_R2_SECRET_ACCESS_KEY`
- `BLUEY_LOG_R2_BUCKET`
- `BLUEY_LOG_STORAGE_PREFIX`
- `BLUEY_UPLOAD_LOG_RETENTION_DAYS` capped at 180
- `BLUEY_UPLOAD_LOG_MAX_BYTES`

Updated admin support bundles:

- include recent diagnostic log metadata
- include diagnostic chunk counts
- hash object keys and local paths before returning them
- do not include log bodies

Updated account deletion:

- account-scoped diagnostic R2 objects are deleted before hard-delete when
  `log_storage` is configured
- diagnostic DB rows cascade with the account

Updated log archive script:

- after uploading a server operational log bundle, it best-effort inserts a
  `diagnostic_log_chunks` row into Postgres when `BLUEY_DATABASE_URL` and
  `psql` are available
- indexing failure does not break log archive upload
- system log bundles use `account_id = NULL` because they are redacted host
  diagnostics, not user-owned product data

Updated deploy/preflight docs:

- required env now distinguishes log archive destination from API log storage
- preflight checks diagnostic log storage mode, retention, and `psql`
  availability for archive indexing

## Privacy Boundary

This round intentionally does not upload raw desktop logs automatically.

Operational log archives are redacted server/journald/file diagnostics. Product
content remains in the product data path with export/delete controls. Future
desktop support upload should be explicit/user-triggered and redacted before it
uses this table.

## Verification

```bash
cargo test --manifest-path server/Cargo.toml diagnostic_logs --lib -- --nocapture
cargo check --manifest-path server/Cargo.toml
bash -n ops/archive-bluey-logs.sh
bash -n scripts/bluey-cloud-preflight.sh
bash -n ops/bluey-disk-guard.sh
```

Local archive smoke:

```bash
BLUEY_ENV_FILE=/dev/null \
BLUEY_EXTRA_ENV_FILES= \
BLUEY_LOG_DIRS="$tmp/logs" \
BLUEY_LOG_ARCHIVE_LOCAL_DIR="$tmp/archive" \
BLUEY_LOG_ARCHIVE_WORK_DIR="$tmp/work" \
BLUEY_LOG_ARCHIVE_REQUIRE_OFFHOST=0 \
BLUEY_LOG_ARCHIVE_SERVICES= \
BLUEY_LOG_ARCHIVE_MAX_FILES=8 \
BLUEY_DATABASE_URL= \
ops/archive-bluey-logs.sh
```

Result: local tarball and `.sha256` were produced successfully.

## Remaining Work

- Add a user-triggered desktop support upload endpoint that writes redacted
  account/session chunks into this index.
- Add an admin UI for diagnostic chunk lookup by short session/ref id.
- Add a retention worker that deletes expired R2 diagnostic objects, not only
  expired DB index rows.
