# FIX-629: Apply ATS Certification at Runtime Admission

> **Codex preflight:** Loaded `$bluey-ops` and diagnosed the failure through a
> source-test signed local Browser claim fixture in the active Round 604
> worktree.

## Issue

An otherwise valid certified Auto-submit application could not reach Phase A.
Local and cloud runtime admission recomputed legacy eligibility, which left
Greenhouse and Lever at `beta_review`, and rejected the run before applying the
current server-owned ATS certification.

## Root Cause

`evaluate_job_eligibility` and packet finalization used the shared ATS resolver,
but `current_execution_authorized_sqlite` and
`current_execution_authorized_postgres` called `build_job_eligibility` plus
discovery authority only. This violated the one-resolver invariant at the last
runtime admission gate.

## Fix Summary

- Resolve current ATS certification inside the existing SQLite/PostgreSQL
  execution transaction using the same posting resolver as evaluation and
  packet finalization.
- Apply the resolution before checking runner-specific queueability and
  `can_auto_submit`.
- Fail closed on every non-storage resolver error and propagate storage errors.
- Keep exact local/cloud runtime identity enforcement in the immediately
  following Phase A binding; this eligibility resolution grants no standalone
  submit authority.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/execution_authority.rs` | Apply transaction-scoped ATS resolution to both dialects before current runtime admission. |
| `server/src/db/jobs/tests.rs` | Drive signed local/cloud source-test paths through current admission, Phase A, Phase B, drift, and replay fences. |
| `docs/work/FIX-629-jobs-certified-runtime-admission-resolver.md` | Record the production-path blocker and repair. |

## Edge Cases Handled

- A missing, expired, revoked, drifted, suspended, or malformed resolution
  remains Review-only and cannot enter Phase A.
- Local certification cannot authorize cloud and cloud certification cannot
  authorize local because runner intersection is rechecked before exact Phase A.
- PostgreSQL callers already hold the ATS advisory lock before this resolver;
  SQLite callers use an immediate transaction.
- The resolver does not replace Track Auto authorization, identity, resume,
  packet, discovery, entitlement, attempt, or evidence revalidation.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml \
  certified_local_production_path_is_single_submit_even_after_response_loss
cargo test --manifest-path server/Cargo.toml \
  certified_local_layout_drift_quarantines_before_any_submit_write
cargo test --manifest-path server/Cargo.toml certified_cloud
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
```

## Known Limitations

- Live PostgreSQL contention and an authenticated managed cloud fleet remain
  external-environment gates; local source tests cannot manufacture them.
