# FIX-604: Reserve Evidence Capacity and Fence Account Deletion

> **Codex preflight:** Loaded `$bluey-ops` and verified the irreversible-submit,
> quota, object-storage, privacy-deletion, and dialect-parity boundaries before
> diagnosis and implementation.

## Issue

An employer-facing submission could begin without guaranteed object capacity
for its immutable evidence. Resume and browser-profile objects created before
the common upload ledger could also escape consistent retry and deletion
handling, and an account deletion could race a fresh object put.

## Root Cause

Submission attempts, object quotas, upload/outbox state, Jobs-specific object
pointers, and account deletion were coordinated by separate transactions. The
database had no durable pre-click evidence reservation or deletion intent that
all account-owned object writers had to recheck.

## Fix Summary

Cloud and local final-submit authority now reserves bounded bytes and object
slots atomically before the click. Definite non-submissions release capacity,
unknown outcomes retain it, and a verified immutable receipt consumes it.
Resume-source and browser-profile writes reserve, put, read back, and publish
through the common object ledger. Dialect-paired migrations adopt compatible
legacy objects and fail on conflicting bindings. Account deletion persists a
write fence, drains live puts under an account-scoped reader/writer guard,
purges the account prefix, and only then removes owned rows. SQLite coordinates
in process; PostgreSQL uses a separate bounded connection pool and a
cross-replica advisory lock so an old but still-running put cannot resume after
the final purge. The artifact store and effective audit store are required after
the fence is durable; an absent configuration, indexed-object deletion failure,
or namespace sweep failure returns `503 Service Unavailable` without deleting
the account. The object store is the audit fallback, and an identical endpoint,
bucket, and prefix is swept once. Known cloud-runner records return `409
Conflict` before a deletion intent is created because this batch does not
implement verified runner-volume purge acknowledgements.

## Files Modified

| File | Change |
|------|--------|
| `infra/sqlite/server-runtime/043_account_deletion_intents.sql` | Adds the durable deletion fence. |
| `infra/postgres/server-runtime/021_account_deletion_intents.sql` | Adds its PostgreSQL equivalent. |
| `infra/*/server-runtime/*submission_evidence_reservations.sql` | Adds dialect-paired pre-click capacity. |
| `infra/*/server-runtime/*jobs_account_object_upload_backfill.sql` | Adopts and validates compatible legacy Jobs objects. |
| `server/src/db/account_data.rs` | Serializes writers with deletion, gates cloud-runner state, and preserves retry intent. |
| `server/src/db/mod.rs` | Separates bounded PostgreSQL lifecycle-lock sessions from the primary query pool. |
| `server/src/db/object_uploads.rs` | Reserves, verifies, commits, releases, and cleans object lifecycle state. |
| `server/src/db/jobs/{execution_leases,local_runner}.rs` | Couples evidence capacity to exact runner authority. |
| `server/src/db/jobs/{resume_assets,browser_profile_snapshots}.rs` | Publishes exact pointers with verified uploads. |
| `server/src/api/{account,jobs,jobs_resume_assets}.rs` | Applies deletion, evidence, profile, and resume workflows. |
| `server/src/object_storage.rs` | Adds bounded read/list and account-prefix purge support. |
| Server tests | Cover replay, races, backfill conflicts, tamper, quota, and deletion order. |

## Edge Cases Handled

- Concurrent attempts competing for one account's byte or object ceiling.
- Unknown submissions retaining evidence headroom until reconciliation.
- Exact receipt replay reusing already verified partial uploads.
- Read-back mismatch, lifecycle conflict, losing pointer CAS, and stale pending
  upload cleanup without deleting a winner's immutable object.
- Deletion during a fresh put, retry after purge failure, and stale puts that no
  longer block deletion indefinitely after their live writer guard has ended.
- Missing storage configuration preserving the account and durable fence,
  effective audit fallback, identical-namespace deduplication, and distinct
  artifact/audit sweep failure returning a retryable `503`.
- Irreversible submission outcomes blocking deletion before a fence is created.
- Known cloud-runner sessions or leases blocking deletion before a fence is
  created, and post-fence session/lease creation failing closed.
- PostgreSQL lifecycle-pool saturation leaving primary-pool finalization
  capacity available; unlock failure discards the affected session.
- Legacy rows that conflict with an existing object-ledger identity.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml submission_evidence_capacity
cargo test --manifest-path server/Cargo.toml account_object_lifecycle
cargo test --manifest-path server/Cargo.toml jobs_account_object_backfill
cargo test --manifest-path server/Cargo.toml --test integration_e2e account_delete
node jobs/scripts/check-jobs-schema-parity.mjs
```

## Known Limitations

- Tests that require `BLUEY_TEST_POSTGRES_URL` skip when no isolated PostgreSQL
  service is configured. Live migration replay and concurrent transaction
  behavior remain an explicit external gate.
- No real R2/S3 bucket was mutated; object-store fault and prefix-purge tests use
  local controlled fixtures.
- The server proves the currently configured artifact and effective audit
  namespaces. Supporting namespace rotation requires a durable registry of all
  historical bucket/prefix identities so a removed old namespace cannot escape
  deletion.
- Cloud-runner persistent volumes do not yet have an account purge/fan-out/ack
  protocol. Deletion therefore remains unavailable for accounts with known
  cloud-runner records; legacy volumes must be reconciled or securely wiped
  before cloud Browser distribution or a stronger deletion claim can ship.
