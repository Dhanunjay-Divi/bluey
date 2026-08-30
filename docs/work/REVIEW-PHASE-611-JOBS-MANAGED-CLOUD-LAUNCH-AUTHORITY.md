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
| Source verdict | 🟢 independently reviewed |
| Exact-tip verification verdict | 🟢 exact source-head CI green |

**Findings:**

- The pinned Playwright Chromium headless-shell payload alone contains 287 non-directory files, so
  the former 256-file ceiling could not construct the intended managed-runner image.
- Candidate construction, embedded runtime verification, and server release-evidence validation
  now share the same finite 512-file contract, with exact 512/513 boundary coverage.
- The 128 KiB canonical measurement-document limit, complete measurement roots, exact inventory,
  per-file hashes, and release flags are unchanged; the correction does not weaken attestation.
- The expanded gate fixture now emits valid 64-character digests for indexes above 255.

### 611.6 — Pinned managed-runner Node path correction

| Field | Value |
|-------|-------|
| Files | Runner Dockerfile/runtime verifier, release gate, Rust authority, CI/release smoke, tests, FIX-694, changelog, implementation, and review evidence |
| Source verdict | 🟢 independently reviewed; no residual P0/P1/P2 finding |
| Exact image verdict | 🟢 Docker build, runtime measurement, and native smoke green at exact source head |

**Findings:**

- Actions run `33313140429`, job `99261666660`, passed every gate before the managed image and
  failed exactly because the contract required `/usr/local/bin/node` while the pinned Playwright
  Noble image installs NodeSource Node at `/usr/bin/node`.
- The former 256-file ceiling failed first while traversing Chromium and masked this later missing
  measurement root; FIX-693 correctly exposed rather than caused the independent defect.
- The final correction preserves the established `/usr/local/bin/node` authority: the Docker build
  copies the pinned base's regular `/usr/bin/node` there before measurement, rejects a symlink,
  measures and runs the normalized file, then removes the source path.
- Candidate measurement, embedded startup remeasurement, stored-image inspection, Rust authority,
  image command, and CI/release native smoke remain on one path. Package-manager cleanup targets
  both the Playwright base's `/usr/bin`/`/usr/lib` layout and `/usr/local` leftovers.
- Pull-request jobs check out and embed the same `github.event.pull_request.head.sha`, with
  `github.sha` retained as the non-PR fallback, so runtime identity is no longer relabeled from a
  synthetic merge tree.
- Filesystem generation, startup remeasurement, stored OCI inspection, and Rust authority reject
  symbolic links throughout every measured runtime root rather than silently skipping or following
  them.
- Full automation and runner tests/typechecks, release-gate tests, Rust formatting, Linux
  image/native smoke, and complete exact source-head CI gates are green.

### 611.7 — Rust 1.98 compatibility backport

| Field | Value |
|-------|-------|
| Files | Eleven core, daemon, RAG, and server files plus FIX-695 |
| Source verdict | 🟢 exact backport independently verified |
| CI precedent | 🟢 unchanged `2f3910a1` patch green at descendant PR #33 head `83f15263` on macOS, Ubuntu, Windows, and observability |
| Phase 611 exact-head verdict | 🟢 macOS, Ubuntu, Windows, and observability/server checks green |

**Findings:**

- The four non-Jobs failures share the rolling Rust 1.98 Clippy surface and are independent of the
  Phase 611 managed-runner logic.
- The proven eleven-file patch was backported without the unrelated Phase 623 UI changes;
  fixing only the first `ipc_auth.rs` diagnostic would have left later failures.
- Phase 611 now has its own corrected exact source-head run; the earlier descendant evidence remains
  compatibility precedent, while runs 33323257936 and 33323257933 are the combined-branch proof.

### 611.8 — Remaining Rust 1.98 daemon application correction

| Field | Value |
|-------|-------|
| Files | `crates/cue-daemon/src/app.rs` plus FIX-696 and Phase 611 records |
| Source verdict | 🟢 independently reviewed; no P0/P1/P2/P3 finding |
| Exact-head verdict | 🟢 exact source-head CI green |

**Findings:**

- Exact-head runs `33322554768` and `33322554753` exposed two additional constant-width
  `chunks_exact(2)` diagnostics in daemon application code on macOS, Ubuntu, Windows, and the
  observability job after the initial eleven-file backport.
