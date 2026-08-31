# FIX-766: Jobs Production-positive Integration Fixture Authority

> **Codex preflight:** Loaded `$bluey-ops` and reconciled its operating memory against Round 614B,
> FIX-725 through FIX-765, and the current authoritative worktree. The SSD archive was not used.

**Severity:** P1 release-test reliability and test-authority containment defect

**Status:** Implemented and locally verified on the frozen source. The explicit integration target
passed 108/108, the feature-off library aggregate passed 1,586/1,586, default and feature checks
and strict Clippy are warning-free, and release-artifact containment is proven locally. This closes
the fixture defect; the overall release remains yellow for the external gates listed below.

## Issue

`server/tests/integration_e2e.rs::setup_execution_lease_run` was written before the signed
original-source, ATS-certification, and job-integrity authorities became mandatory. Its former
positive setup was no longer an authoritative positive route:

1. It assigned `JobDiscoveryEvidence::provider_verified_original_source(...)` directly to mutable
   posting input and called `upsert_posting`. Mutable `discovery_evidence` is a customer projection;
   it cannot mint a current signed original-source receipt, exact ATS head, or dual-role integrity
   authority. Current preparation, approval, reservation, claim, and FinalSubmit boundaries
   correctly reject that shortcut.
2. It used `JobPreferences::default()` as an in-memory argument without first persisting a
   `jobs_preferences` row. Read-only preference APIs can safely project defaults for an absent row,
   but authoritative reservation and execution paths require the persisted row and its account
   input generation.
3. Persisting preferences after `upsert_track` is also invalid. `save_preferences` advances the
   account policy-input generation. A Track reviewed before that write is immediately stale and
   must return to `needs_review`. Preferences therefore have to be persisted before the Track is
   reviewed and approved.

The integration test was supposed to exercise execution-lease and receipt boundaries, not prove
that mutable evidence or absent canonical inputs could bypass current authority.

## Root Cause

The fixture encoded an obsolete convenience path instead of the production-positive authority
lifecycle. It conflated a visible posting projection with effect authority and relied on an
in-memory default where later database gates require a durable semantic input.

The reusable production-positive fixture already existed inside crate test scope, but Rust
integration tests compile `bluey-server` as an ordinary dependency, where `cfg(test)` is not set for
the library. The external `integration_e2e` target therefore could not call the crate-private test
fixture without an explicitly gated support surface.

## Fix Summary

### Canonical account-input and runner order

- Route the existing cloud helper through a runner-specific setup that accepts only `local` or
  `cloud`.
- Set a local fixture account to the `pro` entitlement and a cloud fixture account to the `cloud`
  entitlement before any execution authority is created.
- Persist the normalized default preferences through `save_preferences`.
- Persist preferences before `upsert_track`, so the Track approval binds the current account input
  generation and exact `job_preferences_sha256`.
- Assert that the returned Track authority is `approved` and has a positive policy revision.
- Pass the persisted preferences object into the production-positive posting fixture.
- After application approval, reserve the application attempt for the exact `runner_kind`, reload
  the application, and only then create and assign the same-runner browser session.

The required order is:

```text
account + runner-specific entitlement (`pro` local / `cloud` cloud)
  -> resume source asset + profile
  -> verified application identity
  -> persisted preferences
  -> reviewed/approved Career Track
  -> signed source/ATS/integrity authority
  -> prepare + approve
  -> runner-bound attempt reservation
  -> same-runner browser session + assigned execution run
```

### Real signed positive lifecycle

The integration helper no longer fabricates `discovery_evidence`. It uses the shared
production-positive fixture to:

1. save the supported provider posting and discovery membership through the verified-import path;
2. complete a real discovery snapshot;
3. install and claim a fresh managed original-source-verifier runtime fixture;
4. lease and complete public original-source verification for the exact posting;
5. resolve the resulting signed source integrity binding;
6. install signed ATS authority for the exact provider target, observed surface, and selected
   local macOS/arm64 or cloud Linux/x86_64 runtime;
