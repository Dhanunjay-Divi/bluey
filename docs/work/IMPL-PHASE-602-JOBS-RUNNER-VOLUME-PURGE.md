# IMPL: PHASE-602 - Jobs Signed Runner-Volume Purge

> **Codex preflight:** Loaded `$bluey-ops`, reconciled its Jobs and release
> boundaries against this isolated worktree, and left the meeting checkout,
> credentials, production services, and managed volumes untouched.

## Scope

**Does:**

- Gives every managed persistent runner volume an immutable Ed25519 identity,
  one-time admission authority, process-instance lease, residency ledger, and
  chained current-storage attestation.
- Durably fences account deletion before conservatively freezing every enrolled
  non-destroyed volume, then returns honest `202 Accepted` pending state without
  holding lifecycle locks while offline targets work.
- Delivers server-signed, replay-safe purge commands and verifies
  volume-key-signed acknowledgements, independent destruction evidence, opaque
  restore tombstones, and exact fleet-cutover evidence.
- Places account-bearing profiles, snapshots, checkpoints, staged results,
  receipts, recovery data, and temporary files beneath an owner-private native
  retained-handle boundary with durable account residency.
- Uses signed keyset pagination and a prepare-all-before-first-ACK barrier so
  multi-subject legacy storage can reach global zero without deadlock.
- Resolves response-ambiguous storage attestations before mutation through exact
  promotion, byte-identical retry, obsolete-binding discard, or fail-closed
  conflict handling.
- Keeps CLI, dashboard, and web credentials until the server proves final hard
  deletion, and adds paired migrations, operations contracts, hardened
  container/workflow gates, and fault coverage.

**Does NOT:**

- Deploy or restart a service, run a production migration, enroll or mutate a
  production volume, read or rotate credentials, reconcile a live legacy root,
  or change a Jobs feature flag.
- Claim physical-sector erasure, control of unmanaged copies, sequential-clone
  identity, live PostgreSQL/object-store certification, or provider-managed
  destruction without separately authorized evidence.
- Claim Linux, Windows, or physical-device certification from the local Darwin
  source gate.
- Enable local/cloud Browser distribution, mailbox sync, managed generation, or
  employer-facing execution.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `infra/{sqlite,postgres}/server-runtime/*runner_volume_purge.sql` | Created | Dialect-paired volume, purge, attestation, tombstone, and cutover authority. |
| `server/src/db/jobs/runner_volume_purge.rs` | Created | Transactional authority, fan-out, keyset delivery, ACK, attestation, destruction, and cutover logic. |
| `server/src/api/jobs_runner_volumes.rs` | Created | Strict codecs, signed worker/admin routes, cursor authority, and readiness boundary. |
| `server/src/{api,db}/` | Modified | Account deletion, work leases, object writes, local reconciliation, migrations, and tests. |
| `jobs/runner/native-storage/` | Created | Cross-platform retained-handle N-API boundary and native fault tests. |
| `jobs/runner/src/{native-runner-storage,safe-runner-storage,subject-storage-manager}.ts` | Created | Native root acquisition and managed subject layout. |
| `jobs/runner/src/{account-residency,volume-identity,storage-attestation}.ts` | Created | Durable identity, residency, and chained current-storage evidence. |
| `jobs/runner/src/{legacy-runner-storage,purge-storage-evidence,volume-purge}.ts` | Created | Classified legacy inventory, signed purge journals, and zero-evidence ACKs. |
| `jobs/runner/src/runner-volume-client.ts` | Created | Enrollment, process lease, paginated two-phase purge, attestation recovery, and readiness. |
| `jobs/runner/src/` durable stores and server | Modified | Route every account-bearing write through residency and storage authority. |
| `jobs/runner/tests/` | Created/modified | Native, filesystem, protocol, pagination, replay, crash, and three-volume fault matrices. |
| `crates/cue-{cli,cloud-client,dashboard}/` | Modified | Durable pending deletion and verified completion handling. |
| `web/assets/bluey-account-delete.*` and `web/index.html` | Created/modified | Browser deletion credentials survive pending responses. |
| `jobs/OPERATIONS.md`, `ops/*.env.example` | Modified/created | Managed volume, root, identity, purge, and rollout contracts. |
| `jobs/runner/Dockerfile`, `.github/workflows/{jobs-ci,release}.yml` | Modified | Unprivileged digest-pinned image and exact Linux/Darwin native-addon smoke. |
| `jobs/scripts/{check-jobs-schema-parity,ci-guards-self-test}.mjs` | Modified | Parity coverage for 23 tables/28 indexes and current negative fixtures. |
| `CHANGELOG.md`, Round 602 and work docs | Modified/created | Release note, decisions, fixes, verification, and limitations. |

## Build & Test

```text
Jobs tests                       1,127 passed
  automation                       530
  browser                          151
  runner                           249
  workflows                         76
  portal                           121
Focused runner client/purge         42 passed
Jobs strict typecheck/build          5 workspaces passed
Automation export smoke              passed

Server tests                     1,082 passed (977 lib + 99 HTTP + 6 auxiliary)
Server fmt/check/strict Clippy       passed
Native storage tests                  14 passed
Native release artifact/load smoke   passed on Darwin

Root Rust tests                   1,045 passed, 17 ignored
Root fmt/strict Clippy/release build  passed
Dashboard tests/build                35 passed; build passed
macOS overlay and whisper builds      passed

Schema parity                         23 tables, 28 indexes
Final staged privacy                  2,376 paths / 2,103 text files passed
Policy/self-verifiers                 passed
Workflow YAML, scripts, diff          passed
```

Docker tooling is not installed locally. The digest and workflow contract tests
pass, while the actual Linux image build and in-image N-API load remain explicit
CI/release gates and are not represented as local execution evidence.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| Purge delivery uses two phases and signed stable keyset pagination. | Per-command finalization cannot prove global legacy zero when later signed commands authorize other subjects; preparing all pages before any ACK is the required end-state invariant. |
| Pending attestations carry a local storage-evidence revision. | Network ambiguity must be resolved before later purge preparation mutates the signed snapshot; server state alone cannot distinguish every local transition. |
| Darwin workflows stage the dylib as the real `.node` loader target before smoke. | Building a native library without loading the exact staged artifact did not prove the shipped Node boundary. |
| Container bases are pinned to immutable multi-architecture digests. | Mutable base tags cannot support reproducible release evidence. |

## Known Follow-ups

- Round 603 should add signed local Bluey Browser release manifests, build and
  protocol authority, exact artifact verification, rollback/revocation, and
  authoritative portal download metadata while keeping distribution disabled.
- Run authorized live PostgreSQL/object-storage fault exercises and managed
  volume enrollment, reconciliation, destruction, and cutover ceremonies.
- Complete Linux/Windows and physical-device native storage, crash, power-loss,
  install, and rollback certification before enabling Browser distribution.
- Keep all Jobs generation, Browser, mailbox, and employer-facing production
  flags disabled until their independent gates pass.

## Review Checklist (for reviewer)

- [x] Files match the integrated Round 602 scope described above
- [x] No production configuration, deployment, credential, or live data is included
- [x] Tests cover all Round 602 acceptance criteria and audit regressions
- [x] Rust, TypeScript, native, UI, and policy quality gates pass
- [x] External infrastructure, provider, and device evidence is not overclaimed
