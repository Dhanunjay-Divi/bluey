# Round 264 - Production Data Ops Gates

## Trigger

The owner asked to make the remaining production operations layer ready:

- user exports
- retention deletes that also remove R2 objects
- safe support bundles
- restore drills
- admin visibility into backup health, deletes, exports, support, and disputes

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Changes

Added complete account zip export support on the existing authenticated
`/account/export` endpoint:

- `/account/export` still returns the existing JSON bundle.
- `/account/export?format=zip` returns a zip with:
  - `account-export.json`
  - `manifest.json`
  - `README.txt`
  - `sessions/transcript.md`
  - `sessions/answers.md`
  - original artifact bytes under `artifacts/files/` when object storage is
    configured.
- Zip export fails closed if object storage is missing for referenced objects,
  an object key is outside the account scope, object fetch fails, or
  `BLUEY_EXPORT_MAX_OBJECT_BYTES` is exceeded.
- `include_objects=false` remains available for a metadata/text-only bundle.

Hardened account deletion:

- `/account/delete` still requires:

```json
{
  "confirm_text": "DELETE",
  "accept_data_loss": true,
  "accept_credit_loss": true
}
```

- If synced artifact object references exist, Bluey deletes those R2/S3 objects
  before deleting account DB rows.
- Object keys are verified to be under the account prefix before deletion.
- If object storage is not configured or an object delete fails, deletion fails
  closed and the account row is not removed.
- Delete responses now include `object_count_deleted`.

Added redacted ops audit events:

- New `ops_audit_events` table in SQLite and Postgres runtime schema.
- Events intentionally avoid account foreign keys so export/delete evidence can
  survive hard-delete.
- Account export, account delete, and admin support bundle access now write
  redacted events with account hashes, actor hashes, status, and small metadata
  only.

Added admin operations endpoints:

- `/admin/storage/health`
  - DB backend
  - object storage configured
  - bucket/prefix for admins
  - latest local backup path/size/age/backend hint
  - off-host destination configured/kind
  - support/export/delete feature readiness flags
- `/admin/support/accounts/:account_id`
  - redacted support bundle
  - counts, hashed ids, recent provider/cost rows, artifact object metadata
  - excludes transcript text, answer text, document previews, source URIs, raw
    object keys, and raw email
- `/admin/ops/events`
  - recent redacted export/delete/support audit events

Added restore drill tooling:

- New `ops/restore-drill-bluey-db.sh`.
- Loads `/etc/bluey-api/bluey-api.env` and
  `/etc/bluey-api/bluey-postgres.env`.
- Uses the latest local backup by default.
- For Postgres, requires `BLUEY_RESTORE_DRILL_DATABASE_URL` and refuses to
  restore into live `BLUEY_DATABASE_URL`.
- For SQLite, copies the backup to a temp DB and runs `PRAGMA integrity_check`.
- Prints a sanitized restore summary with backup path, size, checksum, account
  count, and usage event count.

Updated `docs/PRODUCTION-DEPLOY-RUNBOOK.md` with restore drills, zip exports,
object-aware retention delete, and admin operations endpoints.

## Verification

Passed:

```text
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml --test integration_e2e account_export_zip_contains_readable_bundle -- --nocapture
cargo test --manifest-path server/Cargo.toml --test integration_e2e delete_account_deletes_artifact_objects_before_account_rows -- --nocapture
cargo test --manifest-path server/Cargo.toml --test integration_e2e admin_support_bundle_is_redacted -- --nocapture
cargo check --manifest-path server/Cargo.toml
bash -n ops/backup-bluey-db.sh
bash -n ops/restore-drill-bluey-db.sh
git diff --check
```

Disposable SQLite restore drill passed:

```text
restore drill ok
backend=sqlite
accounts=1
usage_events=0
```

The new integration tests verify:

- zip export contains structured export, transcript markdown, answer markdown,
  and records an export audit event
- account delete calls object storage DELETE before removing the account and
  records a delete audit event
- admin support bundle does not leak transcript text, answer text, or raw email
  and `/admin/ops/events` exposes the redacted support event

## Current State

The production data operations layer is now implemented and locally verified in
the repo.

This round did not deploy a new live server binary to the droplet. Live rollout
should follow the production runbook:

1. create/confirm latest backup
2. install the new server binary
3. restart `bluey-api.service`
4. verify `/health`, `/admin/storage/health`, `/admin/ops/events`
5. run a Postgres restore drill against a disposable drill DB

## Remaining QA/Gates

- Run the Postgres restore drill with a real disposable managed Postgres target.
- Add a web admin UI page on top of the new admin endpoints if owner wants a
  browser panel instead of API/CLI checks.
- If a durable worker queue is added later, include worker failed-job counts in
  `/admin/storage/health` or a dedicated `/admin/jobs` endpoint.
