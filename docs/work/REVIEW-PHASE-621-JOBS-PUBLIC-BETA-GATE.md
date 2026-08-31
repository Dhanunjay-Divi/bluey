# REVIEW: PHASE-621 — Jobs Public Beta Gate

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the current source, Round 621,
> implementation record, security fixes, and final local evidence before review.

**Commit range:**
`3f90bc01210345df56f24a8e95a493895c9744ae..95696dd0ce204c06ed382ed73f52d46920501bfb`

**Aggregate source plus FIX-784:** `95696dd0ce204c06ed382ed73f52d46920501bfb`

**Generated portal and dependency-security commit:** `b3d0c79cecc44ca45bf31e6fbe2838108bfabe3a`

**Reviewer:** Independent database/security, fixture, portal, and final source reviewers;
consolidated by the primary implementation agent

**Date:** 2026-08-31

**Review status:** 🟢 ACCEPT for the bounded local source commit; production activation remains
NO-GO pending the release gates below

## Per-Task Review

### Durable public cohort and access authority

| Field | Value |
|-------|-------|
| Files | Paired migrations, `jobs_beta_access` database/API modules, server router integration |
| Verdict | 🟢 accept |

**Verified:**

- Public admission is verified-account-only, first-come, durable, capped, sticky, and transactional
  on SQLite and PostgreSQL paths.
- Draft, open, closed-to-new, suspended, denial, deletion intent, and master-off states fail closed.
- Capacity is cumulative and cannot be reclaimed or over-admitted by denial, deletion, or replay.
- Public status exposes no account, cap, count, position, revision, or release identity.

### Administration, audit, privacy, export, and telemetry

| Field | Value |
|-------|-------|
| Files | Administration methods, operations audit, metrics, owner export, private response middleware |
| Verdict | 🟢 accept |

**Verified:**

- Production mutations require a current real administrator and transaction-coupled redacted audit.
- Audit failure rolls back the mutation; deletion-pending actors and subjects cannot mutate access.
- Status and error responses are private/non-storable with authorization variance.
- Owner export includes beta-only state without creating a Jobs profile; telemetry remains aggregate
  and rejects impossible live-versus-assigned counts.

### Effect boundary and portal composition

| Field | Value |
|-------|-------|
| Files | Job/application/communication claims, local runner, workspace API, portal gate and tests |
| Verdict | 🟢 accept |

**Verified:**

- Cohort admission grants portal access only; it does not grant a runner, provider, Career Track,
  message, or application authority.
- Current beta, deletion, entitlement, release, runner, ATS, and consent checks remain independent.
- Exact already-started replay and lookup-only reconciliation remain reachable after a mutable gate
  closes, without granting a second effect.
- Portal access is resolved before workspace data and cannot bypass the server gate.

## Cross-Task Findings

- FIX-768 through FIX-784 are closed in local source. The final independent reviews reported no
  P0-P3 findings; exact-tip hosted completion remains pending.
- The final source defaults remain master-off, cohort `draft`/cap `0`, and all external-effect flags
  off. The implementation cannot open production by migration or UI state.
- The capped public product decision is not invitation-only and does not authorize fake
  applications, fabricated candidate claims, arbitrary outreach, or provider-control bypass.

## Build & Test Verification

```text
PASS  Jobs workspaces: 1,983 tests; one intentional no-egress simulator skip
      automation 792; Browser 219; runner 308; workflows 300; portal 364
PASS  all five Jobs workspace typechecks and production builds
PASS  repeat portal build: 34 files; digest
      bf6ab3febf060cddc303be099cd01cc4c3d79d1c0a3a05d625b19e1d3f5d2042
PASS  npm production/full audits: 252/673 dependencies; zero advisories
PASS  server library: 1,611/1,611
PASS  HTTP integration: 113/113
PASS  server all-target check and strict all-target Clippy (`-D warnings`)
PASS  repository formatting and diff checks
PASS  paired schema parity: 105 tables / 90 indexes
PASS  privacy: 2,751 tracked paths / 2,475 text files
PASS  provenance: 663 lock entries / 631 package versions / 14 pinned repositories
PASS  Browser release 10/10; managed-cloud release 18/18; deletion 3/3
PASS  business-messaging containment and CI guard self-tests
PASS  native runner storage: fmt, strict Clippy, 14 tests, release build, Darwin N-API smoke
PASS  FIX-784 focused test 1/1 and full test target 2/2; fmt and strict Clippy green
PASS  independent aggregate and FIX-776 through FIX-784 reviews: no P0-P3 findings
```

## Overall Verdict

🟢 **ACCEPT** — The Phase 621 local source is ready for its bounded feature-branch commit and
exact-tip CI. This verdict does not authorize a merge, deployment, cohort opening, flag change,
provider connection, external write, or production release.

## Follow-ups for Release

- Require green exact-tip CI for the pushed source, including the managed-runner Docker build/smoke
  unavailable on this host.
- Execute isolated hosted PostgreSQL migration/race/audit evidence and read back the dark cohort.
- Prove preproduction cap-2 concurrency, suspend/resume, backup/restore, monitoring, and rollback.
- Open production cap 25 only after the separately reviewed Phase 622 runtime/provider gates pass.
