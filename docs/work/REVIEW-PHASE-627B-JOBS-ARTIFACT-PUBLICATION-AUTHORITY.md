# REVIEW: PHASE-627B — Jobs Artifact Publication Authority

> **Codex preflight:** `$bluey-ops` was loaded and reconciled against the
> current repository, local handoff, Phase 611 and Phase 624 authority records,
> and the uncommitted Phase 627B diff. The SSD archive was not used.

**Commit range:** `a6a7c3320fbd6efbc273cf85c1d6fe25ca565a40` plus the
uncommitted Phase 627B working-tree diff

**Reviewers:** independent read-only Codex source review, root audit, and
implementation-owner consolidation

**Date:** 2026-08-31

## Per-Task Review

### FIX-788 — Remote Publication And Read-Back Authority

| Field | Value |
|-------|-------|
| Files | Release workflow, gate/tests, CI guard, Operations, and authority records |
| Verdict | 🟡 latest guard correction implemented; another delta review pending |

**Resolved findings:**

- A P0 receipt-schema mismatch between the publication plan and set was fixed
  by sharing the canonical publication key contract and validating the complete
  plan before any remote-write step can run.
- A P1 provenance gap was fixed by binding promotion to the selected successful
  exact-source Actions run and explicit attempt, complete exact-attempt job
  list, successful `publish` job, successful receipt-upload step, and exactly
  one attempt-qualified artifact. Environment protection is truthfully
  recorded as not attested by that API evidence.
- A P1 portal-identity ambiguity was fixed by separating the signed HTTPS
  delivery reference from the observed immutable registry reference and by
  requiring release-v2 portal evidence to name the exact signed URL.
- A P2 timestamp naming issue is contained by documenting that legacy
  `publishedAtMs` is the deterministic source epoch, not registry publication
  time; Phase 627B uses the new truthful attestation timestamp instead.
- The root permission audit found that Actions read authority was inherited
  globally rather than frozen per job. The workflow now keeps only
  `contents: read` globally and declares exact job-local maps: Actions and
  contents read on all six jobs, Packages write only on publication, Packages
  read only on promotion, and no Packages access on the four read-only jobs.
- The delta review found one stale negative-test error matcher and incomplete
  permission mutations. The matcher now admits the strengthened fail-closed
  error path; both test layers mutate every expected permission field, reject
  Packages access on all four read-only jobs, and reject global narrowing or
  widening.
- Full final review rejected the prior verdict because Actions-run fixtures
  invented `repository.default_branch`, stored CI evidence did not bind one
  authoritative default branch, and the plan was not rebound to GitHub runtime
  identity before registry writes. The workflow now supplies the trusted event
  branch separately, persists and validates it, and runs an exact SHA,
  repository, and repository-ID preflight before registry login.
- The same review found portal and compatibility gaps. Both network pulls now
  hash-bind the compact raw OCI manifest, require exact manifest/artifact/config
  types and one exact portal-tar layer, and reject extra files or directories.
  Historical policy-v1 bytes remain verifiable, while new v2 policy with
  `publish` fenced is mandatory for production publication.
- Fresh full-diff review then rejected promotion's use of raw downloaded
  plan/set JSON before registry access. Promotion now proves the authorized
  candidate, exact canonical plan/set hashes, current repository identity and
  trusted default branch, candidate source commit through the selected Actions
  run and explicit attempt, exact-attempt publish job, successful receipt
  upload, and immutable artifact ID/digest/size into one bounded canonical
  preflight before receipt download, registry login, or ref consumption. Only
  that preflight supplies registry refs, and the final promotion consumes the
  same validated evidence.
- The same review found the portal manifest was validated too late and without
  a tight transport cap. Publication and promotion now cap it at 32 KiB and
  validate its ref/digest/schema/media/artifact/config/exact layer immediately
  after manifest fetch and before pull; post-pull validation separately proves
  the exact tar and closed output tree.
- ORAS's implicit creation time made retry manifests nondeterministic. The exact
  plan now stores a canonical RFC3339 whole-second value derived from signed
  portal `sourceDateEpoch`, the workflow passes it as an explicit annotation,
  and read-back requires equality. Tests prove wall-clock-separated retries
  retain one digest while a changed source epoch changes it.
- Publication runtime identity now binds the trusted current default branch as
  well as SHA/repository/repository ID, including slash-bearing branch coverage.
- The corrected promotion path captures run JSON through a fixed 128 KiB
  streaming cap and jobs/artifacts JSON through fixed 1 MiB caps, atomically
  removes partial output on overflow, selects exactly one artifact named with
  both run ID and attempt, and downloads it only by the validated immutable ID.
