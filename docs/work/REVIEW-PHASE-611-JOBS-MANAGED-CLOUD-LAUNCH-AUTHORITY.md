# REVIEW: PHASE-611 — Jobs Managed Cloud Launch Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its release guidance against the
> Phase 611 round contract, the current working tree, and the predecessor Phase 609/610
> authority before review.

**Commit range:** `89d6b820..working tree`
**Reviewer:** Codex (independent line-by-line review)
**Date:** 2026-08-14

## Per-Task Review

### 611.1 — Signed release, runtime readiness, and administrative authority

| Field | Value |
|-------|-------|
| Files | Managed-cloud release workflows/scripts, API/runtime authority, container definitions, configuration, migrations, and tests listed in the Phase 611 implementation inventory |
| Verdict | 🟢 accept |

**Findings:**

- Canonical signed release objects bind the closed component, schema, protocol, configuration,
  evidence, role, compatibility, activation, cohort, rollback, and revocation sets.
- Readiness is database-derived from the exact active release and fresh role quorum. Debug, loose
  token, mutable tag, stale runtime, and historical fleet shortcuts do not grant launch authority.
- Candidate construction, authorization, promotion, and rollback preserve the build-once/stored-byte
  boundary. All customer-facing gates remain default-off.

---

### 611.2 — Admission, durable request-start, dispatch, and recovery

| Field | Value |
|-------|-------|
| Files | Jobs admission API, workflow-command/cleanup authority, dispatcher, Temporal gateway/workflows, protocol contracts, and tests listed in the Phase 611 implementation inventory |
| Verdict | 🟢 accept |

**Findings:**

- Exact idempotent replay precedes mutable release, readiness, account, entitlement, and
  application checks, while new work freezes the admitted release A into one durable command.
- Request-start persists before gateway I/O. New-effect schema-v3 dispatch and lookup-only
  reconciliation are separate, and the historical schema-v2 recovery route remains byte-compatible.
- Result lookup, ambiguity persistence, terminalization, and cleanup retain the original authority
  after mutable release/runtime changes without creating a second external effect.

---

### 611.3 — Managed runner A+B effect boundary and database serialization

| Field | Value |
|-------|-------|
| Files | Runner execution/volume/checkpoint paths, execution leases, release authority, runner-volume purge, paired migrations, and focused regression tests listed in the Phase 611 implementation inventory |
| Verdict | 🟢 accept |

**Findings:**

- Claim, authorization, and irreversible marking correlate the exact workflow request, release A,
  release digest, runtime instance, epoch, and worker identity before employer-facing I/O.
- A lost or invalid authorization response poisons the pending effect boundary; a changed-A retry or
  final submit cannot bypass it. Exact successful authorization is required to advance.
- The append-only irreversible receipt makes response-loss replay independent of mutable runtime B.
  Account deletion and cleanup retain exact fences and conservative cascade rules.
- PostgreSQL managed effects lock fleet, account/entitlement/application, command/binding, lease,
  then the child volume/key; runner-instance claims lock fleet before the child volume. The
  dedicated lock-order regressions are green, with no residual P0 or P1 finding.

---

### 611.4 — Release automation, schema parity, and repository hygiene

| Field | Value |
|-------|-------|
| Files | CI/release workflows, deterministic release gate, Docker inventories, environment example, Round 611, FIX records, implementation record, and this review |
| Verdict | 🟢 accept |

**Findings:**

- Workflow/contract determinism, generated-output comparison, paired schema parity, privacy,
  provenance, and release-hygiene checks are green.
- The customer boundary is the browser-delivered portal plus managed execution; no installable
  Bluey Browser is required or introduced by this batch.
- The 79-path working-tree inventory matches the implementation record, and `docs/reviews/` remains
  untouched.

## Cross-Task Findings

- Independent review found no remaining P0 or P1 correctness, security, privacy, recovery,
  determinism, or release-authority issue.
- `directDiscovery`, `globalDiscovery`, and `sourceVerification` remain false because their exact
  pre-effect managed runtime boundaries are intentionally outside this batch.
- Local source evidence does not claim hosted registry publication, signing ceremony, Temporal
  behavior, customer admission, real runner capacity, image-digest attestation, read-only-rootfs
  enforcement, canary execution, or rollback execution.
- The final full Rust all-target test rerun passed with zero failures.

## Build & Test Verification

```text
Jobs typecheck
  PASS: automation, browser, runner, workflows, and portal workspaces

Jobs tests
  PASS: 1,749 tests
    automation 660; browser 219; runner 308; workflows 291; portal 271

Jobs production build
  PASS: all five workspaces and the production portal bundle
  NOTE: Vite emitted only its existing advisory for chunks larger than 500 kB

managed-cloud release gate
  PASS: 16 tests

workflow and contract determinism
  PASS

paired schema parity
  PASS

privacy, provenance, and release hygiene
  PASS

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

cargo test --manifest-path server/Cargo.toml --all-targets -- --test-threads=4
  PASS: 1,462 tests; zero failures
    lib 1,348; bluey-jobs-api 0; bluey-server 1; connectinfo 1;
    context migration 1; GDPR 2; integration_e2e 106;
    jobs_runner_plan_matrix 2; usage schema 1
```

## Overall Verdict

🟢 **ACCEPT** — The independent source review and complete local verification are green with no
P0/P1 findings. Hosted launch evidence remains external and is not claimed by this verdict.

## Follow-ups for Next Batch

- Build and preserve the exact candidate once, complete the authorized threshold-signing ceremony,
  publish immutable registry/static destinations, and verify full hosted read-back without rebuild.
- Prove hosted Temporal behavior, customer cohort authority, real managed runner capacity,
  runtime image-digest attestation, read-only-rootfs enforcement, canary behavior, and exact
  higher-sequence rollback before enabling any release flag.
- Close direct-discovery, global-discovery, and original-source verification behind their own exact
  pre-effect managed runtime boundaries before granting those capabilities signed authority.
