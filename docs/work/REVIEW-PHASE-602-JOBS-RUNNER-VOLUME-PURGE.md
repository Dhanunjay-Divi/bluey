# REVIEW: PHASE-602 - Jobs Signed Runner-Volume Purge

> **Codex preflight:** Loaded `$bluey-ops`, reconciled its Jobs authority and
> release boundaries against the current branch, and reviewed only the isolated
> Phase 602 worktree.

**Commit range:** `3c96f554..HEAD`
**Reviewer:** Codex self-review with independent runner/native and server-authority audits
**Date:** 2026-08-05

## Per-Task Review

### Volume identity, deletion fan-out, and server authority

| Field | Value |
|-------|-------|
| Files | paired migrations, `server/src/db/jobs/runner_volume_purge.rs`, API and account lifecycle integration |
| Verdict | 🟢 accept |

**Findings:**

- One-time admission, immutable key epochs, process leases, signed commands,
  volume-signed ACKs, destruction evidence, tombstones, and cutover generations
  remain distinct and replay-safe.
- Account fencing precedes conservative fan-out, offline targets remain in the
  immutable required set, and no lifecycle lock is held while the HTTP caller
  receives durable pending status.
- The server authority review traced outer HMAC and exact Ed25519 payload
  verification into typed database operations; no worker HMAC alone can forge a
  volume acknowledgement or current-storage record.

### Native storage and durable runner integration

| Field | Value |
|-------|-------|
| Files | `jobs/runner/native-storage/`, managed storage/layout/residency modules, durable runner stores |
| Verdict | 🟢 accept |

**Findings:**

- Production opens and locks the configured root before identity I/O and uses
  retained, no-follow, beneath-root, same-device operations for account data.
- Symlink ancestors, root and parent replacement, hardlinks, special files,
  mount mismatch, unsafe moves, and a second process fail closed in native tests.
- The implementation correctly limits its claim: host root, kernel compromise,
  and same-credential sequential clones remain outside software-only proof.

### Paginated purge and current-storage recovery

| Field | Value |
|-------|-------|
| Files | runner volume client/purger, server keyset poll, focused and fault-matrix tests |
| Verdict | 🟢 accept |

**Findings:**

- Independent review found a P1 global-legacy deadlock: the first command could
  delete its own target, fail global zero, and prevent later authorized subjects
  from running. `FIX-607` adds a durable prepare-all barrier, stable keyset
  pagination, bounded resume, and finalization only after complete global zero.
- Independent review found a P1 ambiguous-attestation replay hazard after an
  intervening purge. `FIX-608` resolves a pending body before mutation and binds
  exact retry to predecessor, enrollment, tombstones, and local evidence
  revision.
- Final review found a related P1 post-purge recovery error: exact-base retry
  was incorrectly gated on `storageAttestationRequired`. The invalid hint gate
  was removed while all exact-binding checks remain; a regression proves a lost
  generation-two body retries byte-for-byte while generation one is current.
- Five commands across three pages are all prepared before the first ACK, and a
  reconstructed purger resumes durable journals between phases.

### Deletion clients and release gates

| Field | Value |
|-------|-------|
| Files | CLI/cloud/dashboard/web deletion paths, operations docs, Dockerfile, CI and release workflows |
| Verdict | 🟢 accept |

**Findings:**

- Every client treats `202` as pending and retains credentials until a verified
  final `200` response.
- Review nits found that Darwin built but did not load the release library and
  that both container bases were mutable. Workflows now stage the dylib under
  the real `.node` name and run the N-API smoke; both bases use immutable
  official multi-architecture digests.
- Actual Docker execution is not claimed locally because no Docker-compatible
  runtime is installed. Linux image build/load remains a mandatory CI/release
  job, and production flags stay disabled.

## Cross-Task Findings

- Acceptance criteria 1-8 are covered by shared Node/Rust vectors, admission and
  replay tests, deletion lifecycle/fan-out tests, purge evidence, tombstones,
  three-volume fault matrices, and SQLite/PostgreSQL parity.
- Criteria 9-15 are covered by the fault matrix, honest documentation, fleet
  cutover evidence, all deletion clients, out-of-order generation tests, native
  retained-handle matrices, and the unprivileged container contract.
- Criteria 16-18 are covered by bounded multi-page preparation, crash resume,
  pending-attestation matrices, digest pins, and exact native artifact load
  tests.
- No production service, database, volume, object store, credential, feature
  flag, or meeting-owned checkout was changed.

## Build & Test Verification

```text
Jobs tests                 1,127 passed
  automation                 530
  browser                    151
  runner                     249
  workflows                   76
  portal                     121
Focused client/purge          42 passed
Jobs typecheck/build           5 workspaces passed
Automation smoke               passed

Server library               977 passed
Signed HTTP integration       99 passed
Auxiliary server               6 passed
Server fmt/check/Clippy        passed

Native storage                14 passed
Native release/load smoke      passed
Root Rust                  1,045 passed, 17 ignored
Root fmt/Clippy/release        passed
Dashboard UI                  35 passed; build passed
macOS overlay/whisper          release builds passed

Schema parity                 23 tables, 28 indexes
Final staged privacy          2,376 paths / 2,103 text files passed
Policy/self-verifiers          passed
Workflow/static/diff gates     passed
```

The final index contains only the 100 reviewed Round 602 files. Its privacy
scan, staged diff check, schema/provenance/license guards, and source checks all
pass without a finding.

## Overall Verdict

🟢 **ACCEPT** - Ready to commit as source; production rollout remains gated.

## Follow-ups for Next Batch

- Add signed local Bluey Browser release/build/protocol authority, immutable
  artifact manifests, exact packaging verification, revocation/rollback, and
  authoritative portal download metadata in Round 603.
- Run live PostgreSQL/object-store, provider-volume, Linux/Windows, and physical
  device certification only with approved environments and credentials.
- Keep model generation, Browser distribution, mailbox sync, and
  employer-facing execution flags disabled until their independent gates pass.
