# REVIEW: PHASE-601 - Jobs Evidence Lifecycle and Exact Submit

> **Codex preflight:** Loaded `$bluey-ops`, reconciled its Jobs authority and
> release boundaries against the current branch, and reviewed only the isolated
> Phase 601 worktree.

**Commit range:** `89239675..HEAD`
**Reviewer:** Codex self-review with independent exact-submit and lifecycle audits
**Date:** 2026-08-05

## Per-Task Review

### FIX-601 - Exact employer request and confirmation proof

| Field | Value |
|-------|-------|
| Files | `jobs/automation/`, exact-proof server validation, and focused tests |
| Verdict | 🟢 accept |

**Findings:**

- The approved provider job, effective target, successful-control order, PDF
  hashes, request bytes, execution authority, and receipt are bound end to end.
- Audit findings for unintended redirect statuses, ambiguous returned forms,
  mixed positive/negative confirmation text, and a looser workflow status
  parser were fixed with exhaustive browser, receipt, adapter, workflow, and
  server matrices.
- Multipart documentation now describes the implemented boundary-preserving
  hydration: Chromium's boundary, headers, text bytes, and order remain intact;
  Node inserts only independently verified file bodies omitted by interception.

### FIX-602 - Offline launch and submitted-result recovery

| Field | Value |
|-------|-------|
| Files | `jobs/browser/`, `jobs/runner/`, `jobs/workflows/`, and recovery tests |
| Verdict | 🟢 accept |

**Findings:**

- Fresh and recovered pages stay offline until Bluey selects one bound page,
  rejects surviving service workers, installs the network guard, and navigates.
- Staged submitted results replay only under the same account, application,
  identity, session, run, request, lease token, and fence authority.
- Corrupt state is isolated by browser-profile scope; ambiguous side effects do
  not become retryable submissions.

### FIX-603 - Complete confirmation screenshot evidence

| Field | Value |
|-------|-------|
| Files | receipt materialization, evidence persistence/download, portal UI, and tests |
| Verdict | 🟢 accept |

**Findings:**

- Every one-to-four screenshot set has exact ordered manifest coverage, unique
  immutable object identity, account scoping, integrity checks, and portal
  rendering; coherent legacy single-image receipts remain readable.

### FIX-604 - Evidence capacity and account-deletion lifecycle

| Field | Value |
|-------|-------|
| Files | server object/deletion paths, paired migrations, API/portal integration, and tests |
| Verdict | 🟢 accept |

**Findings:**

- Evidence capacity is reserved before irreversible action, consumed only by a
  valid receipt, released for definite non-submissions, and retained for
  unknown outcomes.
- The durable deletion fence, live-writer drain, account-prefix purge, and
  database removal order fail closed across SQLite and PostgreSQL semantics.
- Independent audit found that absent optional storage configuration could skip
  an orphan-prefix sweep. Deletion now requires the artifact and effective audit
  stores after fencing, deduplicates a shared namespace, and preserves the
  account/fence with `503` on missing or failed storage proof.
- Returning `409 Conflict` for known cloud-runner state is deliberate until a
  signed multi-runner volume-purge acknowledgement protocol exists.

## Cross-Task Findings

- Portal verification was aligned with server download authority for the
  canonical submission timestamp and nonblank browser confirmation, and schema
  parity coverage now includes the two new lifecycle tables and indexes.
- A pre-existing macOS stealth test-state race surfaced during the root gate.
  Commit `8d423fff` serializes the two omitted global-environment tests and adds
  `FIX-605`; 2,000 stress runs and the full root suite passed afterward.
- No production flag, runtime, service, database, object bucket, credential, or
  meeting-owned checkout was changed.
- This review proves local source merge readiness, not public unattended ATS
  certification or production launch readiness.

## Build & Test Verification

```text
Jobs tests                 988 passed
  automation               530
  browser                  151
  runner                   110
  workflows                 76
  portal                   121
Jobs strict typechecks     five workspaces passed
Jobs production builds    five workspaces passed (Vite chunk-size warning only)
Automation export smoke   passed

Server library tests      936 passed
Signed HTTP E2E tests      98 passed
Auxiliary server tests      6 passed
Server fmt                 passed
Server all-feature Clippy  passed with warnings denied
Jobs API compile check     passed

Root cargo fmt             passed
Root cargo Clippy          passed with warnings denied
Root release cargo build   passed
Root cargo tests           passed
```

Final policy verification passed: the staged privacy gate scanned 2,326 paths
and 2,054 text files; schema parity covered 8 tables and 13 indexes; CI guard
self-tests, provenance/license inventory, operating-doc coverage, tracing/PII
analysis, and the staged diff check all passed.

## Overall Verdict

🟢 **ACCEPT** - Ready to merge as source; all final staged policy guards passed.

## Follow-ups for Next Batch

- Add a signed, replay-safe multi-runner account-volume purge protocol with
  complete fan-out acknowledgement and legacy-volume reconciliation.
- Run authorized Greenhouse/Lever tenant certification, isolated live
  PostgreSQL concurrency/migration tests, real R2/S3 fault tests, and physical
  device power-loss tests when their external environments are available.
- Keep model generation, Browser distribution, mailbox sync, and
  employer-facing execution flags disabled until their independent gates pass.
