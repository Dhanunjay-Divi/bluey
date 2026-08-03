# IMPL: Round 594 — Irreversible Submission Reconciliation

> **Codex preflight:** Loaded `$bluey-ops` and verified its memory against the
> current branch and production feature-flag state.

## Scope

**Does:** close the local/cloud `side_effect_unknown` state with a single,
fenced, auditable reconciliation contract and expose the owner action in the
Applications portal.

**Does NOT:** enable model generation, Browser distribution, mailbox sync, or
claim that any ATS has completed live employer certification.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `server/src/db/jobs/submission_reconciliation.rs` | Created | Atomic owner reconciliation |
| `server/src/db/jobs.rs` | Modified | Shared reconciliation contract |
| `server/src/db/jobs/customer_data.rs` | Modified | Trusted late receipt finalization |
| `server/src/api/jobs.rs` | Modified | Route and runner error/state semantics |
| `server/src/api/jobs_local_capability.rs` | Modified | Bounded late capability verification |
| `server/tests/integration_e2e.rs` | Modified | Local/cloud crash-result coverage |
| `jobs/portal/src/App.tsx` | Modified | Reconciliation orchestration |
| `jobs/portal/src/api.ts` | Modified | Typed API call |
| `jobs/portal/src/views/ApplicationsView.tsx` | Modified | Accessible owner confirmation |

## Build & Test

```text
Portal focused tests: 6 passed
Portal full suite: 89 passed
Portal strict typecheck: passed
Portal production build: passed
Jobs HTTP integration slice: 18 passed
Jobs Rust library suite: 262 passed
Rust strict Clippy: passed
Rust formatting: passed
git diff --check: passed
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| No production deployment | Employer-facing flags remain disabled until all launch gates pass |

## Known Follow-ups

- Continue Round 595 with generated application-kit and document-fidelity
  certification.
- Run the final cross-platform and production fault matrix before enabling any
  runner distribution flag.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from plan
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs
