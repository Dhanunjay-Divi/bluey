# Round 611 — Jobs Managed Cloud Launch Authority

> **Codex preflight:** Load `$bluey-ops` before implementation or review. Use the current repository
> and handoff first; archived material is only a fallback for one specifically missing fact.

**Status:** IN PROGRESS; EVERY RELEASE FLAG PARKED; NO DEPLOYMENT AUTHORITY

## Goal

Make the no-install Jobs experience fail closed on one exact, signed, live managed-cloud stack.
Neither a plan entitlement nor a loose combination of flags, credentials, and historical fleet
cutover may advertise Background runner availability or admit new cloud workflow commands.

Round 611 must provide:

1. a build-once, immutable whole-stack candidate and independently verified artifact inventory;
2. signed trust, manifest, activation, cohort, rollback, and revocation authority;
3. database-time runtime grants, fenced instance heartbeats, and exact role quorum;
4. transactional start/resume admission bound to the active release and live readiness; and
5. no-rebuild promotion, read-back, compare-and-swap activation, and rollback contracts.

## Product Boundary

- The product is the browser-delivered Jobs portal plus managed cloud execution. Customers install
  nothing for this path.
- The installable Electron Bluey Browser remains parked P2 and is not a cloud activation
  dependency.
- Round 611 does not deploy, contact a registry or hosted Temporal namespace, enable a flag, admit a
  customer cohort, or claim a live canary.
- Credentials, registry/protected-environment approvals, live PostgreSQL/Temporal/network evidence,
  hosted rollback, platform proof that each runtime is executing the signed image digest, and a
  read-only root-filesystem policy remain external gates.
- Embedded startup measurement proves the closed curated runtime set (the API executable, Jobs
  application trees, Node executable, failure converter, and managed runner browser), not every
  `node_modules` or system-library byte. Activation therefore also requires signed passing canary
  checks named `jobs-api-runtime-artifact-attestation`,
  `jobs-workflows-runtime-artifact-attestation`,
  `managed-runner-runtime-artifact-attestation`, and `read-only-rootfs-policy`; absent hosted
  platform evidence, no customer channel or release flag may be enabled.
- Original-source employer and job-risk verification is the next production worker batch. The
  release schema reserves its closed role, but this round does not invent a nonexistent entrypoint
  or source-verification evidence.
- Direct discovery and global discovery remain separate legacy workers in this round. Their current
  lease routes do not yet bind a managed-cloud runtime session at the pre-effect boundary, so an
  importable Phase 611 release requires `directDiscovery=false`, `globalDiscovery=false`, and
  `sourceVerification=false`. The schema reserves all three capabilities for later fenced worker
  batches without letting them contribute launch authority now.

The checked-in gates remain parked:

```text
BLUEY_JOBS_WORKFLOW_COMMAND_DISPATCH_ENABLED=0
BLUEY_JOBS_WORKFLOW_CLEANUP_ENABLED=0
BLUEY_JOBS_CLOUD_BROWSER_DISTRIBUTION_ENABLED=0
BLUEY_JOBS_LOCAL_BROWSER_DISTRIBUTION_ENABLED=0
```

## Baseline Failure Mode

Before this round, cloud availability and transactional staging could become true from the cloud
distribution flag, any nonempty workflow token, and historical runner-fleet cutover while the only
workflow-command dispatcher remained disabled. Debug builds bypassed those predicates entirely.
The API and database accepted malformed tokens that dispatcher startup and the gateway rejected.
Fleet cutover did not prove a live runner, gateway health did not prove a compatible Temporal
worker, and no durable authority bound the API, gateway, worker, runner, portal, migrations, or
protocols into one promoted release.

Round 611 removes every such shortcut. A debug artifact is never production activation evidence.

## Signed Authority

`bluey-jobs-managed-cloud-release-v1` is distinct from the local Browser registry, roles, and keys.
It reuses only reviewed cryptographic patterns:

- byte-bounded base64url decoding and exact-key typed JSON;
- canonical UTF-8 reserialization equality and domain-separated SHA-256;
- sorted unique signer IDs, role-specific active Ed25519 keys, threshold verification, trust
  generation, and signed time bounds; and
- dual predecessor/successor root authority for trust rotation.

The immutable manifest binds the source commit and one closed component set:

- the `bluey-jobs-api` Linux image and in-image binary;
- the workflow image and its exact gateway, Temporal worker, discovery, and global-discovery
  entrypoints;
- the embedded command and cleanup dispatcher identities;
- the runner image, automation bundle, native addon, Playwright version, Chromium revision, and
  executable digest;
- the deterministic portal static tree;
- SQLite/PostgreSQL migration heads, order, and set digests;
- command, cleanup, workflow/memo, runner, worker-auth, discovery, configuration, and failure
  converter protocol digests; and
- exact SBOM, provenance, inventory, test, platform, architecture, entrypoint, runtime user, and
  production-build-profile evidence.

Mutable tags, debug profiles, unknown files, symlinks, source/test/dev payloads in runtime images,
missing provenance, or an incomplete role set invalidate the manifest.

A signed activation binds one manifest to environment, region, channel, cohort, monotonic sequence,
predecessor head, validity window, rollout caps, schema/config/Temporal/storage/fleet identities,
closed feature authority, derived role counts and heartbeat TTL, compatibility set, and canary
evidence. Shadow authority can observe without customer admission. The base reviewed-cloud quorum is
the API, command dispatcher, cleanup dispatcher, workflow gateway, workflow worker, and managed
runner. Direct discovery, global discovery, and original-source verification may add their exact
runtime roles only in a successor contract that closes each pre-effect lease boundary. Round 611
requires all three feature authorities to be false and reserves, but does not claim, those workers.

