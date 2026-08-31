# IMPL: PHASE-627B — Jobs Artifact Publication Authority

> **Codex preflight:** Loaded `$bluey-ops`, `AGENTS.md`, the current handoff,
> Round/IMPL/REVIEW 611, Round/IMPL/REVIEW/FIX 624, the managed-cloud release
> workflow and gate, and current audit evidence. The SSD archive was not used.

**Status:** Local bounded source implementation corrected after fresh guard
review; another independent delta review pending; uncommitted and unpushed; no
remote publication or production mutation

**Base commit:** `a6a7c3320fbd6efbc273cf85c1d6fe25ca565a40`

**Branch:** `feat/phase-627b-jobs-artifact-publication-authority`

## Scope

### Does

- replace production creation of the Phase 611 `local_fixture` adapter with a
  canonical remote registry publication/read-back plan and receipt while
  retaining a strict historical-v1 audit/read-back validator;
- add a protected build-once `publish` workflow operation with exact job-local
  Packages permission and a pinned ORAS client;
- make Actions/contents read authority explicit on every release job while
  keeping Packages access absent from read-only jobs;
- bind three immutable OCI image-manifest digests, the exact portal tar, and the
  managed runner native addon through full remote pull/read-back;
- bind the selected publication run and explicit attempt to a successful
  exact-source Actions workflow/job/receipt-upload read-back and exactly one
  immutable attempt-qualified artifact without claiming environment settings;
- make protected promotion independently repeat the immutable remote pull;
- derive candidate destinations from controlled repository variables and reject
  the former `ghcr.io/bluey` namespace;
- require full exact-tip Jobs CI and Observability default-branch push evidence
  for release-v2 candidates while keeping the release-contract self-test
  supplemental;
- persist the separately trusted default branch, rebind the plan to exact
  GitHub runtime identity before registry access, including the current default
  branch;
- validate the exact authorized candidate, canonical plan/publication set,
  selected workflow run and attempt, exact-attempt job, upload step, and
  immutable artifact ID/digest/size into one bounded promotion preflight before
  receipt download, registry login, or ref consumption;
- transport-cap both candidate CI run responses and the pre-download promotion
  run response at 128 KiB, jobs and artifacts responses at 1 MiB each, the
  receipt archive at 1 MiB, and each raw portal manifest at 32 KiB, with atomic
  no-final-or-partial failure behavior;
- download the receipt only by the already validated immutable artifact ID and
  preserve its digest and size in canonical preflight/final evidence;
- derive all promotion registry refs only from that canonical preflight;
- bind the portal manifest created annotation deterministically to signed
  `sourceDateEpoch`, and inspect the 32 KiB-capped raw manifest before pull plus
  the exact layer, one-file output, and closed read-back tree after pull;
- preserve legacy builder-policy v1 validation while requiring newly emitted
  policy v2 for production publication;
- distinguish internal Bluey file SBOM/recipe provenance from absent OCI-native
  attestations; and
- add primary and independent structural/mutation coverage.

### Does not

- dispatch candidate, authorize, publish, promote, or rollback;
- create/configure a GitHub environment, reviewer, package, budget, token,
  registry namespace, repository variable, static bucket, or secret;
- generate an OCI-native SBOM/provenance attestation;
- serve the portal, deploy a runtime, activate a release, enable a flag, admit a
  cohort, contact a provider, submit an application, or send a message; or
- edit `docs/reviews/`.

## Files Modified / Added

| File | Action | Purpose |
|------|--------|---------|
| `.github/workflows/jobs-managed-cloud-release.yml` | Modified | Add protected remote publication and promotion read-back |
| `jobs/scripts/managed-cloud-release-gate.mjs` | Modified | Add exact CI and publication/read-back authority |
| `jobs/scripts/managed-cloud-release-gate.test.mjs` | Modified | Add evidence, portal, identity, and workflow mutations |
| `jobs/scripts/ci-guards-self-test.mjs` | Modified | Independently guard workflow structure and mutations |
| `jobs/OPERATIONS.md` | Modified | Document setup, ceremony, truth ceiling, stop conditions, and remaining external gates |
| `CHANGELOG.md` | Modified | Record the bounded source result and no-effect boundary |
| `docs/rounds/ROUND-627B-JOBS-ARTIFACT-PUBLICATION-AUTHORITY.md` | Added | Freeze scope and gates |
| `docs/work/FIX-788-jobs-artifact-publication-readback-authority.md` | Added | Record root cause and correction |
| `docs/work/IMPL-PHASE-627B-JOBS-ARTIFACT-PUBLICATION-AUTHORITY.md` | Added | Record implementation evidence |
| `docs/work/REVIEW-PHASE-627B-JOBS-ARTIFACT-PUBLICATION-AUTHORITY.md` | Added | Consolidate reviews |

## Key Decisions

- The contract is provider-neutral; the first checked-in network adapter is
  GHCR because the repository-scoped `GITHUB_TOKEN` can receive narrowly scoped
  Packages permission without adding a long-lived secret.
- OCI images are copied from candidate OCI layouts and addressed by their
  already signed image-manifest digests. Tags are transport handles only and
  never become release authority.
- ORAS consumes and produces OCI layout directories. The workflow therefore
  extracts each already-verified candidate layout tar without modification,
  pulls into a fresh layout, and creates a normalized tar for the verifier.
  Receipts compare signed manifests and canonical content inventories rather
  than unstable transport-layout tar sizes.
