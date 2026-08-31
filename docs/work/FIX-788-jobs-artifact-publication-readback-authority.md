# FIX-788: Managed-Cloud Promotion Used A Local Copy As Publication Evidence

> **Codex preflight:** Loaded `$bluey-ops` and reconciled this defect against
> Round 611, Round 624, the current release workflow and verifier, and the local
> release audit. The SSD archive was not used.

**Status:** Source correction expanded after fresh guard review; another
independent delta review, exact-tip hosted checks, protected environment
configuration, and real remote publication remain pending

## Issue

The production-shaped `promote` operation never contacted an artifact registry
or immutable static store. It copied candidate payloads into local
`published/` and `readback/` directories, compared hashes on the same runner,
and emitted a `local_fixture` promotion receipt. Candidate OCI references also
used the uncontrolled `ghcr.io/bluey` namespace.

The same candidate could claim test evidence from a two-line self-test log
without proving that full Jobs CI and Observability passed at the exact source
commit. The workflow generated valid internal Bluey file SBOM/provenance
documents but could be read as claiming OCI-native attestations even though
Buildx explicitly disabled both.

## Root Cause

Phase 611 correctly stopped before external registry mutation and implemented a
local contract fixture. The later workflow retained that fixture in the only
promotion path instead of inserting a distinct protected publication stage.
Artifact destination ownership, Packages permission, exact-tip CI evidence, and
remote read-back were left as prose-only external gates.

After candidate capture was corrected, both validators checked the two pipelines
and their cleanup only in the broad candidate section. They did not bind
`pipefail` to the exact named step that owns both producer pipelines, so a
future edit to `set -eu` in that step could pass while unrelated workflow steps
still supplied many matching strict-shell lines.

## Correction

- Add a provider-neutral canonical publication plan and receipt bound to the
  exact authorized candidate.
- Derive candidate refs from protected repository variables rather than the
  invalid `ghcr.io/bluey` literal.
- Add a protected GHCR adapter using a pinned ORAS setup action, job-local
  `packages: write`, stored OCI-layout extraction, immutable digest resolution,
  exact portal artifact publication, fresh-layout pull, normalized verifier
  tar creation, and full remote read-back.
- Bind the runner native addon from the candidate inventory and prove the same
  runner OCI manifest is read back.
- Require promotion to download the protected publication receipt, pull all
  artifacts independently by immutable digest with `packages: read`, and pass
  the read-back into the promotion verifier.
- Require promotion to query the selected Actions run and complete job list,
  prove the successful exact-source `publish` job and receipt-upload step, and
  persist normalized run evidence that truthfully does not attest environment
  settings.
- Distinguish signed delivery refs from observed registry refs and require
  release-v2 portal-readback evidence to bind the exact signed HTTPS URL.
- Replace release-v2 synthetic evidence with the normalized successful Jobs CI
  and Observability default-branch push runs at the candidate source commit.
  Retain the candidate self-test only as a supplemental hash.
- Take the trusted default branch separately from the GitHub event because real
  Actions-run payloads omit it; persist it in evidence and bind every run.
- Stop before GHCR login/write unless plan source commit, repository, and
  repository ID, and trusted current default branch equal the exact GitHub
  runtime values.
- Before promotion registry login or immutable-ref consumption, validate the
  authorized candidate, exact canonical plan/publication-set bytes, current
  runtime identity, explicitly selected Actions run attempt, publish job, and
  successful receipt upload. Emit one bounded canonical preflight and derive
  registry refs only from it rather than directly requiring downloaded receipt
  JSON.
- Fetch and hash-bind the raw portal OCI manifest, require exact artifact and
  config types plus one exact portal-tar layer. Cap it at 32 KiB and validate it
  immediately before `oras pull`; only then inspect the exact pulled tar and
  closed read-back tree, rejecting extra paths including empty directories.
- Derive one canonical RFC3339 portal-manifest created annotation from the
  signed portal `sourceDateEpoch`, store it in the exact plan, pass it
  explicitly to ORAS, and require exact read-back equality so retries are
  byte-identical instead of wall-clock dependent.