7. install disjoint signed employer-identity and job-risk authority for the exact composed source;
8. resolve `JobIntegrityResolutionStatus::Verified`; and
9. return the authoritative projected posting used by the ordinary prepare, approve,
   runner-reservation, and run-assignment path.

The helper asserts that source verification is active, the source integrity binding exists, the
ATS head is active and bound, and composed job integrity is verified. It does not weaken any
production verifier, accept a mutable label, or enable a production flag.

The earlier process-wide verifier-runtime cache has been removed. Every fixture setup invokes the
runtime installer directly, so runtime credentials and grant state are not reused merely because a
later setup resolves to the same database identity.

### Integration-only feature surface

- Add a default-off `integration-test-support` Cargo feature.
- Compile `production_positive_authority_fixture` under either crate tests or that feature.
- Expose one `#[doc(hidden)]` integration helper only when the feature is enabled.
- Keep the package's self dev-dependency feature-free.
- Declare the existing `integration_e2e` file as an explicit `[[test]]` target with
  `required-features = ["integration-test-support"]`.
- Select the feature explicitly for the focused Jobs CI command and for the full release-workflow
  integration Clippy and test commands; preserve the ordinary feature-off `--all-targets` gates.
- Add a static CI self-guard for the Cargo target contract, workflow commands, release guard, and
  absence of the support feature from the Jobs production Docker/release definitions.
- Reject the feature when `debug_assertions` is disabled.

## Files Modified

| File | Change |
| --- | --- |
| `server/Cargo.toml` | Add the default-off support feature and optional `serial_test`; require that feature on the existing `integration_e2e` target while keeping the self dev-dependency feature-free. |
| `server/src/lib.rs` | Reject `integration-test-support` when `debug_assertions` is disabled. |
| `server/src/db/jobs.rs` | Gate the shared fixture and expose one hidden typed-request integration installer. |
| `server/src/db/jobs/production_positive_authority_fixture.rs` | Reuse the public source → ATS → integrity lifecycle with runner-specific runtime targets and a fresh verifier runtime per setup. |
| `server/src/db/jobs/discovery.rs` | Make the exact-source discovery lease helper available to crate tests or support-feature builds. |
| `server/src/db/jobs/original_source_verification.rs` | Make the bounded signed original-source fixture available to support-feature builds. |
| `server/src/db/jobs/ats_certification_authority.rs` | Make the bounded signed ATS fixture available to support-feature builds. |
| `server/src/db/jobs/job_integrity_authority.rs` | Make the bounded dual-signed integrity fixture available to support-feature builds. |
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Make the bounded verifier-runtime fixture available to support-feature builds. |
| `server/src/api/jobs_worker_auth.rs` | Compile the legacy-debug path classifier only for debug or test builds so the default release is warning-free. |
| `server/tests/integration_e2e.rs` | Persist runner-specific entitlement/preferences in canonical order, assert Track authority, replace mutable evidence with the signed fixture, and bind the reservation/session to the same runner. |
| `.github/workflows/jobs-ci.yml` | Select the support feature explicitly for the filtered Jobs integration target. |
| `.github/workflows/release.yml` | Preserve feature-off aggregate gates and add explicit support-feature integration Clippy and full-target test commands. |
| `jobs/scripts/ci-guards-self-test.mjs` | Enforce the target, dependency, workflow, release-guard, and production-build containment contract. |
| `docs/work/FIX-766-jobs-production-positive-integration-fixture-authority.md` | Record the defect, correction, containment risks, and evidence contract. |

`server/Cargo.lock` has no current diff because `serial_test` already existed as a dev dependency;
that fact does not waive dependency-graph or release-artifact checks.

## Release-containment Review

The default feature list is empty, and read-only dependency inspection found that the default
normal dependency graph excludes `serial_test`. Enabling `integration-test-support` adds
`serial_test 3.5.0` and `serial_test_derive 3.5.0`.

