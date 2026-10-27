# Round 629 — Jobs non-production PostgreSQL and Linux/container validation

> Codex preflight: `bluey-ops` and `agent-toolkit`. Current Jobs code and local
> handoff are authority; no SSD/historical checkout is needed.

Date: 2026-10-27 (America/New_York).
Branch: `feat/phase-629-jobs-nonprod-integration-validation`.
Base: `130ef807bc952874a2cb391e43511d7364ab1b0a`.
Status: bounded nonproduction validation complete. PostgreSQL 30/30, Jobs quality,
Linux/Docker and full all-target Rust 1,838/1,838 passed; no production release approval.

## Owner authorization and separation

The owner approved non-production PostgreSQL and Linux/Docker testing after the
Phase 628 local handoff. Deployment, real applications and real emails remain
prohibited. Production-account access is not required for the planned checks;
do not mutate a production account, borrow production credentials or point any
test at an owner/shared database.

Use this Jobs worktree only. Do not edit or start the Bluey host/overlay, the
meeting-owned `/Users/uno/Downloads/cue` checkout, Pinky services or their VMs.
Existing shared library dependencies are not permission to change those
products. No push, hosted workflow dispatch, registry publication, production
flag, production deployment or paid infrastructure is part of this batch.

## Acceptance sequence

1. Establish a fresh task-owned PostgreSQL cluster/database using installed
   PostgreSQL 17, verify the actual server version and pgvector extension, and
   install cleanup before startup. Bind only a private socket/loopback port;
   remove inherited database URLs before explicitly supplying this cluster's
   URL inside the isolated test boundary. Never use the default PostgreSQL
   service or an existing database.
2. Execute migrations and the actual PostgreSQL readiness/replay/concurrency,
   operational lease, immutable attempt and cascade paths. A test returning
   early without configuration is not a pass for this gate. Add bounded tests
   or harness corrections where current SQLite-only fixtures leave a gap.
   Independent cases receive separate freshly generated databases within the
   owned cluster; global activation fixtures must not leak across scenarios.
3. Build the exact managed-runner Dockerfile with its pinned bases in a
   task-owned Linux runtime, then run the normal native-addon loader/smoke with
   read-only root and a single task-owned writable runner volume. Preserve
   architecture, source revision, artifact identity and measured limitations.
   Emulated amd64 execution must never be called native amd64 hardware proof.
4. Exercise the existing mock-ATS/no-provider boundary and applicable API/data
   integration flows using synthetic accounts, resumes, jobs and mail fixtures.
   This does not authorize a real employer submit or mailbox send.
5. Obtain separate review, record commands/counts/artifact identities, and
   verify cleanup of only the exact owned clusters, build roots, containers,
   volumes, builders and VM profile. Keep the user-facing fixture preview's
   existing bounded lifetime separate from test cleanup.

## Capacity and evidence policy

Use one Mac-heavy workload at a time through the shared `mac-build` queue lock.
No broad Docker/system/Cargo prune, shared-cache deletion, existing VM reuse or
production backup deletion. Stop on insufficient capacity rather than changing
another product's environment. Fix real failures at their source; do not loosen
readiness, authority, receipt, deletion, signature or submission fences to make
the tests green.

The observed initial host has 32 GiB RAM, 10 logical CPUs and about 42 GiB free.
Installed PostgreSQL 17.10 and pgvector 0.8.3 were subsequently exercised in
private clusters. Colima/QEMU were used only through the dedicated Jobs VM,
which has now been deleted; observed pre-existing profiles remain stopped.
The Jobs validation must use its own profile/context without home, SSH,
Keychain, overlay or signing-resource mounts.

## Executed checkpoint

- Configured PostgreSQL: 30 exact cases, each with a fresh private database,
  all passed; clean server shutdown and PID-file absence verified.
- Rust formatting and strict all-target Clippy passed. Full server all-target
  tests passed with exit 0: 1,703 library, one binary and 134 integration, zero
  failures/ignored. Exact root removed automatically; all 20 task parents empty.
- Jobs: 2,025 tests, five typechecks/builds, installed audit zero findings;
  actual Chromium synthetic exact-submit case passed, not skipped. Release
  checks 32/32, schema parity 113 tables / 91 indexes, CI guards passed.
- Pinned Linux/amd64 Docker build under QEMU, exact-image hardening/native smoke
  and one unit plus 13 storage integration cases passed. This is emulated local
  execution, not native hardware/performance or a signed release artifact.
- Owned Linux VM, containers and volume removed; existing product VMs, default
  Docker context and host/overlay code unchanged. See IMPL for exact identities,
  failure history and retained release holds.

## Deliverables

- Bounded implementation/harness changes and FIX records for proven defects.
- `docs/work/IMPL-PHASE-629-JOBS-NONPROD-INTEGRATION-VALIDATION.md` with observed
  results, failures, exact resources and cleanup evidence.
- A separately authored review in `docs/work/`, without changing the inherited
  Phase 628 canonical REVIEW or frozen `docs/reviews/`.
- Honest remaining release holds: no local/emulated test implies hosted CI,
  registry read-back, signed activation, provider certification, live capacity,
  rollout/rollback rehearsal or permission to enable autonomous production.
