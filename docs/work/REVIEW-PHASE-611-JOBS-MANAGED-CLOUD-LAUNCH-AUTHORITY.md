# REVIEW: PHASE-611 — Jobs Managed Cloud Launch Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its release guidance against the
> Phase 611 round contract, the current working tree, and the predecessor Phase 609/610
> authority before review.

**Commit range:** `89d6b820..branch tip`
**Reviewer:** Codex (independent line-by-line review)
**Date:** 2026-08-24

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
- The 80-path branch-diff inventory matches the implementation record, and `docs/reviews/` remains
  untouched.

---

### 611.5 — Managed-runner measurement capacity correction

| Field | Value |
|-------|-------|
| Files | Release gate/runtime measurement source and tests, Rust release authority, FIX-693, changelog, implementation, and review evidence |
| Source verdict | 🟢 accept |
| Exact-tip verification verdict | 🟡 resource-capable gates required |

**Findings:**

- The pinned Playwright Chromium headless-shell payload alone contains 287 non-directory files, so
  the former 256-file ceiling could not construct the intended managed-runner image.
- Candidate construction, embedded runtime verification, and server release-evidence validation
  now share the same finite 512-file contract, with exact 512/513 boundary coverage.
- The 128 KiB canonical measurement-document limit, complete measurement roots, exact inventory,
  per-file hashes, and release flags are unchanged; the correction does not weaken attestation.
- The expanded gate fixture now emits valid 64-character digests for indexes above 255.

## Cross-Task Findings

- Independent review found no remaining P0 or P1 correctness, security, privacy, recovery,
  determinism, or release-authority issue.
- `directDiscovery`, `globalDiscovery`, and `sourceVerification` remain false because their exact
  pre-effect managed runtime boundaries are intentionally outside this batch.
- Local source evidence does not claim hosted registry publication, signing ceremony, Temporal
  behavior, customer admission, real runner capacity, image-digest attestation, read-only-rootfs
  enforcement, canary execution, or rollback execution.
- The baseline `423fba5c` full Rust all-target test rerun passed with zero failures; the corrected
  tip still requires a resource-capable rerun.

## Build & Test Verification

Baseline evidence at `423fba5c`, before the measurement-bound correction:

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

Post-correction local evidence at the branch tip on 2026-08-24:

```text
managed-cloud release gate
  PASS: 16 tests, including 512 accepted / 513 rejected

Node strip-types syntax checks
  PASS: managed-cloud-runtime.ts and its focused test

cargo fmt --all -- --check
  PASS

git -P diff --check
  PASS
```

Not rerun at the corrected tip: automation Vitest/typecheck, Cargo check/Clippy/tests, and the exact
managed-runner Docker build/native smoke. The local disk had 5.0 GiB free, below the 8 GiB
release-work floor, no PortableSSD was mounted, and Docker was unavailable. Exact-tip CI evidence
must replace this conditional status; the baseline totals above cannot be inherited by the fix.

## Overall Verdict

🟡 **SOURCE FIX REVIEWED; RESOURCE-CAPABLE VERIFICATION REQUIRED** — Independent source review
found no P0/P1 issue in the correction, and the lightweight local gates are green. The corrected
tip is not accepted until automation, Rust, and exact managed-runner Docker/CI gates pass. Hosted
launch evidence remains external and is not claimed by this verdict.

## Follow-ups for Next Batch

- Build and preserve the exact candidate once, complete the authorized threshold-signing ceremony,
  publish immutable registry/static destinations, and verify full hosted read-back without rebuild.
- Prove hosted Temporal behavior, customer cohort authority, real managed runner capacity,
  runtime image-digest attestation, read-only-rootfs enforcement, canary behavior, and exact
  higher-sequence rollback before enabling any release flag.
- Close direct-discovery, global-discovery, and original-source verification behind their own exact
  pre-effect managed runtime boundaries before granting those capabilities signed authority.
