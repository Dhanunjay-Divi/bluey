# FIX-580: Global Feed Row Quarantine

## Issue

One semantically incomplete Jobhive row caused the entire 46,378-row Ashby
snapshot to fail after more than 45,000 valid rows had already been read.

## Root Cause

`jobs/workflows/src/global-discovery-runtime.ts` treated a row missing the
canonical job identity fields as a structural source failure. The worker could
not distinguish that bounded semantic defect from an unreadable archive,
malformed CSV, truncated stream, or manifest mismatch, so it aborted the whole
source and left Ashby degraded.

The completion API and persistence layer also had no exact accepted/rejected
row evidence. Marking a source healthy after silently dropping a row would have
made replay, source-health, and operator evidence ambiguous.

## Fix Summary

- Quarantine only the typed `missing_identity` semantic defect.
- Keep archive, stream, CSV, and manifest failures hard failures.
- Preserve the immutable manifest row count.
- Require `accepted_rows + rejected_rows == expected_rows`.
- Cap rejected rows at `min(100, max(1, ceil(expected_rows / 1000)))`.
- Persist exact accepted/rejected counts and rejection-reason totals.
- Require idempotent completion replays to match the original evidence exactly.
- Keep original-source revalidation mandatory before a candidate can become
  application truth.

The production Ashby artifact now has an expected completion shape of 46,378
rows: 46,377 accepted and one rejected with reason `missing_identity`.

## Files Modified

| File | Change |
|------|--------|
| `jobs/workflows/src/global-discovery-runtime.ts` | Bounded semantic quarantine and exact completion evidence |
| `jobs/workflows/src/global-discovery-api.ts` | Completion request/response contract and legacy response compatibility |
| `jobs/workflows/tests/global-discovery-runtime.test.ts` | Worker quarantine, mismatch, and compatibility tests |
| `server/src/db/jobs.rs` | Accepted/rejected completion types |
| `server/src/db/jobs/global_discovery_completion.rs` | Server validation, persistence, replay, and health transition |
| `server/src/db/jobs/tests.rs` | Persistence, replay, conflict, and migration tests |
| `server/src/db/mod.rs` | SQLite/PostgreSQL migration registration and upgrade coverage |
| `infra/postgres/server-runtime/012_jobs_global_candidate_index.sql` | Fresh PostgreSQL schema evidence columns |
| `infra/postgres/server-runtime/016_jobs_global_ingestion_quarantine.sql` | PostgreSQL upgrade migration |
| `infra/sqlite/server-runtime/035_jobs_global_candidate_index.sql` | Fresh SQLite schema evidence columns |
| `infra/sqlite/server-runtime/039_jobs_global_ingestion_quarantine.sql` | SQLite migration slot; runtime column upgrades remain idempotent |

## Edge Cases Handled

- Legacy completion responses that omit accepted/rejected evidence.
- Completion count mismatches.
- Unknown rejection reasons.
- Rejection totals that exceed the bounded cap.
- Duplicate completion with identical evidence.
- Conflicting completion replay.
- Fresh and upgraded SQLite/PostgreSQL schemas.
- Response-only accepted batches with contiguous upload numbering.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml
cargo fmt --manifest-path server/Cargo.toml -- --check
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
(cd jobs && npm run typecheck)
(cd jobs && npm test)
(cd jobs && npm run build)
node jobs/scripts/ci-guards-self-test.mjs
node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/check-provenance-licenses.mjs
git diff --check
```

## Known Limitations

- This fix does not make Jobhive rows application truth. Bluey must still
  revalidate the original employer posting before preparation or execution.
- Only `missing_identity` is quarantinable. New semantic rejection reasons need
  an explicit reviewed contract, cap behavior, and tests.
- R2 replication remains an independent operational dependency and must not be
  represented as healthy while its credential returns `AccessDenied`.
