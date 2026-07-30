# IMPL: Round 580 - Global Feed Row Quarantine

## Scope

**Does:**

- Salvage valid rows from a large global-discovery snapshot when a bounded
  number of rows lack canonical identity fields.
- Make accepted/rejected evidence explicit, validated, durable, and replay-safe.
- Keep source health fail-closed until completion evidence is accepted.
- Upgrade fresh and existing SQLite/PostgreSQL databases.

**Does NOT:**

- Tolerate structural archive, CSV, stream, or manifest corruption.
- Grant application or Auto-submit authority to external-feed candidates.
- Enable model generation, local Browser distribution, or cloud Browser
  distribution.
- Change the main API, Caddy, native overlay, audio, STT, or release artifacts.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/workflows/src/global-discovery-runtime.ts` | Modified | Quarantine bounded `missing_identity` rows and upload accepted rows |
| `jobs/workflows/src/global-discovery-api.ts` | Modified | Carry exact completion evidence |
| `jobs/workflows/tests/global-discovery-runtime.test.ts` | Modified | Cover quarantine and fail-closed behavior |
| `server/src/db/jobs.rs` | Modified | Define completion evidence |
| `server/src/db/jobs/global_discovery_completion.rs` | Modified | Validate and persist exact evidence |
| `server/src/db/jobs/tests.rs` | Modified | Cover replay, conflicts, storage, and migrations |
| `server/src/db/mod.rs` | Modified | Register and verify migrations |
| `infra/postgres/server-runtime/012_jobs_global_candidate_index.sql` | Modified | Add fresh-schema evidence columns |
| `infra/postgres/server-runtime/016_jobs_global_ingestion_quarantine.sql` | Created | Upgrade PostgreSQL |
| `infra/sqlite/server-runtime/035_jobs_global_candidate_index.sql` | Modified | Add fresh-schema evidence columns |
| `infra/sqlite/server-runtime/039_jobs_global_ingestion_quarantine.sql` | Created | Reserve SQLite migration order |

## Build & Test

```bash
cargo test --manifest-path server/Cargo.toml
# 781 unit + 76 integration + 2 focused matrix/migration tests passed

cargo fmt --manifest-path server/Cargo.toml -- --check
# success

cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
# success

(cd jobs && npm run typecheck)
# automation, browser, runner, workflows, and portal passed

(cd jobs && npm test)
# 469 tests passed

(cd jobs && npm run build)
# success

node jobs/scripts/ci-guards-self-test.mjs
node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/check-provenance-licenses.mjs
# all passed
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| None | The implementation preserves the planned bounded semantic-only quarantine |

## Known Follow-ups

- Activate the exact merged Jobs API and worker artifacts manually.
- Take and verify a fresh PostgreSQL backup before migration.
- Re-run Ashby and confirm 46,377 accepted plus one `missing_identity` rejection.
- Restore R2 replication independently if its production credential still
  returns `AccessDenied`.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated product changes included
- [x] Tests cover acceptance criteria
- [x] Code style matches repository rules
- [x] No TODOs or secrets introduced
