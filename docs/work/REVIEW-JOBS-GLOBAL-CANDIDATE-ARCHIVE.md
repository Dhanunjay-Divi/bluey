# REVIEW: JOBS-GLOBAL-CANDIDATE-ARCHIVE - Cold storage and write control

**Commit range:** Uncommitted feature-branch diff
**Reviewer:** Codex self-review
**Date:** 2026-07-31

## Per-Task Review

### JOBS-GLOBAL-DISCOVERY-NOOP - Deterministic ingestion

| Field | Value |
|-------|-------|
| Files | `server/src/db/jobs/global_discovery.rs`, `server/src/db/jobs/tests.rs` |
| Verdict | 🟢 accept |

**Findings:**

- Plaintext normalized JSON is hashed before randomized encryption.
- The PostgreSQL no-op CTE still returns the existing canonical ID.
- Metadata-only manifest refresh preserves a completed run schedule.

### JOBS-GLOBAL-CANDIDATE-ARCHIVE - Verified R2 lifecycle

| Field | Value |
|-------|-------|
| Files | `server/src/db/jobs/global_archive.rs`, `server/src/jobs_global_archive.rs`, migration and object-storage files |
| Verdict | 🟢 accept |

**Findings:**

- Eligibility is fail-closed on expiration, age, active memberships, and
  account materialization.
- Completion is conditional on the same lease and content hash.
- PostgreSQL is tombstoned only after exact R2 byte and SHA-256 read-back.
- Failure retains the complete payload and schedules bounded retry.
- Content-addressed objects left unreferenced by a guarded completion race are
  reconciled through object inventory, avoiding a delete race with a newer
  lease using the same key.

## Cross-Task Findings

- Original resumes and heavy submission evidence are already object-storage
  backed; PostgreSQL remains the account-scoped authorization and search
  authority.
- Cross-customer training is not authorized by this lifecycle and must not be
  inferred from artifact retention.

## Build & Test Verification

```bash
cargo fmt --all --check
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path server/Cargo.toml
bash scripts/check-jobs-schema-parity.sh
bash scripts/check-jobs-privacy.sh
bash scripts/check-jobs-client-boundary.sh
bash scripts/check-jobs-provenance.sh
bash scripts/check-server-sqlite-boundary.sh
git diff --check
```

Results:

- 876 server tests passed with zero failures.
- Strict Clippy passed with warnings denied.
- Formatting and whitespace checks passed.
- Jobs SQLite/PostgreSQL schema parity passed.
- Jobs privacy, client/server boundary, and dependency/license/provenance gates
  passed.
- The supported SQLite-boundary audit passed in its informational mode and
reported 55 existing mixed-backend references. Strict PostgreSQL-only mode
is not a passing repository baseline and was not claimed as completed by
this batch.

## Overall Verdict

🟢 accept

## Follow-ups for Next Batch

- Production canary and R2 object read-back proof.
- Terminal application bundle hydration/retention design.
