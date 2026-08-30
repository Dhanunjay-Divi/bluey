# IMPL: PHASE-611 — Jobs Managed Cloud Launch Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its production and release guidance
> against the Phase 611 worktree, current round contract, and predecessor Phase 609/610 authority.
> No archive, registry, hosted service, credential, live tenant, deployment, canary, release
> activation, or flag enablement was used.

## Scope

**Does:**

- define one exact canonical signed release for the browser-delivered, no-install Jobs stack:
  Jobs API, workflows, managed runner, and portal;
- bind immutable content inventories, runtime identities, SBOM/provenance/test evidence, schema,
  protocol, configuration, Temporal, storage, runner-fleet, compatibility, cohort, activation,
  rollback, and revocation authority;
- add database-owned trust rotation, threshold signatures, compare-and-swap release heads,
  one-time runtime grants, fenced database-time heartbeats, exact role quorum, and bounded public
  availability projections;
- remove plan/flag/token/fleet and debug shortcuts from managed availability and start/resume
  admission, then freeze the exact admitted release A into each managed workflow command;
- keep exact idempotent replay ahead of mutable readiness, release, account, entitlement, and
  application checks;
- persist canonical release A at first durable request-start and split new-effect dispatch from
  lookup-only ambiguity reconciliation, including a byte-compatible historical protocol-v2 path;
- propagate A through managed-only Temporal activities and bind execution-lease claim,
  checkpoint/volume restore, resume, and irreversible-effect authorization to A plus a fresh exact
  managed-runner runtime B;
- preserve receipt lookup, ambiguity persistence, finish, terminalization, cleanup, and deletion
  recovery after mutable B or current-head authority changes;
- define credential-free build-once candidate construction and isolated stored-byte verification,
  authorization, promotion, read-back, and higher-sequence rollback contracts; and
- keep direct discovery, global discovery, and original-source verification false because their
  pre-effect managed runtime boundaries are not closed by this batch.

**Does NOT:**

- require or merge an installable Bluey Browser; the customer product remains the web portal plus
  managed execution;
- contact a registry, hosted Temporal namespace, production PostgreSQL, object store, ATS tenant,
  employer portal, runner pool, signing service, or protected deployment environment;
- create/use production credentials, publish/promote an artifact, admit a customer cohort, execute
  a canary/rollback, deploy a service, or enable any release flag;
- claim hosted runtime image-digest attestation, read-only-rootfs enforcement, real capacity,
  replica/network-fault behavior, or provider behavior from source tests;
- grant launch authority to direct discovery, global discovery, or the reserved original-source
  verifier; or
- edit the frozen `docs/reviews/` reference directory.

## Files Created / Modified

Final branch-diff inventory: **93 paths** relative to Phase 610 base `89d6b820`, including this
implementation document and the Phase 611 review document. `M` means modified and `A` means added.

