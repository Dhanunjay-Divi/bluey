# REVIEW: PHASE-609 - Jobs Workflow Command Authority

> **Codex preflight:** Load `$bluey-ops` before final review and verify this record against the
> frozen source snapshot or later byte-equivalent commit range. No SSD/archive or external system
> is required for the local source review.

**Reviewed snapshot:** Frozen Phase 609 working tree at the final local verification checkpoint
before commit. Any later code change invalidates this verdict and requires review again.

**Reviewer:** Independent database, Temporal command-path, and runner reviewers; consolidated
Phase 609 source review

**Date:** 2026-08-13

**Review status:** 🟢 ACCEPT - no blockers or minor findings

## Per-Task Review

### Database-first start/resume authority

| Field | Value |
|-------|-------|
| Files | Paired migrations, `workflow_commands.rs`, Jobs API routes, auth, portal projection |
| Verdict | 🟢 independent database review accepted |

**Review requirements:**

- Prove the local business transition and one immutable encrypted command commit atomically.
- Prove replay with changed semantics fails before another charge, attempt, or workflow identity.
- Prove no request-handler gateway call or compensating ambiguity path remains.
- Review SQLite transaction behavior and PostgreSQL locks/`SKIP LOCKED` parity line by line.

**Addressed before verdict:** Fresh runner failure/ambiguity finalization now proves that no exact
matching public intervention remains open while replay lookup stays first. SQLite/PostgreSQL schema
and constraint inventories were reported in parity.

### Dispatcher and exact Temporal protocol v2

| Field | Value |
|-------|-------|
| Files | `jobs_workflow_dispatch.rs`, gateway service, contracts, worker, failure converter, tests |
| Verdict | 🟢 independent Temporal command-path review accepted |

**Review requirements:**

- Prove request-start evidence precedes I/O and every ambiguous retry is byte/identity stable.
- Prove exact conflict identity, real first execution run ID, closed-workflow rejection, and
  intervention-bound Update replay.
- Prove bounded HTTP behavior, exact headers/bodies/statuses, and permanent versus transient
  failure classification.
- Prove deterministic workflow/failure surfaces contain only opaque authority.

**Addressed before verdict:** The running-Describe to closed-execution race now recovers only the
exact Update ID; timeout-only absence remains `delivery_unknown`. Workflow activities trust only
exact closed Jobs API errors, and the Rust dispatcher/TypeScript gateway enforce one trimmed
32-to-8,192-byte RFC 6750 bearer-token grammar.

### Two-phase intervention and terminal evidence

| Field | Value |
|-------|-------|
| Files | Workflow activities/lifecycle, Jobs prepare/publish/finalize routes and database module |
| Verdict | 🟢 independent runner review accepted; no blockers or minor findings |

**Review requirements:**

- Prove only the exact prepared intervention becomes customer-visible and resumable.
- Prove timeout, limit, runner failure, and runner ambiguity require their exact allowed open-ID
  relationship.
- Prove trusted submitted receipts mark workflow execution terminal across all replay orderings and
  cannot be downgraded by later cleanup failure.

**Addressed before verdict:** Runner result receipts now bind the exact request ID and complete run
identity. Minimal encrypted ambiguity authority must persist before reply, restart recovery checks
durable submitted/failed results before replay, and post-commit browser cleanup retries 401/403
credential drift without downgrading canonical state. Startup recovery proves the profile scope
derived from the frozen account/application identity before persistence; an exact committed
intervention may outlive ordinary checkpoint expiry by first trying current authority restore.
Only exact `lease_unavailable` on that expired claim may fall back to old token/fence
reconciliation/removal; every other restore or reconciliation error fails closed.

### Cleanup and account-deletion boundary

| Field | Value |
|-------|-------|
| Files | Unimported cleanup-service library/tests plus cleanup schema/state scaffolding |
| Verdict | 🟢 accept only the unregistered/fail-closed exclusion boundary |

**Current boundary:**

- `BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED` remains `0`.
- The gateway does not import the cleanup library or register `/workflow-cleanup`; environment
  changes cannot enable that route.
- Authenticated legacy Temporal inventory/drain evidence is not implemented.
- The database cleanup generation intentionally cannot complete.
- No cleanup dispatcher or account-deletion dependency is accepted by this batch.
- Hosted Temporal deletion, visibility, retention, and KMS evidence does not exist locally.

This verdict rejects any broader cleanup or erasure claim. It makes no finding that the unimported
cleanup library, full cleanup lease/fence workflow, account deletion, or hosted erasure is complete.

## Cross-Task Findings

- The accepted scope is durable start/resume authority, exact Temporal v2, two-phase intervention
  binding, and terminal submitted/ambiguity evidence only.
- Workflow-command dispatch, cleanup, and customer cloud distribution are independent gates and all
  remain disabled.
- Cleanup is stronger than flag-disabled in this snapshot: the gateway has no cleanup import or
  route registration, so its environment cannot expose the Phase 610 scaffold.
- The browser-delivered portal remains the launch product; installed Bluey Browser work is parked.
- Account deletion and authenticated legacy workflow inventory remain required follow-up work, not
  minor review nits.

## Build & Test Verification

Frozen Jobs aggregate:

```text
Automation                                      644 tests / 35 files passed
Browser                                         219 tests / 34 files passed
Runner                                          293 tests / 32 files passed
Workflows                                       257 tests / 12 files passed
Portal                                          271 tests / 20 files passed
Aggregate                                     1,684 tests / 133 files passed
All five Jobs workspace typechecks              passed
All five Jobs workspace builds                  passed
```

The final Rust all-target gate passed formatting, strict all-target Clippy, and all-target tests:
1,263 library tests, 101 `integration_e2e` tests, one main test, one `connectinfo` test, one migration
test, two GDPR tests, two runner-plan tests, and one usage-schema test. Runner/workflows full tests,
typechecks, builds, and scoped diff checks are green.

Independent runner verification also passed the focused four-file recovery matrix (91/91), full
runner suite (293/293 across 32 files), typecheck, build, and scoped diff hygiene.

Recorded green evidence:

- [x] Full aggregate Jobs typecheck/build/tests
- [x] SQLite/PostgreSQL schema parity and focused state-machine tests
- [x] Privacy/source scans and generated-output checks
- [x] Fresh `git -P diff --check` before this documentation update
- [x] Final Rust formatting, strict all-target Clippy, and complete all-target tests
- [x] Independent database and Temporal command-path acceptance
- [x] Independent runner acceptance with no blockers or minor findings
- [x] Fresh full-tree diff hygiene after final documentation edits

The source was reviewed before its logical commits, so this record does not invent a commit range.
The commit operation must preserve this byte-equivalent reviewed source; any code edit requires a
new verification and review pass.

## Overall Verdict

🟢 **ACCEPT.** Phase 609's bounded local-source scope is accepted with no blockers or minor
findings. It is ready for intentional commits and a draft PR against its Phase 608 predecessor.

This local-source verdict does not authorize deployment, a Temporal-v1 drain, workflow cleanup,
account deletion erasure, a production canary, or any distribution flag.

## Follow-ups for Next Batch

- Phase 610 authenticated legacy Temporal inventory/drain and durable cleanup dispatch.
- Exact account-deletion integration with cleanup-generation recheck and hard-delete fencing.
- Hosted Temporal/PostgreSQL/network fault evidence and production rollout authority.