- Both ORAS raw-manifest reads now pass stdout through the fixed 32 KiB atomic
  capture before the existing manifest validator and before pull. Tests prove
  32,768 bytes pass and 32,769 bytes fail without final or partial output.
- Final independent delta review found one P2 in candidate evidence capture:
  both exact-tip run responses were written by direct shell redirection. They
  now use the same fixed 128 KiB atomic `run` capture, `pipefail`, and explicit
  final-plus-partial cleanup on producer or capture failure. Both validators
  independently mutate each run to reject direct redirection, kind bypass, and
  missing cleanup; an executable regression proves a producer can finish a
  capture and still fail without leaving either output.
- Follow-up review confirmed the live step was strict but found both validators
  accepted removing only its `pipefail` because their pipeline checks were
  candidate-section-wide. They now isolate the exact named capture step,
  require `set -euo pipefail` at its shell entry and both bounded pipelines in
  that same step, and independently prove a step-only `set -eu` mutation fails
  while unrelated strict steps remain.
- Remote production promotion and rollback creation now emits/requires v2.
  Strict historical-v1 promotion and rollback readers remain audit/read-back
  only; actual canonical fixture tests prove historical v1 validation, remote
  v2 create-to-validate, and mixed/crossed generation rejection.

Another fresh independent delta review is pending; the former clean verdict is
not authority for this corrected diff.

## Cross-Task Findings

- `local_fixture` remains available only to strict historical-v1 audit/readback
  validation and explicit local tests; production publication, promotion, and
  rollback creation cannot emit or accept it.
- The checked-in workflow requests protected environments and narrowly scoped
  token permissions but does not claim that environments, reviewers, branch
  policies, budgets, packages, variables, or access are configured.
- Synthetic candidate evidence remains supplemental and cannot substitute for
  successful exact-tip default-branch Jobs CI and Observability runs.
- Internal Bluey file SBOM and recipe provenance are recorded as present;
  OCI-native SBOM/provenance remain truthfully `not_generated`.
- No workflow, registry, package, provider, deployment, activation, flag,
  application, message, or customer state was mutated during this phase.

## Build & Test Verification

```text
Node syntax checks                                      PASS
managed-cloud release gate                             29 / 29 passed
managed-cloud workflow contract                        PASS
independent Jobs CI guard self-tests                   PASS
workflow YAML parse                                    PASS
git diff --check                                       PASS
```

The focused suite includes positive and negative evidence coverage plus
mutations for global permission drift, every marked job's expected permission
fields, and forbidden Packages access on the four read-only jobs. The
independent CI guard parses the six permission maps and proves the same
mutations fail.
It separately enforces both candidate run-response pipelines and rejects
direct-file redirection, a non-`run` capture kind, or missing cleanup for
either Jobs CI or Observability evidence. Both validator layers also mutate
only the exact live capture step from `set -euo pipefail` to `set -eu` and
reject it without counting unrelated strict-shell directives.
It also uses real API-shaped run fixtures without `repository.default_branch`
or a synthetic job `head_branch`, binds one explicit run attempt and one
attempt-qualified immutable artifact, mutates stored branch bindings and every
pre-write runtime identity field, rejects raw receipt-derived refs and late
preflights, exercises fixed run/jobs/artifacts transport caps plus the manifest
CLI and 32 KiB cap, mutates every portal layer identity field plus extra
layers/files/directories, proves deterministic retry identity, invokes actual
promotion and rollback create/validate paths for v2 and historical-v1 fixtures,
rejects mixed generations, and keeps legacy-policy verification separate from
publication admission.

No Docker build, ORAS network operation, GitHub workflow dispatch, registry
write/read, package mutation, deployment, or production smoke was performed.

## Overall Verdict

🟡 **PENDING ANOTHER FRESH DELTA REVIEW** — Focused corrections pass locally, but the
superseded clean verdict is not reused. This is not merge, publication,
deployment, activation, or production authority.

## Follow-ups for Release Integration

- Run aggregate review and exact-tip hosted Jobs CI after integration.
- Read back protected environment reviewer, branch, and self-review policy.
- Prove Actions/Packages budget and access plus controlled namespace and portal
  origin variables.
- Conduct the first authorized ORAS/GHCR publish and immutable read-back.
- Continue static serving, registry-attestation, dark deployment, hosted
  service, canary, cohort, kill-switch, activation-CAS, and rollback gates as
  separately reviewed phases.
