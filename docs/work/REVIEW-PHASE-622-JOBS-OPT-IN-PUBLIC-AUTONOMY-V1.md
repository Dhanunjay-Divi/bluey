# REVIEW: PHASE-622 — Jobs Opt-In Public Autonomy V1

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the final aggregate source against Round
> 622, the Phase 611 managed-cloud authority, the Phase 621 public gate, and the current dark
> production posture.

**Commit range:**
`3f90bc01210345df56f24a8e95a493895c9744ae..95696dd0ce204c06ed382ed73f52d46920501bfb`

**Aggregate source plus FIX-784:** `95696dd0ce204c06ed382ed73f52d46920501bfb`

**Generated portal and dependency-security commit:** `b3d0c79cecc44ca45bf31e6fbe2838108bfabe3a`

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
  semantics and passes strict Clippy without a waiver. FIX-779/780 preserve trusted-path
  substitution resistance while making opened-descriptor identity portable across Linux/macOS;
  FIX-781 closes all known dependency advisories and makes the audit a release gate; FIX-782 binds
  the combined hosted lane to a reviewed finite 90-minute budget; FIX-784 establishes real public
  beta admission in the stale plan-matrix fixture without weakening production middleware.

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
PASS  Jobs workspaces: 1,983 tests; one intentional no-egress simulator skip
PASS  all five typechecks and production builds; portal digest
      bf6ab3febf060cddc303be099cd01cc4c3d79d1c0a3a05d625b19e1d3f5d2042 (34 files)
PASS  npm production/full audits: 252/673 dependencies; zero advisories
PASS  final exact-state server library: 1,611/1,611 in 1,872.19 seconds
PASS  final exact-state HTTP integration: 113/113 in 558.24 seconds
PASS  server all-target check and strict all-target Clippy (`-D warnings`)
PASS  repository formatting and diff checks
PASS  schema 105/90; privacy 2,751/2,475; provenance 663/631/14
PASS  Browser release 10/10; managed-cloud release 18/18; deletion 3/3
PASS  messaging containment and CI guard self-tests
PASS  native runner storage: fmt, strict Clippy, 14 tests, release build, Darwin N-API smoke
PASS  storage guard, cloud preflight, and restore drill; real PostgreSQL scenario executed
PASS  FIX-784 focused test 1/1 and full test target 2/2; fmt and strict Clippy green
PASS  independent aggregate and FIX-776 through FIX-784 reviews: no P0-P3 findings
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
