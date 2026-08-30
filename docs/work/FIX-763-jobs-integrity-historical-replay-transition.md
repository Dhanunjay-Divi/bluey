# FIX-763: Jobs integrity historical replay transition

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against the
> authoritative Phase 614B worktree. The SSD archive was not used.

**Severity:** P2 immutable replay result instability

**Status:** Implemented; direct SQLite and configured-PostgreSQL lifecycle tests are green on the
current source. Final aggregate evidence remains pending.

## Issue

Replaying an exact older signed job-integrity attestation after a successor advanced the current
head returned the authenticated object as a replay, but omitted the original `headRevision` and
`headTransitionSha256` from its import result.

## Root Cause

`existing_job_integrity_attestation_sqlite` and
`existing_job_integrity_attestation_postgres` recovered replay metadata by left-joining the
mutable current-head table. Once the head pointed at a successor, the older attestation no longer
joined a row even though its original transition remained immutable and unique.

## Fix Summary

- Recover replay metadata from `jobs_job_integrity_head_transitions`, joined by its unique
  `attestation_sha256`, in both storage dialects.
- Preserve the original immutable transition and revision for an exact replay regardless of later
  head advancement.
- Extend the SQLite and configured-PostgreSQL positive lifecycles to import a successor and then
  assert that replaying the predecessor returns the exact original import result with only
  `replayed=true` changed.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/job_integrity_authority.rs` | Join immutable transition history for replay and add successor-replay parity assertions. |
| `docs/work/FIX-763-jobs-integrity-historical-replay-transition.md` | Record the defect, correction, and evidence. |

## Edge Cases Handled

- Exact replay before and after a successor returns the same original transition identity.
- Current-head resolution continues to use the current-head table; only immutable import replay
  recovery uses transition history.
- Changed identity, authorization collision, predecessor fork, and nonexact bytes retain their
  existing fail-closed conflict behavior.

## Evidence

- **PASS:**
  `db::jobs::job_integrity_authority_tests::signed_positive_lifecycle_replays_and_fails_closed_on_source_or_expiry`,
  1 passed and 0 failed.
- **PASS — configured PostgreSQL 17.10 `r6`:**
  `db::jobs::job_integrity_authority_tests::postgres_positive_lifecycle_when_configured`,
  1 passed and 0 failed.
- **PASS:** `job_integrity_authority_tests`, 18 passed and 0 failed; the three configured
  PostgreSQL cases in that unconfigured filter invocation returned through their explicit skip
  branches and are not counted as configured evidence.
- **PENDING:** final frozen-source PostgreSQL manifest, full Rust aggregate, and strict Clippy.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  db::jobs::job_integrity_authority_tests::signed_positive_lifecycle_replays_and_fails_closed_on_source_or_expiry \
  -- --exact --nocapture --test-threads=1

BLUEY_TEST_POSTGRES_URL=<isolated-postgres-url> cargo test \
  --manifest-path server/Cargo.toml --lib \
  db::jobs::job_integrity_authority_tests::postgres_positive_lifecycle_when_configured \
  -- --exact --nocapture --test-threads=1
```

## Known Limitations

- Local PostgreSQL does not replace hosted failover, proxy, backup, or restore evidence.
- This fix stabilizes authenticated import results; it does not make historical attestations
  current execution authority.