| Status | File | Purpose |
|--------|------|---------|
| M | `.github/workflows/jobs-ci.yml` | Run managed-cloud source and release-contract checks in Jobs CI |
| M | `.github/workflows/release.yml` | Prevent the ordinary release path from bypassing the managed-cloud gate |
| A | `.github/workflows/jobs-managed-cloud-release.yml` | Define manual candidate, authorize, promote, and rollback stages over stored bytes |
| M | `CHANGELOG.md` | Record the bounded Phase 611 source result and parked launch evidence |
| M | `crates/cue-core/src/ipc_auth.rs` | Remove the Rust 1.98 fixed-chunk Clippy failure without weakening bearer parsing |
| M | `crates/cue-daemon/src/audio/framer.rs` | Preserve preallocated frame capacity after padded flush |
| M | `crates/cue-daemon/src/audio/system_capture.rs` | Decode fixed PCM pairs through the Rust 1.98 array-chunk API |
| M | `crates/cue-daemon/tests/system_audio_integration.rs` | Honor the declared integration deadline under parallel CI load |
| M | `crates/cue-rag/src/store.rs` | Decode fixed embedding values through the Rust 1.98 array-chunk API |
| A | `docs/rounds/ROUND-611-JOBS-MANAGED-CLOUD-LAUNCH-AUTHORITY.md` | Freeze the product, authority, recovery, release, and external-evidence contract |
| A | `docs/work/FIX-690-jobs-managed-availability-dispatch-authority.md` | Record the loose availability/dispatch root cause and fix |
| A | `docs/work/FIX-691-jobs-request-start-recovery-authority.md` | Record the recovery downgrade/new-effect root cause and fix |
| A | `docs/work/FIX-692-jobs-managed-runner-release-effect-boundary.md` | Record the missing runner release-bound effect authority and fix |
| A | `docs/work/FIX-693-jobs-managed-runner-measurement-bound.md` | Record the impossible runner measurement ceiling and cross-language bounded correction |
| A | `docs/work/FIX-694-jobs-managed-runner-node-path.md` | Record the pinned Playwright path mismatch, normalized runtime, and exact-head binding |
| A | `docs/work/FIX-695-rust-1-98-compatibility-backport.md` | Record the complete proven rolling-toolchain compatibility backport |
| A | `docs/work/IMPL-PHASE-611-JOBS-MANAGED-CLOUD-LAUNCH-AUTHORITY.md` | Inventory implementation, validation state, deviations, and follow-ups |
| A | `docs/work/REVIEW-PHASE-611-JOBS-MANAGED-CLOUD-LAUNCH-AUTHORITY.md` | Record independent review evidence, remaining external gates, and verdict |
| A | `infra/postgres/server-runtime/033_jobs_managed_cloud_release_authority.sql` | Add PostgreSQL release, runtime, readiness, command, request-start, and execution authority |
| A | `infra/sqlite/server-runtime/055_jobs_managed_cloud_release_authority.sql` | Add the paired SQLite authority, parity constraints, and guards |
| M | `jobs/.dockerignore` | Close managed image build contexts to intended runtime inputs |
| M | `jobs/package.json` | Expose the managed-cloud release gate command |
| M | `jobs/automation/package.json` | Export and build shared managed-cloud runtime/release modules |
| A | `jobs/automation/src/managed-cloud-execution.ts` | Canonicalize and validate release A plus current runtime B execution authority |
| A | `jobs/automation/src/managed-cloud-runtime-client.ts` | Claim grants and send fenced runtime heartbeats to Jobs API |
| A | `jobs/automation/src/managed-cloud-runtime.ts` | Parse exact role-scoped runtime configuration and readiness observations |
| M | `jobs/automation/src/worker-auth.ts` | Sign the new exact managed runtime and effect-boundary routes |
| A | `jobs/automation/tests/managed-cloud-execution.test.ts` | Cover canonical A+B execution authority and mismatch rejection |
| A | `jobs/automation/tests/managed-cloud-runtime-client.test.ts` | Cover exact runtime HTTP/auth and fail-closed response parsing |
| A | `jobs/automation/tests/managed-cloud-runtime.test.ts` | Cover role-scoped configuration and heartbeat observations |
| M | `jobs/automation/tests/worker-auth.test.ts` | Cover authentication for every new internal managed route |
| M | `jobs/runner/Dockerfile` | Produce the pinned rootless closed managed-runner image/runtime inventory |
| M | `jobs/runner/src/execution-lease.ts` | Bind claim, pre-effect authorization, and irreversible marker to exact A+B |
| M | `jobs/runner/src/intervention-policy.ts` | Preserve A across managed intervention resume without changing legacy contracts |
| M | `jobs/runner/src/run-checkpoint-store.ts` | Persist exact managed release authority with recoverable run state |
| M | `jobs/runner/src/runner-volume-client.ts` | Bind volume proof to request, release digest, runtime ID, and runtime epoch |
| M | `jobs/runner/src/server.ts` | Report managed runtime health and require A across start/resume/recovery paths |
| M | `jobs/runner/tests/container-hardening.test.ts` | Verify the closed rootless managed-runner container contract |
| M | `jobs/runner/tests/execution-lease.test.ts` | Cover claim, A+B echo, pre-effect authorization, and fencing |
| M | `jobs/runner/tests/intervention-policy.test.ts` | Cover managed release-bound resume and unchanged legacy parsing |
| A | `jobs/runner/tests/managed-cloud-effect-boundary.test.ts` | Cover end-to-end pre-I/O A+B mismatch, staleness, and recovery separation |
| M | `jobs/runner/tests/run-checkpoint-store.test.ts` | Cover durable managed release state and exact restore mismatch |
| M | `jobs/runner/tests/runner-volume-client.test.ts` | Cover managed volume proof fields and canonical release digest |
| M | `jobs/runner/tests/server-result-recovery.test.ts` | Prove result recovery does not depend on mutable B |
| A | `jobs/scripts/managed-cloud-release-gate.mjs` | Build/verify closed contracts and stored artifact inventories without rebuilding |
| A | `jobs/scripts/managed-cloud-release-gate.test.mjs` | Cover canonical release, artifact, promotion, and rollback gates |
| M | `jobs/workflows/Dockerfile` | Produce the pinned rootless closed workflows image/runtime inventory |
| M | `jobs/workflows/src/activities.ts` | Materialize and carry original A through managed-only application activities |
| M | `jobs/workflows/src/contracts.ts` | Add exact schema-v3 managed command/reconciliation and release memo contracts |
| M | `jobs/workflows/src/discovery-runtime.ts` | Expose reserved direct-discovery runtime observation without launch authority |
| M | `jobs/workflows/src/discovery-worker.ts` | Claim/report its reserved exact runtime role while launch feature remains false |
| M | `jobs/workflows/src/gateway-cleanup-service.ts` | Preserve original A in managed cleanup reconciliation without rewriting history |
| M | `jobs/workflows/src/gateway-service.ts` | Separate effect-capable schema v3 from exact lookup-only recovery |
| M | `jobs/workflows/src/gateway.ts` | Report gateway health and expose authenticated managed/historical recovery routes |
| M | `jobs/workflows/src/global-discovery-runtime.ts` | Expose reserved global-discovery runtime observation without launch authority |
| M | `jobs/workflows/src/global-discovery-worker.ts` | Claim/report its reserved exact runtime role while launch feature remains false |
| M | `jobs/workflows/src/worker.ts` | Claim and heartbeat the exact managed Temporal worker role |
| M | `jobs/workflows/src/workflows.ts` | Use distinct deterministic managed activities carrying original A |
| M | `jobs/workflows/tests/activities-auth.test.ts` | Cover exact materialize/runner A echoes and unchanged legacy shapes |
| M | `jobs/workflows/tests/gateway-cleanup-service.test.ts` | Cover managed release memo cleanup compatibility |
| M | `jobs/workflows/tests/gateway-http.test.ts` | Cover disabled/readiness routes and authenticated historical recovery |
| M | `jobs/workflows/tests/gateway-service.test.ts` | Prove recovery-only paths never create a Temporal effect |
| M | `jobs/workflows/tests/workflows.test.ts` | Cover deterministic A propagation only through managed branches |
| M | `ops/bluey-jobs.env.example` | Document every default-off gate and exact managed runtime configuration |
| A | `server/Dockerfile.jobs` | Build the pinned rootless Jobs API candidate image |
| A | `server/Dockerfile.jobs.dockerignore` | Close the Jobs API image context to production runtime inputs |
| M | `server/src/api/jobs.rs` | Derive public availability, admit/materialize A, and serve managed execution leases |
| A | `server/src/api/jobs_managed_cloud_releases.rs` | Serve closed administrative release and internal runtime control-plane routes |
| M | `server/src/api/jobs_runner_volumes.rs` | Bind managed volume proof to release A and runtime B identity |
| M | `server/src/api/jobs_worker_auth.rs` | Authenticate exact runtime, reconciliation, lease, and effect-boundary paths |
| M | `server/src/api/mod.rs` | Register the managed-cloud API authority module |
| M | `server/src/api/router.rs` | Scope the intentional bounded Axum error-envelope Clippy allowance |
| M | `server/src/api/router/completion.rs` | Scope the intentional completion error-envelope Clippy allowance |
| M | `server/src/api/router/embeddings.rs` | Scope the intentional embedding error-envelope Clippy allowances |
| M | `server/src/api/router/streaming_completion.rs` | Scope the intentional streaming error-envelope Clippy allowances |
| M | `server/src/api/router/transcribe.rs` | Scope the intentional transcription error-envelope Clippy allowance |
| M | `server/src/api/stt.rs` | Decode fixed PCM pairs through the Rust 1.98 array-chunk API |
| M | `server/src/bin/bluey-jobs-api.rs` | Validate/start managed reporters and independent dispatch/recovery loops |
| M | `server/src/db/jobs.rs` | Export and integrate managed release/execution authority |
| M | `server/src/db/jobs/execution_leases.rs` | Bind managed execution-lease claim, authorization, irreversible-effect, runtime-correlation, and receipt-replay authority |
| A | `server/src/db/jobs/managed_cloud_release_authority.rs` | Own signed release, runtime, readiness, admission, request-start, and effect validation |
| M | `server/src/db/jobs/runner_volume_purge.rs` | Preserve managed release binding across volume cleanup/deletion authority |
| M | `server/src/db/jobs/tests.rs` | Verify nested managed prelock and protected PostgreSQL admission lock ordering |
| M | `server/src/db/jobs/workflow_cleanup.rs` | Bind cleanup of managed workflows to the original release memo |
| M | `server/src/db/jobs/workflow_commands.rs` | Order admission/replay, persist original A, and split effect from recovery claims |
| M | `server/src/db/mod.rs` | Register paired migrations and extend replay/parity/schema guards |
| A | `server/src/jobs_managed_cloud_runtime.rs` | Measure, claim, heartbeat, drain, and fence Jobs API embedded runtime roles |
| M | `server/src/jobs_workflow_cleanup.rs` | Carry and validate the original managed release memo during cleanup |
| M | `server/src/jobs_workflow_dispatch.rs` | Recheck new-effect A and run independent exact lookup-only reconciliation |
| M | `server/src/lib.rs` | Export the managed-cloud runtime module |
| M | `server/src/main.rs` | Start validated managed runtime reporting and dispatch loops in shared server mode |
| M | `server/tests/integration_e2e.rs` | Prove cloud admission fails closed without signed managed authority and preserve bounded route/recovery coverage |
| M | `server/tests/jobs_runner_plan_matrix.rs` | Prove plan and legacy gates cannot bypass signed managed authority and keep route-contract assertions current |

