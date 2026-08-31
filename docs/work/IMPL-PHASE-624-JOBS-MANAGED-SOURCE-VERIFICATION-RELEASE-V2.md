# IMPL: PHASE-624 — Jobs Managed Source-Verification Release V2

> **Codex preflight:** Loaded `$bluey-ops`, its release guidance, the designated worktree handoff,
> and the current Phase 611, 614/614B, 621, and 622 authority records. The SSD archive was not used.

**Status:** Bounded local source accepted; no commit, push, workflow dispatch, deployment, or flag
change

**Base commit:** `b3d0c79c`

**Branch:** `feat/phase-624-jobs-managed-source-verification-v2`

## Scope

**Does:**

- advance the checked-in managed-cloud candidate path from release v1 to the existing exact v2
  source-verification contract;
- bind the source-verification protocol, compiled entrypoint, workflows image measurement, runtime
  role, activation quorum, and canary/readback requirement already implemented by Phase 614;
- keep direct/global discovery false and preserve all current effect, provider, cohort, and
  production switches at their existing default-off values;
- add executable regression coverage and two independent structural guards;
- bind the release interpreter and selected source ref before signed-evidence processing; and
- document the separate rootless verifier-process launch boundary and stop conditions.

**Does NOT:**

- change the paired 057/035 schema or verifier lease/receipt semantics;
- implement direct/global discovery V3, source enrollment, scheduling, rights, or SLOs;
- contact a provider, run an application, send a message, access private competitor state, or
  introduce a provider write;
- dispatch a release workflow, sign/import/activate a release, open a cohort, deploy, or change a
  production flag; or
- edit `docs/reviews/`.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `.github/workflows/jobs-managed-cloud-release.yml` | Modified | Generate/build/assemble only release-v2 source-verification candidates |
| `jobs/workflows/Dockerfile` | Modified | Measure the exact three-role workflows runtime |
| `jobs/scripts/managed-cloud-release-gate.mjs` | Modified | Reject workflow v1 fallback and incomplete v2 configuration |
| `jobs/scripts/managed-cloud-release-gate.test.mjs` | Modified | Add v2 measurement, readiness/readback, and mutation tests |
| `jobs/scripts/ci-guards-self-test.mjs` | Modified | Add independent workflow/Docker v2 invariants |
| `ops/bluey-jobs.env.example` | Modified | Document protected per-role verifier configuration |
| `jobs/OPERATIONS.md` | Modified | Document launch, readiness, fencing, stop, and rollback rules |
| `CHANGELOG.md` | Modified | Record the bounded source result |
| `docs/rounds/ROUND-624-JOBS-MANAGED-SOURCE-VERIFICATION-RELEASE-V2.md` | Added | Freeze the phase contract |
| `docs/work/FIX-783-jobs-managed-source-verification-release-v2.md` | Added | Record root cause and correction |
| `docs/work/IMPL-PHASE-624-JOBS-MANAGED-SOURCE-VERIFICATION-RELEASE-V2.md` | Added | Record implementation and evidence |
| `docs/work/REVIEW-PHASE-624-JOBS-MANAGED-SOURCE-VERIFICATION-RELEASE-V2.md` | Added | Record independent review and release boundary |

## Authority Decisions

- Phase 611 v1 remains validated for historical stored bytes but cannot represent source
  verification.
- The current candidate workflow intentionally emits only v2. Rollback continues to use old stored
  authorized bytes without rebuilding them.
- The exact workflows image has one measurement containing all three sorted roles. Each deployed
  role still receives a different grant/token/instance/worker identity and runs as a separate
  process.
- Source-verifier readiness requires a successful dependency lease poll before continuing managed
  heartbeat reporting; activation evidence independently requires the named readiness canary.
- The existing server owns assignment time, generation, fence, runtime binding, receipt
  canonicalization, and head transition. This phase adds no worker-trusted shortcut.
- There is no standalone environment switch for source verification. The signed v2 activation,
  live exact role, cohort/account scope, source state, and operational holds compose authority.
- All five Node-using jobs use the full-SHA-pinned setup-node action, assert exactly `v22.23.2`,
  and do so before the first release-evidence Node command.
- Every operation is job-level default-branch-only. OCI inspection admits the verifier only at its
  exact nonempty regular path, while release-v1 assembly rejects its bytes and three-role claim.

## Build & Test

Observed so far on 2026-08-31:

```text
Managed-cloud release gate                              20 / 20 passed
Managed-cloud workflow contract                        passed
Jobs CI guard self-tests                               passed
Workflow YAML + default-branch/Node mutation guards     passed
Real OCI exact/rename/alias + v1 assembly regressions   passed
Automation verifier/runtime suites                     40 / 40 passed
Workflows verifier lifecycle suite                      9 / 9 passed
Automation build + typecheck                            passed
Workflows build + typecheck                             passed
Explicit v2 contract generation                         SQLite 057 / PostgreSQL 035 /
                                                        source_verification protocol v1
Focused Rust v2/runtime rerun                           stopped during compile at 18 GiB free /
                                                        96% shared-disk use; no test failure
Independent line-by-line and correction rereview      passed; no P0-P3 findings
```

The JavaScript package checks reused a dependency tree whose `package-lock.json` SHA-256 exactly
matched this worktree; the temporary link was removed after the run. The focused Rust command
reused the matching Phase 622 cache to avoid a new target tree, but another Phase 623 build was
simultaneously consuming shared capacity. The run was interrupted before tests rather than
crossing the storage stop line. No Phase 624-local Rust target was created, no Rust failure was
observed, and the unchanged server authority remains covered only by the retained reviewed Phase
614 evidence until exact-tip CI reruns it.

The originating release owner separately reported configuring and reading back the build-only
Actions variable `BLUEY_JOBS_NODE_IMAGE` as
`node:22.23.2-bookworm-slim@sha256:83f487e0a63425e5b4d146fb5e5be574bcbe1b7b843d3ebafdd95eaf7767a7e5`,
from Docker Official Image tag metadata updated 2026-08-25. This resolves the previously missing
candidate image input; it is not candidate, registry, activation, or deployment evidence.

No Docker command, workflow dispatch, provider request, or production mutation was performed.

## Deviations From Plan

| Deviation | Rationale |
|-----------|-----------|
| No new migration or server lease code | Paired SQLite 057/PostgreSQL 035 and the v2 runtime/lease/readback authority already exist and are reviewed; duplicating them would create a second authority path |
| No candidate workflow dispatch | This batch owns source configuration and tests only; signing, hosted runtime evidence, and promotion require separate release authorization |
| Focused Rust rerun stopped before tests | Shared disk reached the capacity stop line during an unrelated concurrent build; exact-tip CI must provide fresh Rust evidence |

## Known Follow-ups

- Run exact-tip CI and the exact Docker/Linux workflows and runner image gates.
- Attach hosted PostgreSQL/Temporal, runtime capacity, image/rootfs, provider-canary, monitoring,
  kill-switch, customer-cohort, and rollback evidence before activation.
- Implement direct/global discovery only through the separately reviewed V3 runtime/lease phase.

## Review Checklist

- [x] Files match the bounded release-v2 scope.
- [x] Direct/global discovery remain false.
- [x] Checked-in production/provider-write flags remain off.
- [x] No provider, release, deployment, or customer effect occurred.
- [x] Focused JavaScript tests/typechecks/guards are green on final source.
- [ ] Fresh focused Rust tests are green on exact source.
- [x] Independent review findings are recorded and corrected in source.
- [x] Independent rereview of the P1 corrections is attached.
- [ ] Exact-tip hosted and external release evidence is attached before activation.