The explicit target now requires `integration-test-support`; the self dev-dependency no longer
enables that feature package-wide. Jobs CI and the release workflow select it only for the existing
integration target, while the static guard rejects drift in those commands or feature activation in
the Jobs production Docker/release definitions.

The verifier-runtime cache identified in the first containment review no longer exists. The
fixture installs a new verifier runtime on every setup rather than retaining runtime credentials in
a process-wide database-keyed map.

The following design costs are accepted for this test-only surface because the frozen artifact
evidence and CI guard close the production boundary:

1. The feature currently opens the complete original-source, ATS, integrity, and managed-cloud
   test modules—roughly 17,000 lines—instead of only the fixture constructors. Several contain
   deterministic signing keys and mutation helpers.
2. `#[doc(hidden)]` removes documentation visibility but does not make the integration installer a
   security boundary. A debug or custom artifact built with the feature can contain it.
3. The compile-time prohibition is keyed to `debug_assertions`, not a signed release-build
   allowlist. The expected-negative release build and successful default artifact scan prove the
   current boundary; the static guard prevents the feature from entering production definitions.
4. Moving `serial_test` into optional normal dependencies broadens the feature graph and duplicates
   its manifest declaration. Prefer extracting the minimal fixture surface so it can remain a
   dev-only dependency.
5. Because `required-features` causes ordinary feature-off aggregate commands to skip this one
   target, the explicit Jobs CI and release-workflow commands plus their static guard are mandatory.

Preferred follow-up hardening remains a minimal integration-support module or companion
test-support crate that exposes only the required constructors and keeps test macros entirely in
dev dependencies. It is not required to close this FIX because the exact current feature graph,
release compile guard, production-definition guard, successful default release builds, and binary
string scans are all observed green. Hosted CI must reproduce those checks before release.

## Acceptance Gates

### Fixture correctness

- A `jobs_preferences` row exists before Track review.
- The reviewed Track binds the persisted preference digest and current account-input generation.
- The setup asserts `review_state = approved` and a positive Track policy revision before creating
  the posting authority.
- Saving the same preferences before the Track does not create immediate policy drift.
- Missing preferences still fail closed at authoritative reservation/execution boundaries; the
  production boundary is not changed to accommodate the fixture.
- Mutable `discovery_evidence` alone cannot satisfy signed source, ATS, or integrity authority.
- The exact signed source → ATS → dual-role integrity lifecycle resolves verified for both selected
  local and cloud runtime shapes, and subsequent execution-lease consumers use that returned
  posting.
- The approved application has an attempt reservation for the selected runner before the
  same-runner browser session is assigned.

### Feature and dependency containment

- `default = []` and the default locked normal dependency graph contains no `serial_test`.
- The existing `integration_e2e` target requires `integration-test-support`, and the self
  dev-dependency does not enable it.
- Feature-enabled test builds contain only the explicitly accepted support dependencies.
- Default debug/check/test commands remain feature-off; the focused Jobs CI and release gates
  explicitly select the feature for `--test integration_e2e`.
- A default locked release build succeeds for both server binaries with the support feature absent.
- A release build that explicitly enables `integration-test-support` fails with the intended guard.
- Final release artifacts contain no integration installer, fixture assertion text, deterministic
  test-authority material, or `phase614b-production-positive` fixture identifiers.
- Hosted/Docker build definitions and release scripts do not enable the support feature.
- The CI guard rejects target-contract, workflow-command, release-guard, or production-build drift.

### Regression and aggregate evidence

- Every `integration_e2e` test that consumes `setup_execution_lease_run` passes. This includes
  worker fencing, managed receipt, checkpoint reconciliation, idempotency, tamper rejection,
  intervention, managed-authority denial, and reconciliation paths.