## Build & Test

Baseline JavaScript and release-contract evidence at `423fba5c`, before the measurement-bound
correction:

```text
node --test jobs/scripts/managed-cloud-release-gate.test.mjs
  PASS: 16 tests

managed-cloud contract generation
  PASS: closed contracts generated from the current repository

(cd jobs && npm run typecheck)
  PASS: automation, browser, runner, workflows, and portal workspaces

(cd jobs && npm test)
  PASS: 1,749 tests
    automation 660; browser 219; runner 308; workflows 291; portal 271

(cd jobs && npm run build)
  PASS: all five workspaces and the production portal bundle
  NOTE: Vite emitted only its existing advisory for chunks larger than 500 kB
```

Baseline Rust and repository evidence at `423fba5c`, after the final PostgreSQL lock-order
correction:

```text
cargo fmt --all -- --check
  PASS

server binary cargo check
  PASS

cargo clippy --all-targets -- -D warnings
  PASS

cargo build --all-targets
  PASS

focused managed-cloud Rust tests
  PASS: 37 tests

PostgreSQL lock-order regression
  PASS: 1 test

workflow/contract determinism and generated-output comparison
  PASS

paired schema parity
  PASS

privacy, secret/flag, provenance, release-hygiene, and diff checks
  PASS

cargo test --manifest-path server/Cargo.toml --all-targets -- --test-threads=4
  PASS: 1,462 tests; zero failures
    lib 1,348; bluey-jobs-api 0; bluey-server 1; connectinfo 1;
    context migration 1; GDPR 2; integration_e2e 106;
    jobs_runner_plan_matrix 2; usage schema 1
```

