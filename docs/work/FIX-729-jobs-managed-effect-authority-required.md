# FIX-729: Managed Effect Authority Required

> **Codex preflight:** Load `$bluey-ops` before diagnosis or implementation and verify its memory
> against the final Phase 614 record, Round 614B, and the current repository state.

**Severity:** P2 defense in depth

**Status:** Implemented; focused evidence green; Phase 614B aggregate gates pending

## Issue

Before this fix, `authorize_managed_execution_effect` accepted optional managed authority. Its
`None` branches skipped managed operational-hold, release, ATS, and current-admission checks that
the managed effect path is expected to require.

The bypass was not externally reachable: the only API caller already rejected missing managed
authority. The database API was nevertheless weaker than its name and security contract, so a
future internal caller or test could have invoked a managed effect without the required authority.

## Root Cause

The function preserved an optional parameter for mixed or historical call patterns even though its
current semantic scope is managed execution. The HTTP layer supplies the missing invariant, but the
authoritative database boundary does not encode it in the type or reject it before transaction
work.

## Required Fix

The historical FIX-729 correction, scoped specifically to
`authorize_managed_execution_effect`, does the following:

- changes the managed-effect boundary to require
  `&ManagedCloudExecutionLeaseClaimInput`, so absence is unrepresentable before database access;
- removes the SQLite and PostgreSQL `None` branches that skipped managed worker discovery,
  `H -> M -> ATS` prelock, current-admission resolution, and final worker revalidation;
- preserves the API's HTTP `400` responses for wholly missing and partially supplied authority;
- preserves the complete-authority input and positive response shape;
- adds a compile-time function-signature test that cannot type-check with a direct `None` call; and
- does not make every shared FinalSubmit boundary managed-only.

Phase 614B composes its signed source-integrity recheck at this same effect boundary. This fix owns
only the nonoptional input invariant of the explicitly managed authorization function. The later
FIX-762 repair owns durable classification and pairing for the shared FinalSubmit path, where a
legitimate unmanaged cloud execution has no managed tuple and a managed execution may not omit or
change that tuple.

## Files Modified

| File                                     | Change                                                        | Status      |
| ---------------------------------------- | ------------------------------------------------------------- | ----------- |
| `server/src/db/jobs/execution_leases.rs` | Require managed authority and remove skipping `None` branches | Implemented |
| `server/src/api/jobs.rs`                 | Preserve explicit HTTP validation and pass required authority | Implemented |
| Unit/API tests in the two files above    | Cover missing, complete, and typed-boundary behavior           | Green       |
| Existing worker-auth regression          | Preserves legacy-debug bearer rejection                        | Green       |
| This FIX                                 | Record actual implementation and focused evidence             | Updated     |

## Edge Cases To Handle

- Missing managed authority is rejected before transaction creation.
- A revoked or stale provided authority fails through normal current-admission checks.
- A valid current authority follows the existing positive managed route.
- A legitimate unmanaged cloud FinalSubmit remains possible only after durable state proves the
  execution is unmanaged; absence is never accepted as a shortcut around stored managed state.
- Refactoring cannot reintroduce an optional default or a test-only `None` bypass.

## How To Test

Observed focused evidence:

```text
cargo fmt --all -- --check                                          PASS
git diff --check -- execution_leases.rs jobs.rs                     PASS
cargo check --manifest-path server/Cargo.toml                       PASS (34.42s)
cargo test ... managed_execution_effect -- --nocapture              PASS (4/4)
cargo test ... managed_cloud_execution_ -- --nocapture              PASS (6/6)
cargo test ... managed_cloud_runner_effect_requires_exact_...       PASS (1/1)
cargo test ... local_submit_distribution_gate -- --nocapture        PASS (1/1)
```

The four managed-effect tests prove the required HTTP `400`, incomplete-tuple HTTP `400`, exact
complete tuple preservation, nonoptional database function signature, and continued rejection of
the legacy debug bearer. The six managed-cloud execution tests preserve managed input pairing,
canonical response/digest binding, current effect admission, and migration replay. The local
distribution test remains green. The exact managed runner-effect test continues to reject role,
epoch, release, and worker drift while accepting the unchanged complete tuple. Later shared
FinalSubmit changes are deliberately outside the historical source claim of FIX-729 and are
tracked by FIX-762.

An additional existing local-submit database test was attempted but stopped during its fixture
setup at the already-known Phase 614 queue-authority denial (`current Jobs authority does not permit
application queueing`), before it could reach the local submit behavior. It is therefore not
claimed as FIX-729 evidence; Phase 614B's signed positive fixture work owns that shared baseline.

## Known Limitations

- This is a defense-in-depth closure for a currently unreachable internal bypass, not evidence of
  an observed external exploit.
- The complete end-to-end managed effect requires a signed Phase 614B integrity fixture and the
  broader Phase 614B aggregate gates; this focused fix does not claim those pending results.
- This document must not be read as evidence that every missing tuple is invalid: FIX-762 defines
  the durable managed-versus-unmanaged classifier for the shared FinalSubmit boundary.
- It does not enable managed Browser execution, change flags, deploy, or perform an effect.