- Full library, integration, doc, and all-target Rust tests pass against the same frozen source.
- Default and support-feature checks pass.
- Strict Clippy passes with warnings denied, without a blanket suppression hiding production code.
- The final source manifest and binaries are hashed after the last source change.

## Evidence

- **OBSERVED — read-only metadata:** `integration-test-support` is default-off.
- **OBSERVED — read-only dependency tree:** the default locked normal graph excludes
  `serial_test`; the feature-enabled normal graph includes `serial_test 3.5.0` and
  `serial_test_derive 3.5.0`.
- **OBSERVED — current source containment:** `integration_e2e` is an explicit 108-test target that
  requires `integration-test-support`; the self dev-dependency is feature-free; Jobs CI and release
  select the feature only for that target; the static guard checks those facts and forbids the
  feature in the Jobs production Docker and managed-cloud release definitions.
- **OBSERVED — focused integration (root-supplied):**
  `jobs_intervention_answer_revises_packet_without_resuming_runner` passed **1/1**; the
  `jobs_local_` filter passed **3/3**; and
  `jobs_legacy_v1_local_result_resume_and_submitted_replay_remain_recovery_only` passed **1/1**.
  The full target below supersedes those focused results.
- **OBSERVED — frozen source:** Git `HEAD`
  `be925f89ccc5af2fb4b2ea41ba123c571fefadd6`; 44 non-document changed files, comprising 36
  modified and eight new files; canonical SHA-256 of sorted per-file SHA-256 lines
  `fd49f94a6c8e22c373a848f74956abb0af57b62c090d8b4f40c15856b46359aa`; recorded at
  `2026-08-30T09:38:57Z` after the last source edit.
- **PASS — full explicit integration:**
  `CARGO_INCREMENTAL=0 cargo test --locked --manifest-path server/Cargo.toml
  --no-default-features --features integration-test-support --test integration_e2e --
  --test-threads=1` passed **108/108**, zero failed/ignored/measured/filtered, in `543.78s` test
  time. The run began `2026-08-30T08:41:16Z` and finished by `2026-08-30T08:51:53Z`.
  Binary `server/target/debug/deps/integration_e2e-835dc4e8758d9a13` has SHA-256
  `4cc05b9abe006f8dc23bdf79ba2bc56323f579fa09e755f409ce842838e96b10`.
- **PASS — feature-off library aggregate:**
  `CARGO_INCREMENTAL=0 cargo test --locked --manifest-path server/Cargo.toml --lib` passed
  **1,586/1,586**, zero failed/ignored/measured/filtered, in `2178.55s` test time. It began
  `2026-08-30T08:52:02Z` and finished by `2026-08-30T09:31:19Z`.
  Binary `server/target/debug/deps/bluey_server-0ad0add04d272872` has SHA-256
  `1eb9fb796a5789c53dfbacd7d47fc90f4347ddbbc15c50b99d61d6bd6e947f0b`.
- **PASS — remaining Rust targets:** ConnectInfo **1/1**, context migration **1/1**, GDPR cleanup
  **2/2**, runner-plan matrix **2/2**, configured PostgreSQL schema **1/1**, startup-bin **1/1**,
  and doc tests **0/0**, all with zero failures. The integration target is intentionally absent
  from feature-off aggregate commands and is proven separately above.
- **PASS — compile hygiene:** locked default `cargo check --all-targets` and the locked explicit
  support-feature integration check passed. Locked default all-target strict Clippy and the
  explicit support-feature integration Clippy both passed with `-D warnings` and zero warnings.
- **PASS — default release containment:** the locked default release build for `bluey-server` and
  `bluey-jobs-api` completed without warnings. SHA-256 values are respectively
  `4cc6937745138957f91946981fbf5206f7f03f2bb672e783bc3ebb2952f3550f` and
  `4c662fcfcaa4e9b54adb12a27e09e1a2b937b4a3ec6bdb15779231c8e26ec30a`.
- **PASS — negative release containment:** the locked release check with
  `--features integration-test-support --lib` exited `101` only at
  `compile_error!("integration-test-support must never be enabled in release builds")`.
