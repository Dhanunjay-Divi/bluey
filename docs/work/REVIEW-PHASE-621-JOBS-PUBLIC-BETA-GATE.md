# REVIEW: PHASE-621 — Jobs Public Beta Gate

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the current source, Round 621,
> implementation record, security fixes, and final local evidence before review.

**Commit range:** `3f90bc01210345df56f24a8e95a493895c9744ae..95bb966fd897db94599a0c7e2defe1fb02ea3912`

**Source commit:** `55d24e95234c1fcc2db22a912c739dfd5760eb66`

**Generated portal commit:** `95bb966fd897db94599a0c7e2defe1fb02ea3912`

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

- FIX-768 through FIX-778 are closed. The final independent reviews reported no P0-P3 findings.
- The final source defaults remain master-off, cohort `draft`/cap `0`, and all external-effect flags
  off. The implementation cannot open production by migration or UI state.
- The capped public product decision is not invitation-only and does not authorize fake
  applications, fabricated candidate claims, arbitrary outreach, or provider-control bypass.

## Build & Test Verification

```text
PASS  Jobs workspaces: 1,982 tests; one intentional no-egress simulator skip
      automation 791; Browser 219; runner 308; workflows 300; portal 364
PASS  all five Jobs workspace typechecks and production builds
PASS  repeat portal build: 32 files; digest
      65be1e2ebda67b02fcd1c9340240bb0100aade36a2748c2266369898b3d65235
PASS  server library: 1,611/1,611
PASS  HTTP integration: 113/113
PASS  server all-target check and strict all-target Clippy (`-D warnings`)
PASS  repository formatting and diff checks
PASS  paired schema parity: 105 tables / 90 indexes
PASS  privacy: 2,719 tracked paths / 2,443 text files
PASS  provenance: 663 lock entries / 631 package versions / 14 pinned repositories
PASS  Browser release 10/10; managed-cloud release 17/17; deletion 3/3
PASS  business-messaging containment and CI guard self-tests
PASS  native runner storage: fmt, strict Clippy, 14 tests, release build, Darwin N-API smoke
PASS  independent aggregate and final-fix review: no P0-P3 findings
```

## Overall Verdict

🟢 **ACCEPT** — The Phase 621 local source is ready for its bounded feature-branch commit and
exact-tip CI. This verdict does not authorize a merge, deployment, cohort opening, flag change,
provider connection, external write, or production release.

## Follow-ups for Release

- Push the exact source and generated bundle, then require green exact-tip CI including the
  managed-runner Docker build/smoke unavailable on this host.
- Execute isolated hosted PostgreSQL migration/race/audit evidence and read back the dark cohort.
- Prove preproduction cap-2 concurrency, suspend/resume, backup/restore, monitoring, and rollback.
- Open production cap 25 only after the separately reviewed Phase 622 runtime/provider gates pass.
