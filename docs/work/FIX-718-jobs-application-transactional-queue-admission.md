# FIX-718: Application Queue Admission Was Not Transactionally Revalidated

> **Codex preflight:** Load `$bluey-ops` before diagnosis, implementation, or review and reconcile
> this record with Round 614 and the final branch diff.

**Status:** Implemented; focused source evidence green, integration aggregate explicitly non-green

## Issue

Application state could be persisted as `queued` or `running` after an earlier eligibility check
without one same-transaction recheck of the current source head, managed execution authority,
Career Track, identity, resume, risk, ATS certification, operational holds, entitlements, runner
kind, and policy. Auto-submit preparation could also expose `queued` before its approved execution
snapshot was durably attached, leaving a crash-visible effect-capable intermediate.

## Root Cause

The application save path treated queue state as a projection of a prior decision. It did not own
the complete commit-time admission gate, and auto-submit packet persistence and approval promotion
were separate observable steps.

## Fix Summary

- Require the full queue/running admission authority inside the SQLite or PostgreSQL save
  transaction.
- Bind the exact current original-source head and managed authority, Track/identity/resume/risk/ATS
  state, holds, entitlement, policy, and the selected local/cloud runner kind.
- Acquire PostgreSQL authorities for application save/queue admission in canonical
  `H -> M -> ATS -> D` order.
- Persist an Auto-submit draft as `awaiting_review` with an explicit pending marker; atomically
  promote it to `queued` only when an `approved_execution` snapshot is added.
- Recheck source freshness at queue time and preserve Review-first-only behavior when independent
  employer/risk authority is absent.

## Verification

Observed checkpoints:

```text
Execution-lease regressions                    13 / 13
Local-run regressions                           4 / 4
Reservation source/discovery recheck             1 / 1; ATS composition parked for Phase 614B
Projection/effect regressions                   2 / 2
Submitted verified-runner finalization denial   1 / 1 (2.51s final-source rerun)
Application state/mode no-mutation              1 / 1 (2.33s final-source rerun)
Runner-plan Review-first matrix                 2 / 2
Final-source check                              passed (34.06s)
Final-source strict Clippy                      passed (49.41s)
Integration E2E                                86 passed / 22 failed / 108 total
```

Focused tests cover pending-approval persistence, current/stale source rechecks, runner-specific
reservation, and Review-first zero mutation. All 22 integration failures stop at the shared
`setup_execution_lease_run` approval with HTTP `409`, `Confirm the sponsorship answer before
Auto-submit.` That repeated positive-fixture authority gap keeps the aggregate non-green; the final
full `cargo test --all-targets` rerun remains pending against the final Phase 614 source.

The application save/queue and final-effect paths compose `H -> M -> ATS -> D`. The current
`reserve_application_attempt` and running-status transition take `H -> M -> D` and recheck
original-source/discovery authority, but do not independently re-resolve ATS. Phase 614B must add
the composed reservation ATS/integrity recheck before production-representative positive evidence;
the 1/1 reservation regression is not evidence that this missing composition already exists.
Later claim/final-effect gates still prevent an external effect, but stale authority at reservation
time may consume capacity until the composed recheck exists.

## Limits

This fix does not provide the missing independent employer-identity/scam-clear authority.
**Phase 614B — Signed Job Integrity Authority** owns that prerequisite. A Phase 614 hosted-ATS
snapshot remains Review-first and cannot queue or authorize an employer-facing effect by itself.

The plan matrix consequently proves entitlement/release observability and fail-closed approval and
queue behavior for Free/Pro/Cloud. It cannot exercise route-level payment/service/success branches
until a Phase 614B employer/risk authority fixture exists; unit tests retain availability mapping
coverage.
