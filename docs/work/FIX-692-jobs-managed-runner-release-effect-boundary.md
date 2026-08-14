# FIX-692: Managed Runner Effects Were Not Bound to the Frozen Release

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the Phase 611 activity, execution-lease,
> checkpoint, and runner boundaries against the current worktree. No provider, employer portal,
> hosted runner, credential, deployment, canary, or flag was used.

## Issue

A managed workflow command could be admitted on release A, then materialized or executed by a
runner without proving that immutable A and a fresh, compatible managed-runner runtime B at every
pre-effect boundary.

## Root Cause

The workflow command was opaque to Temporal and the legacy activity/runner contracts did not carry
managed release authority. Execution leases, volume proofs, checkpoints, resume, and the final
irreversible authorization therefore bound account/application/run fences but not the release that
admitted the command or the exact live runner instance. A current fleet flag alone could not prove
that a process executing employer-facing I/O was a compatible member of A's release set.

## Fix Summary

- Materialization authenticates the command and returns the original canonical release-A memo.
  Resume uses the original start request's A rather than the mutable current head. Legacy command
  responses remain byte-shape compatible.
- Managed Temporal branches carry A through distinct managed activity contracts; legacy activity
  names, arguments, and request bodies remain unchanged.
- Managed run start/resume, execution-lease claim, volume proof, checkpoint persistence/restore,
  intervention resume, and result-recovery context bind the exact A digest plus workflow request,
  runtime instance ID, and fenced runtime epoch.
- Lease claim proves the frozen command/request-start binding, a fresh compatible managed-runner B,
  and signed current release/revocation/readiness authority before returning A+B. All managed
  response echoes are exact-key and compared to the runner's local runtime identity.
- Add an explicit `authorize-managed-effect` boundary immediately before new checkpoint restore or
  resume and require a same-transaction B/revocation/runtime revalidation before the irreversible
  click marker. A changed, stale, revoked, or wrong-runtime B cannot authorize new I/O.
- Keep result lookup, ambiguity persistence, finish, and terminal receipt recovery independent of
  mutable B after an effect may have happened. They cannot be downgraded into another effect.

## Files Modified

| File | Change |
|------|--------|
| `server/src/api/jobs.rs` | Authenticate materialization and require/echo the exact original release-A memo for managed commands |
| `server/src/db/jobs/workflow_commands.rs` | Resolve the immutable start/resume A authority without following the mutable current head |
| `jobs/workflows/src/{workflows,activities}.ts` | Propagate A only through managed activity paths and verify exact server/runner echoes |
| `jobs/workflows/tests/{workflows,activities-auth}.test.ts` | Prove deterministic managed propagation and byte-identical legacy behavior |
| `server/src/api/jobs_worker_auth.rs` | Authenticate the managed execution-lease and pre-effect authorization routes |
| `server/src/db/jobs/managed_cloud_release_authority.rs`, `server/src/db/jobs.rs` | Persist, revalidate, and integrate managed execution-lease A+B authority around new effects |
| `jobs/runner/src/execution-lease.ts` | Send the all-or-none managed claim tuple and require exact A+B claim, authorization, and irreversible-boundary echoes |
| `jobs/runner/src/{server,run-checkpoint-store,runner-volume-client,intervention-policy}.ts` | Carry A through run/resume, durable checkpoints, volume proof, interventions, and recovery without adding B to terminal recovery |
| `jobs/runner/tests/managed-cloud-effect-boundary.test.ts` and related runner tests | Cover stale/swapped A+B, runtime fencing, legacy shapes, pre-I/O failure, and recovery-only paths |
| `infra/{postgres,sqlite}/server-runtime/*jobs_managed_cloud_release_authority.sql` | Add all-or-none lease binding and immutable request-start/release references |

The complete Phase 611 path inventory is maintained in
`docs/work/IMPL-PHASE-611-JOBS-MANAGED-CLOUD-LAUNCH-AUTHORITY.md`.

## Edge Cases Handled

- current head advances from A to compatible or incompatible B after admission;
- A is revoked before first effect versus after request-start/possible side effect;
- resume tries to follow the current head instead of the original start release;
- swapped release memo, workflow request ID, runtime instance ID, epoch, lease token, or fence;
- runner heartbeat expires between claim and checkpoint restore or irreversible click;
- checkpoint or volume proof was created by another release/runtime generation;
- a managed endpoint omits or malforms A+B, or a legacy endpoint gains an extra field;
- result/finish replay after B expires or is revoked; and
- recovery ambiguity that must never trigger a second employer-facing effect.

## How to Test

```bash
npm --workspace @bluey/jobs-workflows test
npm --workspace @bluey/jobs-runner test
npm --workspace @bluey/jobs-workflows run typecheck
npm --workspace @bluey/jobs-runner run typecheck
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --lib
git -P diff --check
```

The managed activity propagation passed an interim 285-test workflows suite before later gateway
recovery additions. Final runner, workflows, Rust, migration-parity, and adversarial effect-boundary
gates remain pending at this documentation checkpoint.

## Known Limitations

- Source tests do not prove a hosted runner is executing the signed image digest or under a
  read-only root filesystem; both require platform canary evidence.
- Real ATS/provider behavior, browser capacity, live PostgreSQL concurrency, network faults, and
  physical runtime replacement remain external tests.
- This fix does not enable cloud distribution, dispatch, cleanup, or any customer cohort.
