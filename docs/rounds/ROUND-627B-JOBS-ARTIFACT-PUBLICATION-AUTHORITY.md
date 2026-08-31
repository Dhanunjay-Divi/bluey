# Round 627B — Jobs Artifact Publication Authority

**Date:** 2026-08-31

**Branch:** `feat/phase-627b-jobs-artifact-publication-authority`

**Status:** Bounded source implementation corrected after fresh guard review;
another independent delta review pending; no registry write, deployment,
activation, customer admission, or production authority

> **Codex preflight:** Load `$bluey-ops` before implementation or review. Use
> the current repository and local handoff first. The SSD archive is only a
> fallback for one specifically missing historical fact.

## Goal

Replace the Phase 611 local-copy promotion adapter with a fail-closed production
publication contract. One authorized candidate must be published without a
rebuild, fetched again by immutable remote identity, and independently verified
before protected promotion can continue.

This round also removes the uncontrolled `ghcr.io/bluey` reference and prevents
the candidate's small release-contract self-test from standing in for the full
exact-tip CI and observability gates.

## Baseline Defects

The checked-in workflow previously had no registry publisher. Its promotion
step copied candidate files into sibling `published/` and `readback/`
directories on one Actions runner and emitted `adapter: local_fixture`. That
proved only a local filesystem copy, not package ownership, immutable remote
storage, remote read-back, or independent promotion evidence.

Candidate image references were hard-coded under `ghcr.io/bluey`, a namespace
the current repository does not control. The workflow requested no Packages
write permission and had no protected artifact-publication environment.

Release-v2 test evidence contained only a hash of a two-line candidate-local
self-test log. It did not bind the successful Jobs and observability workflows
at the exact source commit. Docker Buildx disabled OCI-native SBOM and
provenance attestations while the workflow wording did not clearly distinguish
those absent attestations from Bluey's internal file SBOM and recipe
provenance.

Full review also found that real Actions-run repository objects do not contain
`default_branch`; release-v2 evidence did not persist one authoritative branch;
the publisher did not rebind its plan to GitHub runtime identity before writes;
portal pull-back did not inspect the OCI artifact manifest/layer; historical
builder-policy bytes had been reinterpreted; and empty extra read-back
directories were not rejected.

Fresh full-diff review then found that promotion still parsed registry refs
from an unvalidated downloaded receipt and began registry access before proving
the selected Actions run/job/upload step. Portal manifest validation happened
only after `oras pull`, the ORAS-created timestamp was wall-clock dependent,
and publication runtime identity did not bind the trusted current default
branch. Those are source blockers, not external-environment nits.

The final independent delta review found one remaining P2: candidate assembly
redirected both exact-tip Actions run responses directly from `gh api` into
authoritative files. Unlike promotion's run capture, those writes had no 128
KiB transport ceiling and could leave producer output behind on failure. Both
candidate responses now use the fixed atomic `run` capture and explicitly
remove final and partial output when either side of the pipeline fails.

A follow-up guard review found the live workflow already used
`set -euo pipefail`, but both workflow validators accepted changing only that
named capture step to `set -eu`. They validated the pipelines and cleanup in
the broad candidate section without binding producer-failure propagation to the
exact step that runs both captures. Both validators now extract that one named
step, require its strict shell preamble and both bounded pipelines there, and
mutate only its `pipefail` setting to prove an unrelated strict step cannot
satisfy the contract.

## Authority Contract

```text
successful exact-tip default-branch Jobs CI push
  + successful exact-tip default-branch Observability push
  + supplemental release-contract self-test
  -> build one release-v2 candidate with controlled immutable refs
  -> threshold authorization
  -> protected package writer
  -> rebind plan to exact GitHub SHA/repository/repository ID/default branch
  -> publish stored OCI layouts and exact portal tar
  -> resolve immutable registry digests
  -> fetch and validate the 32 KiB-capped portal manifest before portal pull
  -> full remote pull/read-back
  -> inspect raw portal manifest, exact layer, closed trees, and runtime images
  -> protected promotion validates one canonical candidate/receipt/run/job
     preflight before registry login or ref consumption
  -> promotion independently pulls only those preflighted immutable refs again
  -> activation/canary/CAS authority remains separately required
```

The publication plan is provider-neutral. It binds a controlled lowercase OCI
repository prefix and canonical portal artifact origin supplied by protected
repository configuration rather than workflow-dispatch input. The checked-in
network adapter currently uses the repository owner's GHCR namespace and the
automatic `GITHUB_TOKEN`; a different provider needs a separately reviewed
adapter while preserving the same plan and receipt contract.

Workflow-token authority is job-local and fail closed. All six release jobs
declare only `actions: read` and `contents: read` as their common baseline.
Candidate, verify, authorize, and rollback receive no Packages permission;
publication alone adds `packages: write`, and promotion alone adds
`packages: read`. The workflow-level default remains `contents: read` only.