The final independent line-by-line source review found no residual P0/P1/P2 issue across FIX-693,
FIX-694, and FIX-695, and the completed baseline full all-target Rust rerun passed with zero
failures. Release status remains conditional on exact-head resource-capable and hosted evidence.

Post-correction local evidence at the branch tip on 2026-08-24:

```text
node --test jobs/scripts/managed-cloud-release-gate.test.mjs
  PASS: 16 tests, including 512 files accepted and 513 rejected

Node strip-types syntax checks
  PASS: managed-cloud-runtime.ts and its focused test

cargo fmt --all -- --check
  PASS

git -P diff --check
  PASS
```

The first resource-capable pull-request rerun at head `f50103e8` used GitHub's synthetic merge
commit `f552ac2a` and produced Actions run `33313140429`, job `99261666660`. It passed the managed
release gate, 1,748 passed Jobs tests with one intentional skip, every Jobs typecheck/build, portal
parity, native runner format, Clippy, tests, and release build, then failed at the managed-runner
image measurement with:

```text
Runtime measurement root is missing: /usr/local/bin/node
```

That exact pinned Playwright Noble base installs the NodeSource package at `/usr/bin/node`.
FIX-694 normalizes that regular file into the already-authoritative `/usr/local/bin/node` before
measurement, removes the original after measurement, makes pull-request jobs check out and embed
the same head SHA, and rejects symbolic links throughout every measured runtime root. Four sibling
CI jobs also failed the new Rust 1.98
Clippy surface; FIX-695 backports the complete proven eleven-file repair from `2f3910a1` without
absorbing unrelated Phase 623 work. Local evidence after the final source correction:

