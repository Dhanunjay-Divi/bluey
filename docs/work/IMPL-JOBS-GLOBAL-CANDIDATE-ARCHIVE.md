# IMPL: JOBS-GLOBAL-CANDIDATE-ARCHIVE - Verified cold candidate storage

## Scope

**Does:**

- Stops metadata-only source refreshes from retriggering completed ingestion.
- Stops unchanged job candidates from rewriting canonical PostgreSQL rows.
- Archives only expired, unreferenced candidate bodies to private R2.
- Verifies exact bytes and SHA-256 before PostgreSQL tombstoning.
- Preserves retries, leases, reactivation, and account-materialization guards.
- Documents the PostgreSQL/R2 authority split and customer-data boundaries.

**Does NOT:**

- Enable the archive worker in production.
- Archive active jobs, account matches, application records, or live receipts.
- Use one customer's artifacts to train a cross-customer model.
- Change Jobs model generation, local Browser, or cloud Browser flags.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `server/src/db/jobs/global_discovery.rs` | Modified | Semantic source and candidate updates |
| `server/src/db/jobs/global_archive.rs` | Created | Transactional archive lifecycle |
| `server/src/jobs_global_archive.rs` | Created | Object-store worker and read-back verification |
| `infra/postgres/server-runtime/017_jobs_global_candidate_archive.sql` | Created | PostgreSQL archive columns/index |
| `server/src/db/mod.rs` | Modified | Register PostgreSQL/SQLite schema |
| `server/src/object_storage.rs` | Modified | Deterministic archive key |
| `server/src/bin/bluey-jobs-api.rs` | Modified | Start/stop opt-in archive worker |
| `server/src/db/jobs/tests.rs` | Modified | Lifecycle and regression tests |
| `jobs/OPERATIONS.md` | Modified | Activation, monitoring, rollback, and data boundary |

## Build & Test

```bash
cargo fmt --all --check
cargo test --manifest-path server/Cargo.toml global_candidate_archive
cargo test --manifest-path server/Cargo.toml jobs_global_archive
cargo test --manifest-path server/Cargo.toml \
  unchanged_global_candidate_does_not_rewrite_the_canonical_payload
cargo test --manifest-path server/Cargo.toml \
  legacy_expired_candidate_hashes_before_archive
cargo test --manifest-path server/Cargo.toml \
  corrupt_legacy_candidate_retries_without_blocking_valid_candidate
cargo test --manifest-path server/Cargo.toml \
  rediscovered_archived_global_candidate_restores_the_hot_payload
```

Final full-suite results are recorded after verification in this batch review.
The completed pass recorded 876 server tests with zero failures, strict
Clippy, formatting, schema parity, privacy, client-boundary, provenance, and
whitespace gates all passing.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Archive remains default-off | Production R2 permissions and exact read-back must pass before deletion of any hot payload |
| Customer application rows remain in PostgreSQL | Current authorization, interview-prep, export, and deletion paths query this authority directly |

## Known Follow-ups

- Run a production R2 test-object write/read/delete preflight with rotated,
  least-privilege credentials.
- Enable a canary batch only after migration and candidate eligibility counts
  are reviewed.
- Design terminal application bundle archival only after a complete hydration
  path and reviewed retention policy exist.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from plan
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs
- [x] `bluey-ops` preflight and no-Keychain behavior remain unchanged