The three runtime images retain their signed OCI image-manifest digests. The
publisher unpacks each already-verified candidate tar into an OCI layout
directory because the registry client consumes layouts, then pulls into a
fresh layout and creates a normalized verifier tar. Transport tar size is not
treated as authority because registry-created `index.json` tag annotations can
change layout packaging without changing the signed manifest or rootfs bytes.
The receipt instead binds the manifest digest and canonical content inventory.
The portal tar is published as an OCI artifact and its exact raw tar bytes are
read back and re-inspected. Its raw OCI manifest digest, artifact/config types,
single layer digest/size/media type/title, and one-file ORAS output are exact;
the final read-back tree admits no extra file or directory. Each record
distinguishes the signed delivery ref
from the immutable registry ref actually observed. Release-v2 activation
portal-readback evidence separately names and verifies the exact signed HTTPS
URL; registry storage is not mislabeled as static delivery. The receipt also
binds the runner native addon at
`app/runner/dist/native/bluey_jobs_runner_native_storage.node`, including its
mode, size, and SHA-256 through the signed runner inventory.

The publication plan also binds one canonical UTC RFC3339 whole-second
`portalManifestCreatedAt`, derived from the signed portal provenance
`sourceDateEpoch`. The publisher supplies that exact value as the ORAS
`org.opencontainers.image.created` annotation and the verifier requires exact
equality. Retries at different wall-clock times therefore retain one manifest
identity; changing the signed source epoch changes that identity.

`local_fixture` cannot enter any production create path. Its strict v1 reader
exists only to audit and read back historical promotion/rollback trees; remote
production authority uses v2 and rejects mixed generations.

## Exact-Tip Test Evidence

Release-v2 candidates require canonical evidence for exactly these two GitHub
workflow paths:

- `.github/workflows/jobs-ci.yml`
- `.github/workflows/observability-policy.yml`

Both runs must be completed successful `push` runs on the repository default
branch, share the same repository identity, and have `head_sha` equal to the
candidate source commit. Because Actions-run payloads omit repository
`default_branch`, the trusted workflow-event branch is supplied separately,
stored once as `defaultBranch`, and required on both normalized runs. Run IDs
and attempts are signed into the candidate.
The candidate-local release-contract self-test remains bound by SHA-256 but is
never sufficient on its own. Historical release-v1 bytes retain their original
schema; new release-v2 assembly fails without evidence v2.

Both candidate Actions run responses pass through the fixed 128 KiB atomic
`run` capture before parsing. Byte 131,072 is admitted; byte 131,073, capture
failure, or API producer failure removes both final and partial output and
aborts candidate assembly. The exact named capture step must begin its shell
body with `set -euo pipefail`; either workflow validator rejects removal of
`pipefail` from only that step even while every unrelated strict step remains.

Promotion requires an explicit run ID and run-attempt number, fetches that exact
attempt's run and complete job list through the GitHub Actions API, and lists
the run-scoped artifacts. Each API body passes through an atomic fixed-kind
capture before it can become a file: 128 KiB for the run and 1 MiB each for
jobs and artifacts. It requires a successful
`workflow_dispatch` of this exact workflow and source commit, exactly one
successful `publish` job, and the successful receipt-upload step. The
normalized evidence is stored and hash-bound into the promotion. It names the
declared publication environment while explicitly recording that repository
environment settings are not attested by the Actions run API.

The publication artifact name is exact and attempt-qualified as
`jobs-managed-cloud-publication-<run-id>-<run-attempt>`. Selection admits
exactly one unexpired artifact whose immutable ID, digest, size (at most 1 MiB),
repository IDs, branch, and head SHA match the authorized source. Download is
by that validated artifact ID, never by a reusable name. The publication set,
preflight, workflow evidence, and final promotion all bind the same run and
attempt. An older attempt, ambiguous artifact list, missing digest, oversized
receipt, wrong repository/source, truncated API body, or cap-plus-one body
fails before receipt download or registry login and leaves no partial output.

Before registry login or immutable-ref consumption, promotion validates the
authorized candidate, exact canonical plan and publication-set bytes, current
GitHub repository ID/name/default branch, candidate source commit through the
selected workflow run and exact-attempt job shape, and receipt-upload step.
It emits a bounded canonical preflight document and derives every registry ref
only from that document; downloaded plan/set JSON is never used directly as
shell authority.

Both portal-manifest fetches stream through an atomic 32 KiB capture instead
of letting ORAS write an unbounded file. Byte 32,768 is admitted; byte 32,769
aborts the pipeline, removes the temporary file, and produces no authoritative
output. The existing canonical manifest validator still runs before any pull.

New remote promotions use
`bluey-jobs-managed-cloud-promotion-v2`/schema 2, and new rollback documents
and rollback sets use their v2 audiences/schemas. A strict historical
`local_fixture` promotion-v1 and rollback-v1 reader is retained only for audit
and read-back of already stored bytes. Production create paths emit/require v2,
and mixed v1/v2 trees or rollback pairs fail closed without an implicit
migration.