```text
automation full test suite and typecheck
  PASS: 660 tests; 1 intentional skip; TypeScript clean

runner full test suite and typecheck
  PASS: 308 tests; TypeScript clean

managed-cloud release gate
  PASS: 17 tests

server Rust formatting
  PASS

git diff --check
  PASS
```

The complete automation and runner test/typecheck gates are now green locally. Rust
check/Clippy/tests plus the exact corrected managed-runner Docker build/native smoke remain
required. Docker is unavailable locally, so a resource-capable CI rerun must record the image and
native-addon result; the earlier successful stages cannot be inherited by either correction.

Hosted container publication, immutable registry/static read-back, Temporal task-queue behavior,
live runner capacity, runtime image-digest attestation, read-only-rootfs policy, customer cohort,
canary, and rollback are external-only gates and cannot be converted into local test claims.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Direct and global discovery are false in the importable release | Their existing lease routes do not bind an exact managed runtime session immediately before effect. Merely adding heartbeat processes would overclaim launch authority. |
| Original-source verification is reserved but false | The production worker entrypoint and original-source pre-effect contract are a successor batch, not evidence this round can invent. |
| Historical protocol-v2 recovery uses a dedicated route | Its byte shape must remain unchanged, while a distinct endpoint makes lookup-only semantics physically closed. |
| Runtime measurement covers the curated product boundary | Hashing every system library or dependency at process startup is neither stable nor sufficient platform proof. Signed hosted image-digest and read-only-rootfs canaries remain mandatory. |
| No release pipeline stage was executed | Publication, signatures, protected approvals, hosted attestation, customer cohort, canary, and rollback require explicit external authority. |

## Known Follow-ups

- Build the exact candidate once, verify stored OCI/static bytes in isolation, attach approved
  threshold signatures, publish immutable destinations, and prove full read-back without rebuild.
- Supply hosted Jobs API/workflows/runner image-digest and read-only-rootfs attestations.
- Rehearse live PostgreSQL multi-replica and network-fault behavior plus hosted Temporal
  start/Update/reconciliation and cleanup behavior.
- Prove real managed runner capacity, ATS/provider canaries, monitoring, kill switch, and exact
  higher-sequence rollback before any customer cohort or flag enablement.
- Close the direct-discovery, global-discovery, and original-source verification pre-effect runtime
  boundaries in separate reviewed batches before setting their signed feature authority true.
- Keep installed Bluey Browser release work parked unless later customer demand creates a separate
  P2 product requirement.

## Review Checklist (for reviewer)

- [x] Final branch diff contains exactly 93 inventoried paths
- [x] Files match the Round 611 scope with no unrelated or `docs/reviews/` changes
- [x] New-effect availability requires exact live release and role quorum with no debug bypass
- [x] Idempotent replay precedes mutable admission checks and preserves original A
- [x] Request-start reconciliation is lookup-only and can never create a second effect
- [x] Managed execution binds A plus fresh compatible runner B immediately before I/O
- [x] Recovery, receipt, cleanup, and terminal paths survive mutable-head/B loss without new effects
- [x] Paired schema parity, focused tests, builds, privacy, flags, provenance, and diff checks pass
- [x] All customer flags remain `0` and hosted evidence is not claimed from local source
- [x] No TODO lacks a tracked follow-up
- [x] Baseline `423fba5c` full all-target Rust rerun passes with zero failures
- [ ] Final corrected branch tip passes automation Vitest, Rust check/Clippy/tests, and exact managed-runner Docker/CI gates