Rollback is a new higher-sequence transition to previously imported, verified, compatible,
nonrevoked bytes. It never rewinds a pointer, rebuilds an artifact, or downgrades a migration.

## Runtime Readiness and Admission

Deployment-issued one-time grants bind an activation, component digest, role, environment, region,
channel, and resource identity. Grant consumption and instance heartbeats are separately durable,
fenced, monotonic, and evaluated with database time. Expired or revoked grants, stale heartbeats,
old-release instances, insufficient capacity, or role mismatch cannot contribute to readiness.

Readiness is derived; no caller or administrator can store `ready=true`. It is the intersection of:

- a current, unexpired, nonrevoked signed activation and CAS head;
- exact cohort eligibility;
- the explicit cloud-distribution and workflow-dispatch gates plus valid private dispatcher
  configuration;
- fresh compatible API, gateway, Temporal worker, dispatcher, cleanup, and runner role evidence,
  plus only the discovery or verifier roles enabled by signed feature authority;
- exact schema, protocol, task-queue, failure-converter, portal, storage, and runner-fleet identity;
- independent ATS certification, circuit, quarantine, operational-hold, account, entitlement,
  deletion, and cleanup authority; and
- the activation's required live capacity and safety margin.

New start and resume admission recompute this authority under the common lock order and freeze its
exact head revision, transition, activation, manifest, cohort, release, compatibility, and readiness
digests into a one-to-one workflow-command binding. Exact idempotent replay is checked first and
returns the frozen command even after current head or flags change. It never remints, remeters, or
switches a run to another stack.

Immediately before first gateway I/O, the dispatcher rechecks the frozen release and a compatible
live role. If authority disappeared before request-start, the command remains safely pending or is
cancelled only under an exact pre-I/O contract. Once request-start or any irreversible/ambiguous
boundary exists, revocation and rollback block new effects but never block receipt persistence,
result lookup, terminalization, or ambiguity recovery.

PostgreSQL uses one common order: operational-hold authority, cloud-release registry, ATS registry,
runner fleet, account/deletion fence, entitlement, application/attempt, command, and lease. No
network I/O occurs under database locks.

## Build Once, Promote Stored Bytes

The candidate workflow starts from an exact clean commit and builds once without production
credentials. It exports OCI layouts and deterministic binary/static archives, normalizes and
inventories every file, emits SBOM/provenance/test evidence, and stores immutable candidate bytes.

An isolated verifier downloads only those stored bytes and validates all internal hashes, runtime
users, entrypoints, closed inventories, schema/protocol/config compatibility, and container smoke.
Authorization attaches offline threshold signatures without rebuilding. Promotion downloads the
authorized candidate, copies exact blobs to immutable destinations, reads them back, verifies every
digest, deploys dark, records canary evidence, and only then compare-and-swap activates the head.
A crash before the CAS leaves no customer authority.

## Acceptance Criteria

1. Fresh SQLite/PostgreSQL migrations create no trust policy, release, activation, cohort, grant,
   runtime, head, or customer authority.
2. Every signed object is exact-key, canonical-byte, domain, threshold, predecessor, sequence, time,
   role, and digest verified server-side; supplied hashes and clocks never become authority.
3. Same request and identical bytes replay exactly; same identity with changed bytes conflicts.
4. Activation and rollback use expected-revision plus expected-current-transition CAS. Concurrent or
   stale writers cannot replace the head.
5. Revocation, expiry, missing head, debug build, schema/protocol/config drift, or incomplete artifact
   inventory denies new admission.
6. Runtime grants are one-time, token-digest bound, release/component/role scoped, revocable, and
   response-loss replay safe. Heartbeats use database time, monotonic epoch/sequence, and fencing.
7. Readiness requires the exact active release and fresh required-role quorum with capacity. Old or
   incompatible instances never keep a successor head ready.
8. Dispatch off, malformed token, invalid origin, missing gateway/worker/runner, expired heartbeat,
   fleet drift, operational hold, ATS revocation/quarantine/circuit, account deletion, or cohort miss
   each independently deny new cloud admission.
9. Debug builds and tests no longer bypass cloud availability or transactional admission.
10. Start/resume bind one exact release authority; idempotent replay precedes mutable readiness and
    remains available after rollback or flags-off.
11. Request-start is durable before external I/O. Head loss after request-start cannot erase
    delivery-unknown, exact receipt, intervention, submitted, or terminal recovery authority.
12. A command bound to release A runs only on A or an explicitly signed compatible release. Rolling
    overlap is bounded and never inferred from a mutable tag.
13. Candidate construction is deterministic and credential free. Promotion and rollback use stored
    bytes only, verify immutable read-back, and never rebuild.
14. Workflow/API/runner images are pinned, rootless, and closed to the intended production runtime
    inventory; the portal archive has an exact path/size/SHA tree and no maps or symlinks.
15. Customer status exposes only bounded reason codes and no account lists, grant tokens, raw provider
    IDs, internal URLs, or release secrets. Logs and metrics remain pseudonymous.
16. Paired migrations, full Jobs/Rust gates, reproducibility, privacy scans, generated portal parity,
    and independent line-by-line reviews pass with all release flags still zero.

## External-Only Evidence

Local source acceptance cannot prove registry immutability, protected-environment approvals,
production signing ceremonies, hosted Temporal task-queue behavior, live PostgreSQL replica/network
faults, real runner capacity, provider credentials, immutable-host read-back, runtime image-digest
attestation, read-only root-filesystem enforcement, customer cohort approval, or canary/rollback
behavior. Those remain mandatory before any customer channel or release flag can be enabled.