## Supply-Chain Truth

The candidate contains Bluey's exhaustive file-inventory SBOM and deterministic
recipe/source provenance documents for each component. They are not registry
attestations. Because current Buildx calls explicitly use
`--sbom=false --provenance=false`, the publication plan and receipt record:

```text
blueyFileInventorySbom       present
blueyRecipeProvenance        present
ociSbomAttestation           not_generated
ociProvenanceAttestation     not_generated
```

No source, UI, documentation, or release evidence may imply otherwise.

Historical builder-policy v1 bytes remain valid for release-v1 and legacy
release-v2 verification. New release-v2 contracts emit builder policy v2 with
`publish` in the no-rebuild set. Only builder policy v2 can enter production
publication, so compatibility validation cannot grant old candidates new write
authority.

## Environment And Permission Boundary

The `publish` job names `jobs-managed-cloud-artifact-publication`, requests
`packages: write` only for that job, and serializes every managed-cloud release
operation under one non-cancelling lock. Promotion uses
`jobs-managed-cloud-release` and receives `packages: read` only.

Those declarations are requirements, not proof that either environment has
required reviewers, branch policy, self-review prevention, billing capacity,
or package access. The 2026-08-31 release audit reported zero configured GitHub
environments, no existing packages, and a zero-dollar Packages budget. This
round does not change any of those settings or dispatch the workflow.

An interrupted registry operation can leave unreferenced immutable blobs or
transport tags. They are never release authority: no publication set is
uploaded and promotion cannot pass without all four immutable read-backs and a
successful exact-source publication job. Cleanup of orphaned registry objects
is a separately reviewed retention operation, not an automatic failure path.

## Acceptance Criteria

1. Release-v2 candidate assembly fails without both successful exact-tip
   default-branch workflow runs, and their one named capture step must retain
   `set -euo pipefail` plus both bounded pipelines.
2. Candidate image references derive from protected namespace configuration;
   the former `ghcr.io/bluey` destination cannot be emitted.
3. Only the protected publication job receives `packages: write`.
4. Every release job receives exactly the job-local read permissions it needs,
   and read-only jobs receive no Packages authority.
5. Publication unpacks stored OCI-layout tars without rebuilding, publishes the
   layouts, and verifies the resolved digest equals the signed image-manifest
   digest.
6. Before registry login or write, plan source commit/repository/repository ID
   and trusted current default branch equal the exact GitHub runtime values.
7. Publication reads back exactly three OCI archives, the exact portal tar, and
   its raw manifest in a flat tree; missing, extra, changed, or ambiguous paths
   fail closed.
8. The portal manifest is capped at 32 KiB and validated immediately after
   fetch and before pull. It has the exact digest, deterministic created
   annotation, manifest/artifact/config types, and one exact candidate-tar
   layer; post-pull ORAS output contains one regular file.
9. The receipt binds the native addon, manifest, candidate, authorization,
   source repository, namespace, portal origin, signed delivery refs, observed
   registry refs, artifact records, workflow run/job, and truthful supply-chain
   status.
10. Promotion selects exactly one attempt-qualified receipt by validated
   immutable artifact ID/digest/size after bounded exact-attempt API capture,
   then emits one canonical preflight over the authorized candidate, plan/set,
   current runtime identity, selected run/job/attempt, and successful upload
   step before login. It independently pulls every registry artifact only by
   immutable refs in that preflight.
11. `local_fixture`, mutable or uncontrolled namespaces, missing remote
   read-back, and package-permission drift fail closed.
12. Historical builder-policy and local-fixture promotion/rollback v1 bytes
   remain strictly verifiable for audit, but production publication, promotion,
   and rollback create paths require v2 and reject mixed generations.
13. No registry write, deployment, activation, flag change, provider call,
   application, message, or customer effect occurs while implementing or
   testing this source phase.

## External Stop Conditions

Do not dispatch `publish` or `promote` until both protected environments and
reviewers are read back, Packages budget and access are approved, namespace and
portal-origin variables are set and read back, threshold material is available
through the established ceremony, and exact-tip CI is green at the default
branch commit.

Registry publication still does not prove static portal serving, deployed
image identity, read-only root filesystems, hosted PostgreSQL/Temporal behavior,
real capacity, source-provider rights/canaries, monitoring, cohort approval,
kill switches, activation CAS, or rollback. Those remain production no-go
gates.

The inherited release-manifest field named `publishedAtMs` remains the
deterministic source commit epoch for reproducible historical schemas; it is
not registry publication time. Phase 627B does not use it as publication
evidence. The new publication set uses `attestedAtMs`; a future manifest schema
should rename the legacy field without reinterpreting stored v1/v2 bytes.
