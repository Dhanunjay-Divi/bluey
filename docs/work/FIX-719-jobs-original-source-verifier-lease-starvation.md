# FIX-719: Original-Source Verifier Lease Head Could Starve Valid Tail Work

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this fix against the current Phase 614
> source checkpoint, Round 614, and the canonical PostgreSQL lock order. The SSD archive was not
> used.

**Status:** Implemented; focused scheduler, lifecycle, replay, and PostgreSQL lock-order regressions
green

## Issue

`lease_original_source_verification` could choose the oldest time-eligible assignment and discover
only afterward that its current managed/source/subject authority was invalid or that it was under an
operational hold. Rolling that transaction back let the same head assignment win again, so invalid
or held work could indefinitely block a valid tail assignment.

The same path also needed complete attempt audit and lock-order semantics: an expired active attempt
under a hold had to record `lease_expired` before hold backoff, a still-active hold could not emit
repeated false `released` events, and PostgreSQL could not take an assignment row lock before the
account discovery fence (`D`).

## Root Cause

Candidate selection, current-authority recheck, hold handling, and assignment mutation were treated
as one winner-or-rollback operation. The first invalid/held row remained immediately eligible after
rollback. A naïve unbounded candidate walk would avoid that row but violate the scheduler's
bounded-lock contract, while a simple fixed scan window could still let repeated held work consume
the window forever.

PostgreSQL enumeration also coupled candidate selection to row locking before the per-account
discovery authority was known, creating an `assignment -> D` order contrary to the canonical
`H -> M -> D -> assignment` lease path.

Late review found the same inversion outside lease selection: public PostgreSQL heartbeat, first
terminal publication, and changed-byte replay quarantine acquired the assignment row before
resolving account `D`. It also found that the original 19 focused tests did not invoke public
heartbeat, complete, fail, or terminal replay entrypoints.

## Fix Summary

- Examine one normal scan window per lease call, capped at 32 candidates and ordered by
  `(next_attempt_at_ms, created_at_ms, assignment_id)`. Validate monotonic cursor movement inside
  that window; do not run an unbounded keyset loop.
- Convert current managed/source/subject failures into atomic typed supersession and continue within
  the normal scan window. Reasons are `managed_authority_revoked`, `managed_runtime_revoked`,
  `source_untrusted`, `subject_revoked`, and `subject_changed`.
- Atomically move a newly held candidate to `retry_wait`, set
  `last_error_code=operational_hold`, set the next database-time check to `now+60s`, append one
  typed `released:operational_hold` event, and do not increment the attempt count. The persisted
  backoff lets a later lease call reach valid tail work.
- Reconcile at most eight due held assignments in a separate budget. A still-active hold only
  rotates its next due time with no new event. A released hold returns to `pending` with one
  `released:operational_hold_released` event. A candidate that became invalid is superseded.
- If the held candidate owns an expired active attempt, append the exact old-attempt
  `lease_expired` event before the hold-backoff event and clear the old lease/fence material.
- Preserve strict public runtime/release fencing while composing the scheduler through the public
  SQLite lease entrypoint.
- In PostgreSQL, take `H`, then exclusive global `M`; enumerate the normal scan window and separate
  due-hold budget without a row lock; take account `D`; then reload and lock the exact assignment
  `FOR UPDATE` before the CAS. Normal and hold paths therefore have hard 32+8 lock budgets and order
  `H -> M -> D -> assignment`.
- In PostgreSQL heartbeat, first terminal publication, and conflicting replay quarantine, resolve
  account/job identity without a row lock, take `H -> M -> D`, then lock the exact assignment and
  revalidate its identity and authority. Exact terminal replay is a read-only recovery lookup.
- Add public SQLite lifecycle coverage for heartbeat, positive completion, retryable failure,
  byte-identical terminal replay, replay after later runtime revocation, denial of a new request,
  changed-byte quarantine, reclaimed-lease stale fences, and publication-time hold/source/runtime
  denial.