- FIX-696 applies only the matching `as_chunks::<2>()` transformations already present at green
  descendant PR #33 head `83f15263`; no unrelated daemon or Phase 623 work is imported.
- Rust 1.98 workspace formatting, diff checks, and a repository-wide numeric `.chunks_exact(...)`
  scan pass. Independent review found no P0/P1/P2/P3 issue; the final combined exact source-head CI
  run passed.

## Cross-Task Findings

- Final independent review found no residual P0/P1/P2 issue across FIX-693/FIX-694/FIX-695 and no
  P0/P1/P2/P3 issue in FIX-696.
- `directDiscovery`, `globalDiscovery`, and `sourceVerification` remain false because their exact
  pre-effect managed runtime boundaries are intentionally outside this batch.
- Local source evidence does not claim hosted registry publication, signing ceremony, Temporal
  behavior, customer admission, real runner capacity, image-digest attestation, read-only-rootfs
  enforcement, canary execution, or rollback execution.
- The baseline `423fba5c` full Rust all-target test rerun passed with zero failures, and the final
  corrected exact source head also passed the resource-capable combined workflows.

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

Post-FIX-693 local evidence at the branch tip on 2026-08-24:

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

The first resource-capable PR rerun for head `f50103e8` checked out GitHub merge result `f552ac2a`.
It passed managed release authority, 1,748 Jobs tests with one intentional skip, every Jobs
typecheck/build, portal parity, and native runner storage format/Clippy/tests/release build. It then
failed at image construction before the native smoke with
`Runtime measurement root is missing: /usr/local/bin/node`.

Post-FIX-694/FIX-695 local evidence before commit on 2026-08-30:

```text
automation full test suite and typecheck
  PASS: 660; 1 intentional skip; TypeScript clean

runner full test suite and typecheck
  PASS: 308; TypeScript clean

managed-cloud release gate
  PASS: 17

server Rust formatting
  PASS

git diff --check
  PASS
```

Exact-tip CI evidence for source head `032568c3698af10150752bb23cebf71268b614b6`:

- [Jobs run 33323257948](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33323257948):
  [Jobs CI/privacy job 99288862771](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33323257948/job/99288862771)
  passed 1,749 Jobs tests with one intentional skip (660/219/308/291/271 by workspace), 17
  managed-cloud release-gate tests, all typechecks/builds and privacy/contract gates, native
  storage format/Clippy/tests/release build, the Linux managed-runner Docker build with exact-head
  runtime measurement, and native-addon smoke. Its server subset passed 729 unit and 34 integration
  tests. [Darwin native storage 99288862640](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33323257948/job/99288862640)
  passed independently.
- [CI run 33323257936](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33323257936):
  Ubuntu, Windows, and macOS checks all passed.
- [Observability run 33323257933](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33323257933):
  server, observability-policy, workspace, and aggregate checks passed. The
  [server job 99288828813](https://github.com/Dhanunjay-Divi/bluey/actions/runs/33323257933/job/99288828813)
  reported 1,464 passed tests across its suites with zero failures.

## Overall Verdict

🟢 **SOURCE AND EXACT-TIP CI GREEN; HOSTED LAUNCH AUTHORITY REMAINS NO-GO** — Final independent
review found no residual issue through FIX-696, and the exact source head passed Jobs/privacy,
cross-platform CI, observability/server, Linux managed-runner Docker measurement, and native smoke.
This verdict is limited to source and CI evidence. Registry publication/read-back, protected
threshold approval, hosted PostgreSQL/Temporal/network behavior, deployed runtime image and
read-only-rootfs attestation, real runner capacity, ATS/customer canaries, cohort admission, flag
enablement, kill-switch exercise, and rollback rehearsal remain unproven and must not be inferred.

## Follow-ups for Next Batch

- Build and preserve the exact candidate once, complete the authorized threshold-signing ceremony,
  publish immutable registry/static destinations, and verify full hosted read-back without rebuild.
- Prove hosted Temporal behavior, customer cohort authority, real managed runner capacity,
  runtime image-digest attestation, read-only-rootfs enforcement, canary behavior, and exact
  higher-sequence rollback before enabling any release flag.
- Close direct-discovery, global-discovery, and original-source verification behind their own exact
  pre-effect managed runtime boundaries before granting those capabilities signed authority.