- The portal tar is published as an OCI artifact for durable controlled
  storage/read-back. Its signed HTTPS serving URL remains separate and must be
  proven by release-v2 activation portal-readback evidence, which now binds the
  exact signed URL as well as the returned bytes.
- Publication receipts are not activation. Promotion must independently pull
  the immutable refs and still satisfy every activation/canary/CAS gate.
- A downloaded publication receipt is data, not shell authority. Promotion
  first produces a canonical preflight that binds candidate, plan/set hashes,
  GitHub runtime identity, selected workflow run/job/attempt, and receipt upload;
  only that output may supply registry refs.
- Portal publication uses the signed portal provenance epoch as one canonical
  RFC3339 `org.opencontainers.image.created` value. Retry wall-clock time cannot
  change the manifest digest.
- Historical release-v1 test evidence remains valid for historical stored
  bytes. New release-v2 candidates require CI evidence v2.
- Production promotion and rollback creation emits only audience/schema v2.
  Strict historical audience/schema-v1 `local_fixture` promotion and rollback
  readers remain available for audit/read-back of stored bytes; mixed v1/v2
  trees fail closed and cannot enter a production creation path.
- Historical release-v1 and pre-627B release-v2 builder policies remain
  verifiable as schema v1. New v2 contracts emit policy v2, and publication
  rejects anything older so compatibility does not create write authority.
- Producer failure is shell authority, not merely capture-process authority.
  Both validators isolate the exact named candidate CI capture step, require
  `set -euo pipefail` at the start of its shell body, and require both bounded
  run-response pipelines inside that same step. A strict shell directive in a
  different step cannot satisfy this contract.
- OCI-native SBOM/provenance attestations remain absent and are recorded as
  such. Bluey's internal documents are preserved without relabeling them.

## Local Verification

Observed on 2026-08-31:

```text
node --check jobs/scripts/managed-cloud-release-gate.mjs
  PASS

node --check jobs/scripts/managed-cloud-release-gate.test.mjs
  PASS

node --test jobs/scripts/managed-cloud-release-gate.test.mjs
  PASS: 29 / 29

node jobs/scripts/managed-cloud-release-gate.mjs workflow \
  --file .github/workflows/jobs-managed-cloud-release.yml
  PASS

node jobs/scripts/ci-guards-self-test.mjs
  PASS

ruby -e 'require "yaml"; YAML.load_file(ARGV.fetch(0))' \
  .github/workflows/jobs-managed-cloud-release.yml
  PASS
```

No Docker, ORAS network, GitHub workflow, registry, package, environment,
provider, production, or customer command was executed.

## Known Blockers

1. Another independent delta review of the named-step `pipefail` correction is
   pending; aggregate integration review is still required before this
   uncommitted diff lands.
2. The repository currently has no proven protected release environments or
   package access, and the Packages budget was reported as zero.
3. `BLUEY_JOBS_ARTIFACT_NAMESPACE` and
   `BLUEY_JOBS_PORTAL_ARTIFACT_ORIGIN` must be configured and read back.
4. A real publish/read-back must prove the pinned ORAS/GHCR path on exact
   authorized bytes.
5. Exact-tip default-branch CI must be supplied after the aggregate release
   candidate is integrated; PR evidence cannot substitute.
6. Static serving, registry-native attestations, dark deployment,
   image/rootfs/runtime identity, hosted PostgreSQL/Temporal/network behavior,
   real capacity, canaries, monitoring, cohort, flags, kill switches, CAS
   activation, and rollback remain unproven.
7. The inherited manifest `publishedAtMs` name still denotes the deterministic
   source commit epoch. It is not publication evidence; a successor manifest
   version should rename it without breaking historical stored bytes.

## Review Handoff

Prior reviews were superseded by the full final review and its fresh full-diff
follow-up. The current correction set adds real API-shaped exact-attempt
evidence, bounded atomic capture of both candidate run responses, every
pre-download promotion API response, and each raw portal manifest, immutable
artifact ID/digest/size selection, ID-only receipt
download, exact pre-network publication/promotion runtime identity, canonical
promotion preflight, deterministic portal-manifest identity, closed post-pull
inspection, production-only promotion/rollback v2 creation, strict historical
v1 readers, mixed-generation rejection, and historical builder-policy
compatibility with a publication-only v2 cutoff. It now also binds
`set -euo pipefail` to the exact named step that contains both candidate
producer pipelines, with a step-only negative mutation in both validators.
Another fresh independent review is required before restoring a clean source
verdict.

Review the complete branch diff from the exact Phase 624 base. Pay particular
attention to API-cap-before-download and artifact-selection ordering, exact
run-attempt binding, immutable-ID-only receipt download, preflight-before-
registry ordering, absence of raw receipt-derived registry refs, manifest-
before-pull ordering and byte cap, deterministic ORAS annotation identity,
slash-bearing default branches, shell quoting around ORAS, namespace ownership,
tag-versus-digest semantics, promotion's second read-back, cross-generation
promotion/rollback rejection, historical release-v1 compatibility,
test-evidence versioning, and whether any status wording overstates current
external configuration. Specifically mutate only the named candidate capture
step from `set -euo pipefail` to `set -eu` and confirm both validators reject
it without relying on strict-shell lines elsewhere in the workflow.
