# IMPL: Round 596 - Durable Cloud Checkpoint Recovery

> **Codex preflight:** Loaded `$bluey-ops` and verified the current feature
> branch, repository state and disabled production Jobs flags.

## Scope

**Does:** persist versioned runner checkpoints, reconcile them through a
worker-authenticated API, atomically bind cloud attempts, release safe work,
and preserve possibly activated submissions as unknown.

**Does NOT:** enable Browser distribution, start a production Temporal worker,
certify an ATS tenant, or automatically retry an uncertain submission.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/runner/src/run-checkpoint-store.ts` | Modified | Read and persist v1/v2 checkpoints safely |
| `jobs/runner/src/execution-lease.ts` | Modified | Reconcile retained checkpoints with the server |
| `jobs/runner/src/server.ts` | Modified | Recover before accepting new work and delete only after acknowledgement |
| `jobs/runner/tests/*.test.ts` | Modified | Cover upgrade, replay, safe and unsafe recovery |
| `server/src/api/jobs.rs` | Modified | Add the authenticated checkpoint reconciliation route |
| `server/src/db/jobs/execution_leases.rs` | Modified | Add atomic claim binding and SQLite/PostgreSQL recovery transactions |
| `server/src/db/jobs/tests.rs` | Modified | Cover lease, token, attempt and receipt invariants |
| `server/tests/integration_e2e.rs` | Modified | Cover the signed worker route and fencing contract |
| `CHANGELOG.md` | Modified | Record the restart-recovery fix |
| `docs/rounds/ROUND-596-JOBS-DURABLE-CLOUD-CHECKPOINT-RECOVERY.md` | Created | Round evidence |
| `docs/work/FIX-586-jobs-cloud-checkpoint-recovery.md` | Created | Root-cause record |
| `docs/work/REVIEW-JOBS-ROUND-596.md` | Created | Self-review |

## Build & Test

```text
jobs/runner npm test                     54 passed
jobs/runner npm run typecheck             passed
cargo test --manifest-path server/Cargo.toml
  library                               826 passed
  HTTP integration                      80 passed
  additional integration                 5 passed
cargo test ... checkpoint                 4 passed
cargo clippy --all-targets -D warnings    passed
cargo fmt --check                         passed
git diff --check                          passed
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| No live PostgreSQL recovery run | `BLUEY_TEST_POSTGRES_URL` is not configured locally; equivalent PostgreSQL code and schema checks are present, but live PostgreSQL remains a deployment gate |
| No deployment | Cloud/local Browser distribution flags remain disabled until the complete worker and fault matrix passes |

## Known Follow-ups

- Exercise checkpoint reconciliation against the production-compatible
  PostgreSQL test schema.
- Deploy the durable Temporal worker and isolated cloud Browser pool behind
  disabled distribution flags.
- Add worker restart and browser-container crash tests across processes.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from plan
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs
