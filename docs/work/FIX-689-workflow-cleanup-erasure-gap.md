# FIX-689: Workflow Cleanup Was Not an Account-Deletion Authority

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and verify its memory
> against the current repository state.

## Issue

Account deletion could reach object cleanup without first consuming durable, exact absence proof for
all retained Jobs Temporal executions. The Phase 609 cleanup schema intentionally could not complete,
and the production gateway intentionally exposed no cleanup route.

## Root Cause

The earlier command-authority batch correctly prioritized fail-closed workflow delivery and history
privacy, but it had no authenticated production protocol-v1 inventory authority, no durable cleanup
dispatcher, and no account-scoped source contract for finding old `applicationWorkflow` executions.
Using a caller boolean, a process-local map, or an invented account query would have made deletion
unsafe.

## Fix Summary

Round 610 adds a global namespace-, cutoff-, and fixed-query-bound protocol-v1 inventory with
immutable pages and two aged zero passes, exact protocol-v1/protocol-v2 run reconciliation, a
database-owned combined completion authority, and atomic account-deletion fencing. The cutoff is
cutover authority, not a visibility filter, so late-visible and continue-as-new runs remain in
scope. A first confirmed account delete advances any complete global inventory to a new
database-time proof epoch, freezes the exact account protocol-v2 set, and returns HTTP 202 before
object I/O. Exact retries preserve that binding.

Reversible legacy identifiers and provider page tokens are encrypted while needed and compacted
before global completion; pseudonymous HMAC/digest and proof-epoch material remains. The final
object path atomically binds current workflow cleanup, runner purge, sorted storage scopes, known
object manifests, and prefix sweeps before external deletion. Partial progress is durable, and a
transaction-scoped cascade token prevents child cleanup rows from being deleted outside the exact
account hard-delete transaction. Cleanup remains disabled until hosted Temporal evidence exists.

## Files Modified

| File | Change |
|------|--------|
| `infra/postgres/server-runtime/032_jobs_workflow_cleanup_authority.sql` | Add PostgreSQL v1/v2 cleanup, privacy, sweep, tombstone, and cascade authority |
| `infra/sqlite/server-runtime/054_jobs_workflow_cleanup_authority.sql` | Add the SQLite parity authority and guards |
| `jobs/workflows/src/{contracts.ts,gateway-cleanup-service.ts,gateway.ts}` | Close and register the exact default-off schema-v3 provider boundary |
| `jobs/workflows/tests/{gateway-cleanup-service.test.ts,gateway-http.test.ts}` | Cover the provider protocol, disabled route, limits, mutation, and absence semantics |
| `server/src/db/jobs/workflow_cleanup.rs` | Own inventory, leases, proof epochs, compaction, account binding, sweeps, and tombstones |
| `server/src/jobs_workflow_cleanup.rs` | Add the exact-true default-off durable Rust dispatcher |
| `server/src/api/account.rs` | Make workflow proof and durable sweep authorization prerequisites of hard deletion |
| `server/src/db/account_data.rs` | Freeze deletion/cleanup atomically and enforce exact final transaction gates |
| `server/src/db/jobs/workflow_commands.rs` | Serialize command admission/materialization against the deletion fence |
| `server/src/{main.rs,lib.rs,bin/bluey-jobs-api.rs}` | Export and validate/start the optional dispatcher before background workers |
| `server/src/db/{jobs.rs,mod.rs}` | Export the authority and register paired migrations/replay guards |
| `server/src/db/jobs/{runner_volume_purge.rs,tests.rs}` | Preserve strict workflow/cascade requirements in runner and teardown tests |
| `server/tests/integration_e2e.rs` | Cover fence-first deletion, revalidation, partial progress, hard delete, and privacy |
| `ops/bluey-jobs.env.example`, `jobs/OPERATIONS.md` | Keep every release gate parked and document the external rollout evidence |
| `CHANGELOG.md`, `docs/rounds/ROUND-610-*`, `docs/work/{FIX-689-*,IMPL-PHASE-610-*,REVIEW-PHASE-610-*}` | Record the bounded source result, accepted local review, and parked rollout boundary |

The exact 27-path status and purpose inventory is frozen in
`docs/work/IMPL-PHASE-610-JOBS-WORKFLOW-CLEANUP-AUTHORITY.md`.

## Edge Cases Handled

- response loss before and after provider mutation;
- gateway or dispatcher restart and replica handoff;
- duplicate, changed, repeated-token, oversize, and non-exhaustive pages;
- a late legacy run after an apparent zero scan;
- running protocol-v1 work with no safe termination authority;
- protocol-v2 type, memo, request, payload, first-run, target, lease, and fence mismatch;
- successful empty History responses that are not NotFound;
- visibility lag and second-pass age;
- concurrent account deletion and workflow-command materialization;
- a completed global inventory that predates a first account-deletion fence;
- caller/server clock skew versus database-owned cleanup freshness;
- post-completion raw legacy identifier/token retention and exact rediscovery;
- object-delete success followed by progress-write loss;
- runner/workflow authority drift before and after sweep authorization;
- stale cleanup or runner-purge tombstones before hard delete;
- privacy-safe retention of only pseudonymous deletion proof; and
- success wording that is bounded to configured active storage.

## How to Test

```bash
npm --workspace @bluey/jobs-workflows test
npm --workspace @bluey/jobs-workflows run typecheck
npm --workspace @bluey/jobs-workflows run build
(cd jobs && npm test)
(cd jobs && npm run typecheck)
(cd jobs && npm run build)
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
CARGO_INCREMENTAL=0 cargo build --all-targets
CARGO_INCREMENTAL=0 cargo test --all-targets
```

Current verified result: the focused 267 workflow tests across 12 files passed. The Jobs aggregate
passed 1,694 tests across 133 files (automation 644/35, Browser 219/34, runner 293/32, workflows
267/12, portal 271/20); all five workspace typechecks and builds passed. The generated portal was
byte-identical and added no status path. Focused Rust gates passed 10 cleanup-authority, 25
dispatcher, 16 account-authority, 6 command-lock, 10 account-delete HTTP, and 4 delete-account HTTP
tests. Rust formatting, strict all-target Clippy, all-target build, and all-target tests passed:
1,301 library tests, 105 integration E2Es, and every remaining test target. PostgreSQL 17.10 and
SQLite fresh-install, exact-replay, integrity, parity, and adversarial authority gates passed.
Independent workflows, server, and database reviews accepted the local source with no blockers or
minors. No rollout flag, deployment, hosted provider, or customer state changed.

## Known Limitations

- Production protocol-v1 scope, inventory/drain, hosted deletion/visibility behavior, retention,
  archival, payload-codec/KMS evidence, and live canary behavior cannot be proven locally.
- Cleanup, command dispatch, and customer cloud distribution flags remain `0`.
- API-level NotFound evidence is not a claim of physical or KMS erasure.
- The successful local response certifies only the configured active-storage boundary covered by
  the exact deletion transaction; provider retention and physical erasure remain external evidence.