- **PASS — artifact and graph containment:** the default normal dependency graph has no
  `serial_test`; the support-feature graph contains only `serial_test 3.5.0` and
  `serial_test_derive 3.5.0` for that addition. `strings` scans of both default release binaries
  found none of the feature, installer, fixture-module, fixture suffix, or long-horizon fixture
  identifiers. `node jobs/scripts/ci-guards-self-test.mjs` passed the same target/workflow/
  production-definition contract.

## How To Test

Run only after the source freezes:

```bash
# Static containment contract.
node jobs/scripts/ci-guards-self-test.mjs

# Representative exact integration boundary.
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --test integration_e2e \
  jobs_execution_lease_routes_require_worker_auth_and_fence_submit -- --nocapture

# Focused cloud/default and local/legacy consumers.
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --test integration_e2e \
  jobs_intervention_answer_revises_packet_without_resuming_runner
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --test integration_e2e jobs_local_
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --test integration_e2e \
  jobs_legacy_v1_local_result_resume_and_submitted_replay_remain_recovery_only

# Full explicit integration target, then the feature-off aggregate.
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --no-default-features \
  --features integration-test-support --test integration_e2e
CARGO_INCREMENTAL=0 cargo test --manifest-path server/Cargo.toml --all-targets

# Default and feature-enabled compile hygiene.
CARGO_INCREMENTAL=0 cargo check --locked --manifest-path server/Cargo.toml --all-targets
CARGO_INCREMENTAL=0 cargo check --locked --manifest-path server/Cargo.toml \
  --no-default-features --features integration-test-support --test integration_e2e
CARGO_INCREMENTAL=0 cargo clippy --locked --manifest-path server/Cargo.toml --all-targets \
  -- -D warnings
CARGO_INCREMENTAL=0 cargo clippy --locked --manifest-path server/Cargo.toml \
  --no-default-features --features integration-test-support --test integration_e2e \
  -- -D warnings

# Production-positive default release artifacts.
CARGO_INCREMENTAL=0 cargo build --locked --release --manifest-path server/Cargo.toml \
  --bin bluey-server --bin bluey-jobs-api

# Negative containment test: this command must fail with the explicit compile_error.
CARGO_INCREMENTAL=0 cargo check --locked --release --manifest-path server/Cargo.toml \
  --no-default-features --features integration-test-support --lib

# Dependency graph comparison.
cargo tree --locked --manifest-path server/Cargo.toml --edges normal --no-default-features
cargo tree --locked --manifest-path server/Cargo.toml --edges normal --no-default-features \
  --features integration-test-support
```

Use a separate, explicit artifact-inspection step to prove that the successful default release
binaries contain no integration-support fixture symbols or identifying strings. Do not reinterpret
the intentionally failing negative containment command as a release-build failure.

## Known Limitations

- Hosted CI must reproduce the explicit feature target, default release build, expected-negative
  release guard, dependency graph, and artifact scan. Local evidence cannot prove hosted workflow
  permissions or runner-image behavior.
- The broad test-module compilation, optional-normal `serial_test`, and manually selected debug
  feature surface remain follow-up hardening opportunities. Package-wide dev activation and the
  verifier-runtime cache are no longer current risks, and the current production artifacts are
  proven feature-free.
- This test-only correction does not provide hosted PostgreSQL, Docker/Linux, registry,
  production-key, provider canary, deployment, customer-cohort, or production flag evidence.
- All production/provider-write flags remain `0`; source verification, direct discovery, and global
  discovery remain disabled in current activations.

## Decision

The integration fixture follows the same signed positive authority chain required by production
code, the persisted preference/Track ordering matches the canonical policy ledger, and the support
surface is absent from normal release artifacts. FIX-766 is locally closed. This does not grant
merge, deployment, activation, provider-write, or production authority; Phase 614B remains yellow
for hosted and external release gates.
