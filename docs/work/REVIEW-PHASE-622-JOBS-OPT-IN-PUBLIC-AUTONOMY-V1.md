# REVIEW: PHASE-622 — Jobs Opt-In Public Autonomy V1

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the final aggregate source against Round
> 622, the Phase 611 managed-cloud authority, the Phase 621 public gate, and the current dark
> production posture.

**Commit range:** `3f90bc01210345df56f24a8e95a493895c9744ae..95bb966fd897db94599a0c7e2defe1fb02ea3912`

**Source commit:** `55d24e95234c1fcc2db22a912c739dfd5760eb66`

**Generated portal commit:** `95bb966fd897db94599a0c7e2defe1fb02ea3912`

**Reviewer:** Independent aggregate security, local-run recovery, fixture, portal, and final
source reviewers; consolidated by the primary implementation agent

**Date:** 2026-08-31

**Review status:** 🟢 ACCEPT for the bounded local source commit; 🔴 NO-GO for production autonomy
until hosted runtime/provider/canary gates pass

## Per-Task Review

### Public admission composed with external-effect authority

| Field | Value |
|-------|-------|
| Files | Jobs API/database effect paths, public-beta authority, operations and metrics |
| Verdict | 🟢 accept |

**Verified:**

- Every fresh managed/local claim, irreversible submit, communication claim, and provider request
  start rechecks current beta and deletion authority inside its database transaction.
- Admission never substitutes for Career Track, entitlement, signed release/runtime, ATS,
  evidence, budget, circuit, kill-switch, or provider consent authority.
- Denial, suspension, deletion intent, or master-off blocks new effects while preserving evidence
  persistence and lookup-only reconciliation.

### Exact local-run replay and ambiguity safety

| Field | Value |
|-------|-------|
| Files | Browser release authority, local runner, execution leases, API routes, regression tests |
| Verdict | 🟢 accept |

**Verified:**

- Claim and submit reach replay-first database authority; only fresh work is denied by changed
  distribution, fleet, cohort, or master state.
- Review-first schema-3 and ATS-certified schema-4 click-started recovery bind the exact stored
  ticket, proof, release, application, session, and evidence capacity.
- Exact retry returns the original database-owned authorization timestamp and cannot create a
  second employer-facing effect.
- FIX-778 groups distribution input into one typed request without changing replay or authorization
  semantics and passes strict Clippy without a waiver.

### Portal truth and release boundary

| Field | Value |
|-------|-------|
| Files | Portal API/types/views/tests, generated `web/jobs`, docs and configuration |
| Verdict | 🟢 accept |

**Verified:**

- Product copy describes a capped public release, not invitations or automatic access.
- The portal distinguishes admission, Career Track authorization, actual runner availability, and
  effect capability; it does not infer runtime readiness from plan entitlements.
- The generated bundle is deterministic and current. Every production flag remains default-off.

## Cross-Task Findings

- Independent reviews found no remaining P0-P3 correctness, security, privacy, replay, deletion,
  audit, telemetry, portal, or documentation issue in the bounded source.
- The source candidate is not sufficient production evidence. Hosted PostgreSQL, Temporal,
  managed-runner capacity/runtime, encrypted volume, provider, monitoring, restore, and rollback
  proof remain mandatory.
- C2C execution, autonomous recruiter replies, MCP delegation, direct/global discovery runtime,
  WhatsApp/iMessage, and broader provider automation remain successor phases, not launch claims.

## Build & Test Verification

```text
PASS  Jobs workspaces: 1,982 tests; one intentional no-egress simulator skip
PASS  all five typechecks and production builds; portal digest
      65be1e2ebda67b02fcd1c9340240bb0100aade36a2748c2266369898b3d65235
PASS  final exact-state server library: 1,611/1,611 in 1,872.19 seconds
PASS  final exact-state HTTP integration: 113/113 in 558.24 seconds
PASS  server all-target check and strict all-target Clippy (`-D warnings`)
PASS  repository formatting and diff checks
PASS  schema 105/90; privacy 2,719/2,443; provenance 663/631/14
PASS  Browser release 10/10; managed-cloud release 17/17; deletion 3/3
PASS  messaging containment and CI guard self-tests
PASS  native runner storage: fmt, strict Clippy, 14 tests, release build, Darwin N-API smoke
PASS  independent aggregate, FIX-776/777, and FIX-778 reviews: no P0-P3 findings
```

## Overall Verdict

🟢 **LOCAL SOURCE ACCEPT** — Ready for bounded feature-branch commits and exact-tip CI.

🔴 **PRODUCTION AUTONOMY NO-GO** — Do not merge, deploy, open the cohort, enable a model, distribute
a runner, connect a provider, send a message, or submit an application until the remaining hosted
proof-to-enable sequence is complete and separately authorized.

## Follow-ups for Release

- Integrate and verify the exact Phase 611 managed-cloud compatibility head without rebuilding
  promoted artifacts.
- Prove hosted PostgreSQL/Temporal locking and recovery, real runner capacity/identity/rootfs,
  encrypted-volume restore/purge, ATS and mailbox/reply/calendar canaries, and ambiguity handling.
- Prove monitoring, disk/backup/restore dead-men, cap-2 concurrency, kill switches, and rollback.
- Threshold-sign and open production cap 25 only after those exact artifacts and gates are green.
