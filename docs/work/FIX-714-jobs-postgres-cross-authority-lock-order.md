# FIX-714: PostgreSQL Cross-Authority Lock Order Could Deadlock

> **Codex preflight:** Load `$bluey-ops` before diagnosis, implementation, or review and reconcile
> this record with Round 614 and the final branch diff.

**Status:** Implemented; application/execution and verifier lifecycle ordering are source-reviewed
and covered by focused regressions

## Issue

Jobs transactions acquired operational-hold (`H`), managed-release (`M`), ATS-certification (`ATS`),
and discovery/application (`D`) authority in different orders. A source-verification terminal path,
execution-lease claim, local-runner claim/submit, queue admission, or final-effect path could
therefore wait on another path that held a later authority and was waiting for an earlier one.

## Root Cause

The authorities were added in separate phases and each call path resolved the authority closest to
its own effect. There was no shared, enforced PostgreSQL order across the combined transaction.
Managed-registry readers also used an exclusive advisory lock, which made safe prelocking harder
and could invite a shared-to-exclusive upgrade if a reader were introduced locally.

## Fix Summary

- Use the canonical PostgreSQL order `H -> M -> ATS -> D` for effect-capable transactions.
- Add one shared global managed-registry advisory read fence; keep registry writers exclusive.
- Prelock current managed authority before ATS or discovery/application rows.
- Apply the order to current execution authorization, execution-lease claim/finalization,
  local-runner claim/submit, application save/queue admission, discovery scheduling, and final
  employer-facing effects.
- Preserve the current reservation/running-transition subset order `H -> M -> D` and its
  original-source/discovery recheck. It does not independently resolve ATS today; composed
  reservation ATS/integrity authority remains a Phase 614B prerequisite.
- Apply `H -> M -> D -> assignment` to PostgreSQL verifier heartbeat, first terminal publication,
  and changed-byte replay quarantine. Resolve the immutable account/job identity without a row
  lock, take `D`, then load and lock the exact assignment with account/job predicates and revalidate
  the locked row. Exact terminal replay remains a read-only recovery check and takes no assignment
  lock.
- Make the managed-cloud claim and final-Submit combined
  `H -> exclusive M -> ATS -> fleet` prelock the first protected operation. Keep the unmanaged
  branch on explicit `H -> shared M -> ATS`, then acquire account `D` after either branch prelock.
  Do not duplicate operational-hold or ATS acquisition around the managed helper.
- Add a static regression that pins the call-path order and focused behavioral tests for the
  affected effect boundaries.

## Verification

Observed current-source checkpoints:

```text
Static PostgreSQL lock-order regressions      3 / 3
Protected-admission PostgreSQL lock order     1 / 1
Rust original-source authority               25 / 25; normal-parallel twice
Execution-lease regressions                  13 / 13
Local-run regressions                         4 / 4
Reservation source/discovery recheck           1 / 1; ATS composition parked for Phase 614B
Final-source cargo check --all-targets        passed (34.06s)
Final-source strict Clippy, -D warnings       passed (49.41s)
```

The reviewed paths are `managed_cloud_release_authority.rs`, `execution_authority.rs`,
`execution_leases.rs`, `local_runner.rs`, `applications.rs`, `eligibility.rs`, and their focused
tests. The review did not infer a separate verifier/discovery deadlock where the existing global
managed fence already serializes the paths.

Late review subsequently found assignment-row-before-`D` ordering in PostgreSQL verifier heartbeat,
first terminal publication, and changed-byte replay quarantine. FIX-719 closes that finding. Its
focused static regression pins `H -> M -> D -> assignment`, while public SQLite lifecycle tests
exercise the equivalent heartbeat, terminal, replay, authority-loss, quarantine, and stale-fence
behavior. Those local tests are not represented as live PostgreSQL contention evidence.

The pre-final full-library baseline then exposed a separate managed-cloud prelock regression.
FIX-723 closes it: managed claim and final Submit now begin with the combined
`H -> exclusive M -> ATS -> fleet` helper; unmanaged execution retains
`H -> shared M -> ATS`; both take `D` afterward. Its focused static regression passed 1/1. This
source-level proof does not replace live PostgreSQL contention and interruption evidence. Global
fmt, scoped diff, and post-fix server all-target check/strict Clippy passed.

No authorized `BLUEY_TEST_POSTGRES_URL` was available. Disposable/hosted PostgreSQL contention and
failure injection remain required; compilation and the static guard do not prove deadlock freedom
under live concurrency.

## Limits

This fix changes transaction ordering only. It does not activate a worker, deploy a release, enable
provider writes, or substitute for the hosted PostgreSQL/network-fault gate.