- Preserve legacy builder-policy v1 bytes for historical v1/v2 verification,
  emit policy v2 for new release-v2 candidates, and require policy v2 before
  production publication.
- Record internal Bluey SBOM/provenance as present and OCI-native attestations as
  `not_generated`.
- Serialize all candidate/authorize/publish/promote/rollback operations under
  one non-cancelling release lock.
- Declare `actions: read` and `contents: read` explicitly on every release job,
  while keeping Packages access absent from read-only jobs, write-only on
  publication, and read-only on promotion.
- Reject `local_fixture` anywhere production publication or promotion evidence
  is required.
- Qualify every publication receipt name with both workflow run ID and attempt;
  capture exact-attempt run/jobs and run-scoped artifact API bodies through
  atomic 128 KiB/1 MiB/1 MiB fixed-kind caps; require exactly one unexpired
  receipt with matching repository/source and immutable ID/digest/size; and
  download only by the validated artifact ID. Bind run attempt and artifact
  identity through the publication set, preflight, and promotion.
- Replace both candidate `gh api > final.json` writes with producer-to-fixed
  `run` capture pipelines. Each uses the same 128 KiB atomic ceiling and
  explicitly removes final and partial output if either the producer or capture
  fails; direct redirection and per-run capture bypasses are guarded by both
  workflow validators.
- Make both validators isolate the exact named candidate capture step, require
  `set -euo pipefail` immediately after its shell block begins, and require both
  bounded pipelines inside that same step. Each test layer mutates only that
  live step to `set -eu` and rejects it while unrelated strict steps remain.
- Replace both unbounded ORAS manifest-output writes with an atomic streaming
  32 KiB capture. A 32,768-byte body is admitted; a 32,769-byte body aborts
  without a final or partial output before the existing manifest validator and
  pull.
- Emit remote promotion, rollback, and rollback-set audiences/schemas v2.
  Retain strict historical local-fixture promotion/rollback v1 validation only
  for audit/readback, reject mixed tree generations, and expose no production
  v1 creation path.

## Containment

No workflow was dispatched. No package, environment, budget, credential,
registry object, static object, deployment, activation, flag, application,
message, or customer state was created or changed. The implementation uses
only the automatic short-lived workflow token in the dormant publication job;
it adds no repository secret or long-lived credential.

## Verification

```text
managed-cloud release gate                         29 / 29 passed
  includes exact-tip CI evidence positive/negative coverage
  includes remote OCI/portal/native read-back coverage
  includes exact-attempt artifact selection, all API/manifest byte caps,
  no-partial cleanup, real promotion/rollback v2 create/readback, strict
  historical v1 readback, mixed-generation rejection, runtime identity,
  branch, portal-manifest/tree, historical-policy, and exact job-local
  permission/namespace/read-back mutation coverage, including removal of
  pipefail from only the named live candidate capture step
managed-cloud workflow contract                    passed
independent Jobs CI guard self-tests               passed
Node syntax checks                                 passed
another independent source re-review               pending
```

The remote ORAS commands were not executed because this source phase has no
publication authority and current external configuration is incomplete.

## Remaining Gates

- exact-tip hosted Jobs and observability runs after integration;
- protected environments, reviewers, branch policy, and self-review settings;
- approved Actions/Packages budgets and repository package access;
- controlled namespace and portal-origin variable read-back;
- first real publish and an independent immutable read-back;
- first read-back of the signed portal HTTPS URL using evidence v2;
- registry-native SBOM/provenance attestations if they become policy-required;
- static serving, dark deployment, runtime/rootfs, hosted service, canary,
  activation, cohort, kill-switch, and rollback evidence.

The legacy manifest field `publishedAtMs` is still the deterministic source
commit epoch, not registry publication time. This fix does not reinterpret the
historical schema; publication authority uses the new truthful `attestedAtMs`
field. A successor manifest schema should rename the legacy field.
