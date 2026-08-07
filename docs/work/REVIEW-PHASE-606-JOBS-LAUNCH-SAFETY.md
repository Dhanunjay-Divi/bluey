# REVIEW: PHASE-606 - Jobs Launch Safety Control Plane

> **Codex preflight:** Loaded `$bluey-ops` and reviewed the complete Phase 606
> local snapshot against current source and evidence. No SSD/archive, provider
> credential, live tenant, production service, deployment, or external write
> was used.

**Reviewed code snapshot:**
`72056e6db026dcd25ebe54bd41e1681e4185c727`

**Commit range:**
`936fba2419e093eb6dd9d27a764d3c4bc8c6fb25..72056e6db026dcd25ebe54bd41e1681e4185c727`

**Repository index:** source commit frozen; final documentation closeout is a
separate docs-only commit

**Reviewers:** independent Phase 606 code-audit agent with focused
context-projection and lock/recovery reviewers; final evidence assembled by the
root agent

**Date:** 2026-08-07

## Per-Task Review

### Canonical hold authority, API, readiness, and metrics

| Field | Value |
|-------|-------|
| Files | Paired migrations, operational-hold DB module, admin API, readiness, metrics, schema guards |
| Verdict | 🟢 accept |

**Findings:**

- Append-only canonical events, monotonic compare-and-swap heads, exact replay,
  predecessor ancestry, and immutable paired database constraints fail closed
  for malformed or mismatched current state.
- Both server routers require administrator bearer authority. Mutation bodies
  are strict and bounded; success and errors are private/non-storable.
- Lists expose encrypted pagination and opaque references only. Metrics use a
  closed capability/scope label set; reason, actor, event, and raw scope
  identity remain private.
- Generic readiness composes with paused discovery sources and open signed ATS
  circuits without allowing one authority to release another.

### Transactional admission and exact post-marker recovery

| Field | Value |
|-------|-------|
| Files | Discovery, generation, reservation, runner, final-submit, mailbox, communication, local recovery, object capacity |
| Verdict | 🟢 accept |

**Findings:**

- Every new protected authority evaluates the matching hold inside the same
  database serialization boundary as its claim, reservation, or irreversible
  marker. Bounded candidate scans do not let a held queue prefix starve later
  eligible work.
- Heartbeat, exact provider request-start replay, `click_started` recovery,
  result/receipt persistence, mailbox completion, communication completion,
  and reconciliation remain reachable because they reduce uncertainty after a
  possible side effect.
- Local recovery preserves the frozen Browser build/release binding. A current
  server `A -> B` change is accepted only when the immutable bound activation
  accepted both IDs; another server ID remains denied.
- Expired signed-v2 submit grace, distribution pause, and storage-maximum drift
  reach only exact durable recovery. Existing capacity is reused under current
  upload limits; no marker, ATS authority, or capacity is reminted.

### Snapshot, projection, and lease hardening

| Field | Value |
|-------|-------|
| Files | Operational contexts, discovery/materialization, Track writes, mailbox sync, eligibility |
| Verdict | 🟢 accept |

**Findings:**

- PostgreSQL readers and all relevant writers share the discovery-account
  advisory fence with consistent advisory, parent-account, then child-row lock
  order. Application/job context validates binding and locks deterministic
  posting, membership, source, and Track projections.
- Track mutation is denied while a relevant source lease is unexpired.
  Account-wide curated discovery validates relational/JSON ID and active parity
  for every Track and selects active Tracks only. A directly bound inactive
  Track still contributes Career Track and Region scopes.
- A `curated_feed:*` posting without the exact account-level managed membership
  fails closed for both application and job context until repaired.
- Mailbox direct/batch claims validate canonical connection, relational sync,
  and encrypted sync provider projections. An `unassigned` reservation omits
  runner-kind scope; concrete claims evaluate cloud/local before binding.

### Diagnostics, privacy, and operational handoff

| Field | Value |
|-------|-------|
| Files | Discovery health script/tests, privacy/schema/CI guards, operations and work docs |
| Verdict | 🟢 accept |

**Findings:**

- Health output uses keyed opaque source references. The diagnostic key and
  database URL cross only inherited file descriptors, and child environments
  omit database, data, diagnostic, and provider secrets.
- The final staged-index privacy gate rejected ambiguous credential-shaped test
  literals; FIX-675 marked them explicitly dummy without weakening the guard.
- Operations documentation preserves the source-only boundary and never treats
  hold release as feature enablement or production authority.

## Build & Test Verification

```text
Jobs workspace                         1,507 tests / 127 files passed
Jobs typecheck/build                       5/5 + 5/5 passed
Portal bundle                          2,292 modules / 27 files / no maps
Portal aggregate SHA-256               8eadf40bc36d19daaae2169342d08dd5a140a3bd0e0d676b989c87e9cff0bcee
Server fmt/check/strict Clippy          passed; all targets; warnings denied
Server Rust                             1,351 tests passed
Native runner storage                      14 tests + release build passed
Schema parity                              71 tables / 66 indexes passed
Dependency/source provenance            663 entries / 14 repositories passed
Browser release/account-delete guards     10/10 + 3/3 passed
CI, privacy, diagnostic, docs, tracing   passed
Independent final code audit             no blocker or minor
```

Optional PostgreSQL migration/concurrency tests compiled and self-skipped
without `BLUEY_TEST_POSTGRES_URL`. Docker is unavailable locally, so the
unchanged managed-runner image smoke was not run. Neither omission is
represented as live or hosted evidence.

## Overall Verdict

🟢 **ACCEPT - SOURCE COMPLETE AND LOCALLY VERIFIED.** Phase 606 closes its
source acceptance criteria at the reviewed code snapshot. Merge still requires
fresh hosted exact-SHA checks. No credential, provider write, device, tenant,
artifact promotion, deployment, canary, or production flag authority is
claimed.

## Follow-ups for Next Batch

- Phase 607: installed Bluey Browser update, rollback, boot verification, and
  crash-recovery authority before local distribution certification.
- Authorized live PostgreSQL migration/concurrency and backup/restore rehearsal.
- Immutable artifact publication/read-back, administrator/monitoring ownership,
  incident/canary drill, physical-device certification, and hosted exact-SHA
  checks.