This fix does not add lifecycle aliases. The exact stored assignment states remain `pending`,
`leased`, `retry_wait`, `idle`, `quarantined`, `superseded`, and `cancelled`. Lease expiration is an
immutable `lease_expired` event followed by retry/reclaim or hold backoff; it is not a stored
`expired` state. Revocation and authority loss map to typed `superseded`, `cancelled`, or
`quarantined` transitions rather than a stored `revoked` state.

## Files Modified

| File                                                                   | Change                                                                                                                                                                                          |
| ---------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `server/src/db/jobs/original_source_verification.rs`                   | Bounded normal scan, persisted typed hold backoff/supersession, separate hold reconciliation, expired-attempt audit, D-before-row-lock PostgreSQL composition, and public lifecycle regressions |
| `server/src/db/jobs/managed_cloud_release_authority.rs`                | Extract strict-v2 verifier runtime fixture under `cfg(test)` for public lease composition tests                                                                                                 |
| `docs/rounds/ROUND-614-JOBS-ORIGINAL-SOURCE-VERIFICATION-AUTHORITY.md` | Reconcile exact stored assignment states and bounded scheduler semantics                                                                                                                        |
| Phase 614 IMPL/REVIEW/CHANGELOG                                        | Record focused lifecycle evidence and the remaining external boundary                                                                                                                           |

## Edge Cases Handled

- 33 held assignments ahead of one valid tail assignment: the first call defers exactly 32 and
  returns `None`; the second call reaches the valid 34th tail.
- a paused/degraded source or revoked/changed subject at the head is typed `superseded` and does not
  starve a valid second candidate;
- an expired active attempt under a hold records `lease_expired` for the exact old attempt before
  the one-time hold-backoff event;
- a still-active hold recheck emits no duplicate `released` event and does not consume the normal
  32-candidate budget;
- actual hold release returns the assignment to `pending` and makes it leasable again;
- attempt count and fence do not advance merely because a candidate entered hold backoff;
- equal scheduler timestamps are ordered by `created_at_ms`, then `assignment_id`; and
- PostgreSQL enumeration races are resolved by D-before-exact-row-lock reload/CAS.

## How To Test

Observed after the lifecycle and lock-order correction:

```text
Original-source verification module                         25 / 25 in four owner/reviewer runs (59.08s-71.89s)
Public SQLite lifecycle/replay subset                        6 / 6 owner and reviewer (23.04s, 27.41s)
PostgreSQL lifecycle static lock order                       1 / 1
Public strict-runtime 33-held-prefix/valid-tail regression   1 / 1 (3.73s final-source rerun)
Strict runtime fencing/revocation regression                 1 / 1 (2.12s)
cargo fmt --all --check                                      passed
cargo check --all-targets                                    passed owner and reviewer
cargo clippy --all-targets -- -D warnings                    passed owner and reviewer
Live PostgreSQL lease/contention regression                  self-skipped; URL absent
```

The public scheduler regression asserts the exact two-call 32/34 behavior, one-time markers/events,
expired-before-release event order, no event on a later still-held recheck, and recovery after the
real hold release. The added lifecycle regressions exercise public heartbeat, complete, fail,
exact replay, changed-byte quarantine, stale fences, and publication-time authority loss. The
PostgreSQL order regression is static source evidence only.

The heavy scheduler/lifecycle path uses a canonical test-only five-minute heartbeat and
fifteen-minute activation/cohort/policy/portal/grant horizon, plus an exact public heartbeat
refresh. The explicit expiry test ages the heartbeat beyond both its signed TTL and the managed
database clock's second-rounding margin. This removes parallel wall-clock-load flakiness without
changing production timing or adding global test serialization.

## Known Limitations

- `BLUEY_TEST_POSTGRES_URL` was absent. PostgreSQL source/compile/static-order evidence is green,
  including scheduler, heartbeat, first-terminal, and conflict-quarantine order; live row-lock
  contention, interruption, and hosted behavior remain external and unclaimed.
- One bounded 32-candidate scan window per call intentionally trades single-poll tail latency for
  hard transaction/lock bounds. Fairness comes from persisted hold backoff and typed supersession
  across calls, not an unbounded in-call keyset walk.
- This fix does not operate the Round 615 scheduler fleet, budgets, SLOs, or alerts and does not
  activate source verification or any provider/customer effect.
