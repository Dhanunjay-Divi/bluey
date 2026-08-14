# FIX-690: Managed Availability Could Outrun Dispatch Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled it against the Phase 611 worktree and
> round contract. No registry, hosted service, credential, tenant, deployment, canary, or release
> flag was used.

## Issue

The browser portal and Jobs API could advertise or admit the managed Background runner while the
workflow-command dispatcher or another required managed-cloud role was not live and compatible.

## Root Cause

Availability was assembled from plan entitlement, the cloud-distribution flag, a nonempty workflow
token, and historical runner-fleet cutover. Debug builds could bypass those predicates. The
dispatcher had an independent disabled flag, token parsing was not identical across admission and
startup, and fleet cutover proved neither a live runner nor an exact API/gateway/worker/runtime
release. There was no signed whole-stack head or database-time role quorum to join those facts.

## Fix Summary

- Introduce an exact, canonical, threshold-signed managed-cloud trust, manifest, activation,
  cohort, compatibility, rollback, and revocation authority for four immutable artifacts: Jobs API,
  workflows, runner, and portal.
- Persist one-time role-scoped runtime grants and fenced, monotonic instance heartbeats. Derive
  readiness with database time from the active nonrevoked head, exact release identities, required
  role count/capacity, schema/protocol/configuration/Temporal/storage/fleet bindings, cohort, and
  independent operational and ATS gates.
- Require both cloud distribution and workflow-command dispatch configuration for new-effect
  readiness. Validate the private dispatcher configuration before starting background workers and
  remove debug-only customer availability/admission bypasses.
- Recompute readiness transactionally during start/resume admission and freeze the exact release
  authority into the workflow command. Exact idempotent replay is resolved before mutable current
  readiness and never remints authority.
- Report only bounded availability/reason projections to customers. Administrative release and
  runtime endpoints retain the exact server-owned control-plane evidence.
- Add build-once artifact contracts and a credential-free candidate verifier. Promotion and
  rollback are defined over stored, read-back-verified bytes without rebuilding.

## Files Modified

| File | Change |
|------|--------|
| `infra/{postgres,sqlite}/server-runtime/*jobs_managed_cloud_release_authority.sql` | Add paired trust, release, activation, cohort, runtime, readiness, binding, and replay authorities |
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Verify signed objects and own release import, activation, runtime, readiness, admission, rollback, and revocation decisions |
| `server/src/api/jobs_managed_cloud_releases.rs` | Expose the closed administrative and runtime control-plane contracts |
| `server/src/jobs_managed_cloud_runtime.rs` | Claim and heartbeat the Jobs API and embedded dispatcher roles from measured runtime state |
| `server/src/api/jobs.rs`, `server/src/db/jobs/workflow_commands.rs` | Replace loose cloud availability/admission with transactional exact-release authority |
| `server/src/{main.rs,lib.rs,bin/bluey-jobs-api.rs}` | Register and start only valid managed runtime reporters and independently gated dispatchers |
| `jobs/automation/src/managed-cloud-{execution,runtime,runtime-client}.ts` | Define canonical release memos and the shared runtime claim/heartbeat client |
| `jobs/workflows/src/{gateway,worker,discovery-worker,global-discovery-worker}.ts` | Report exact process role health while keeping reserved discovery capabilities out of launch authority |
| `jobs/runner/src/server.ts` | Report the exact managed-runner runtime only after its local execution boundary is ready |
| `jobs/scripts/managed-cloud-release-gate.mjs` | Generate closed contracts and inspect, authorize, promote, read back, and roll back stored artifacts |
| `.github/workflows/jobs-managed-cloud-release.yml`, component Dockerfiles | Define the manual build-once candidate, isolated verification, and no-rebuild promotion shape |
| `.github/workflows/{jobs-ci,release}.yml`, `ops/bluey-jobs.env.example` | Enforce source gates while leaving all launch flags parked |

The complete Phase 611 path inventory is maintained in
`docs/work/IMPL-PHASE-611-JOBS-MANAGED-CLOUD-LAUNCH-AUTHORITY.md`.

## Edge Cases Handled

- dispatcher disabled while cloud distribution is configured;
- empty, malformed, or admission/startup-inconsistent private gateway tokens;
- debug builds, missing head, expired activation, revoked release, and cohort miss;
- stale, fenced, wrong-role, old-release, or insufficient-capacity runtime instances;
- gateway live without a compatible Temporal worker or managed runner;
- schema, protocol, task-queue, failure-converter, portal, storage, or runner-fleet drift;
- concurrent activation/rollback writers and stale expected-head revisions;
- exact idempotent command replay after head, flag, or readiness changes; and
- reserved discovery/verifier roles that do not yet close their pre-effect lease boundaries.

## How to Test

```bash
node --test jobs/scripts/managed-cloud-release-gate.test.mjs
npm --workspace @bluey/jobs-automation test
npm --workspace @bluey/jobs-runner test
npm --workspace @bluey/jobs-workflows test
cargo fmt --all -- --check
CARGO_INCREMENTAL=0 cargo clippy --all-targets -- -D warnings
CARGO_INCREMENTAL=0 cargo test --all-targets
git -P diff --check
```

At this documentation checkpoint, the release-gate unit suite had passed 16 tests and an interim
workflow suite had passed 290 tests before subsequent recovery coverage changed. The final frozen
Jobs/Rust, paired-migration, privacy, artifact, and independent-review gates remain pending and
must replace this interim evidence before acceptance.

## Known Limitations

- All customer release flags remain `0`; no deployment or customer availability changed.
- Registry immutability/read-back, production signatures and credentials, protected-environment
  approval, live PostgreSQL/Temporal/network behavior, genuine runner capacity, runtime image
  attestation, and read-only-rootfs enforcement require hosted evidence.
- Direct discovery, global discovery, and original-source verification remain explicitly false in
  the importable Phase 611 feature authority until later work closes their pre-effect boundaries.
- A signed customer cohort, production canary, and hosted rollback rehearsal remain mandatory.
