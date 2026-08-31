# REVIEW: PHASE-624 — Jobs Managed Source-Verification Release V2

> **Codex preflight:** Load `$bluey-ops` and reconcile the review against Round 624, FIX-783, the
> implementation record, the final branch diff, and the retained Phase 614/614B authority.

**Commit range:** `b3d0c79c..8858abe28d590b1f234481ace3abba24be076e04`

**Implementation commit:** `02648f09ceb1790ef34d21632baac00c06c37e17`

**Reviewer:** Independent line-by-line review and correction rereview complete

**Date:** 2026-08-31

**Status:** 🟢 bounded source accepted; production release remains no-go

## Per-Task Review

### Release-v2 candidate and runtime measurement

| Field | Value |
|-------|-------|
| Files | Managed-cloud workflow, workflows Dockerfile, release gate and tests |
| Verdict | 🟢 accept |

**Review requirements:**

- candidate contracts and descriptor are exact v2;
- source verification is true, while direct/global discovery are false;
- workflows measurement contains the exact sorted verifier/gateway/worker roles;
- the compiled verifier entrypoint and source-verification protocol are bound to stored bytes; and
- release v1 remains fail closed rather than being weakened.
- only the exact nonempty regular verifier entrypoint is admitted by OCI inspection, while v1
  assembly rejects a three-role/verifier image.

### Runtime, lease, readiness, and readback composition

| Field | Value |
|-------|-------|
| Files | Existing Phase 614 runtime/lease authority plus new v2 executable regressions and runbook |
| Verdict | 🟢 accept |

**Review requirements:**

- the separate verifier process uses an exact role-scoped one-time grant;
- a successful lease poll precedes continued readiness heartbeat reporting;
- assignment and terminal publication retain activation/manifest/runtime/epoch/fence checks;
- exact terminal replay remains read-only and changed bytes quarantine; and
- activation evidence rejects a missing `original-source-verifier-readiness` check.

### Containment and production boundary

| Field | Value |
|-------|-------|
| Files | CI guards, operations/environment docs, Round/IMPL/FIX/CHANGELOG |
| Verdict | 🟢 accept |

**Review requirements:**

- independent guards reject v1 fallback, role omission, and discovery enablement;
- all release-evidence Node commands follow exact pinned Node 22.23.2 setup and assertion;
- all five jobs are default-branch-only before their steps execute;
- no new standalone environment flag or configured runtime identity exists;
- all production/provider-write flags remain off; and
- no workflow dispatch, provider access, deployment, cohort, or production mutation occurred.

## Build & Test Verification

```text
PASS  managed-cloud release gate: 20 / 20
PASS  managed-cloud workflow contract
PASS  Jobs CI guard self-tests
PASS  workflow YAML and Node/default-branch structural mutation coverage
PASS  real OCI exact-entrypoint, renamed/alias rejection, and v1 assembly containment
PASS  automation verifier/runtime suites: 40 / 40
PASS  workflows verifier lifecycle suite: 9 / 9
PASS  automation and workflows builds/typechecks
PASS  explicit v2 contracts: SQLite 057 / PostgreSQL 035 / source_verification v1
STOP  focused Rust rerun: interrupted during compile at 18 GiB free / 96% shared-disk use;
      no test failure, and this diff changes no Rust or migration source
PASS  workflow syntax, YAML parse, formatting, and diff checks
PASS  independent correction rereview: no remaining P0-P3 findings
```

## Independent Review Findings

Three P1 blockers were confirmed and corrected locally:

1. Ambient runner Node was outside release authority. All five jobs now install the existing
   full-SHA-pinned setup-node action for exact 22.23.2 and assert `node --version` before evidence.
2. Caller-selected workflow refs were not fenced. Candidate, verify, authorize, promote, and
   rollback now require a branch ref equal to the repository default branch at job scope.
3. OCI inspection unconditionally rejected the v2 verifier path. It now admits only the exact
   nonempty regular workflows entrypoint; aliases and renamed paths fail, and v1 assembly is
   symmetrically denied.

The corrections have local executable and mutation evidence. Independent rereview found no
remaining P0-P3 issue. Exact-tip hosted CI remains mandatory.

## Overall Verdict

🟢 **SOURCE ACCEPT; PRODUCTION NO-GO** — The source change is bounded to the existing v2 contract,
and independent review found no remaining P0-P3 issue. Exact-tip hosted Rust/Docker evidence and
the external protected-environment controls remain mandatory. This verdict does not authorize a
candidate run, signature, activation, deployment, provider canary, customer cohort, or flag change.

## Follow-ups For Release

- Require exact-tip hosted CI and exact workflows/runner Docker image evidence.
- Retain the configured digest-pinned `BLUEY_JOBS_NODE_IMAGE` variable and prove its value in the
  candidate evidence; configuring it alone is not a candidate run.
- Complete protected signing, immutable readback, hosted database/Temporal/runtime, provider,
  monitoring, kill-switch, cohort, and rollback gates.
- Keep direct/global discovery disabled until the separate V3 authority phase is reviewed.
