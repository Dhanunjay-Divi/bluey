# FIX-693: Managed Runner Measurement Bound Rejected the Pinned Browser Runtime

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the Phase 611 release and
> container contracts against the current worktree. No registry, hosted runner,
> credential, deployment, canary, customer cohort, or release flag was used.

## Issue

Pull request #32 Actions run `31819200673`, job `94828128144`, passed every earlier
Jobs/privacy/native step and then failed while building the managed runner image with
`Runtime measurement contains too many files`.

## Root Cause

The managed runner deliberately retains and measures the full pinned Playwright 1.61.1
`chromium_headless_shell-*` and `ffmpeg-*` runtime trees. The retained Chromium headless-shell
payload alone has 287 non-directory files, exceeding the repeated 256-file ceiling before the
automation bundle, runner bundle, native addon, package metadata, and Node executable are counted.

The same impossible ceiling was enforced independently by candidate construction, embedded
TypeScript startup verification, and both Rust server verification paths. Omitting browser files
or narrowing the measurement roots would have made the image build but weakened the runtime
attestation contract.

## Fix Summary

- Raise the closed runtime-measurement file ceiling from 256 to 512 in the release gate, embedded
  runtime verifier, and Rust release authority.
- Keep the independent 128 KiB canonical measurement-document ceiling, exact path inventory,
  per-file SHA-256 validation, and all measurement roots unchanged.
- Prove the inclusive boundary in JavaScript and TypeScript: 512 files are accepted and 513 are
  rejected.
- Centralize the Rust boundary in one constant/helper and test `1`/`512` as valid and `0`/`513` as
  invalid.
- Correct the expanded release-gate fixture to generate one valid 64-character SHA-256 value for
  indexes above 255.

## Files Modified

| File | Change |
|------|--------|
| `jobs/scripts/managed-cloud-release-gate.mjs` | Raise the bounded candidate/runtime measurement ceiling to 512 |
| `jobs/scripts/managed-cloud-release-gate.test.mjs` | Cover 512/513 with valid 64-character fixture digests |
| `jobs/automation/src/managed-cloud-runtime.ts` | Match the startup parser and filesystem walker to the 512-file contract |
| `jobs/automation/tests/managed-cloud-runtime.test.ts` | Cover the exact embedded-runtime boundary |
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Share the Rust ceiling across artifact and release-evidence validation and test it |
| `CHANGELOG.md` | Record the fail-closed CI correction without changing launch authority |
| `docs/work/IMPL-PHASE-611-JOBS-MANAGED-CLOUD-LAUNCH-AUTHORITY.md` | Reconcile the final branch inventory and post-fix evidence |
| `docs/work/REVIEW-PHASE-611-JOBS-MANAGED-CLOUD-LAUNCH-AUTHORITY.md` | Record the independent correction review and still-required exact-tip gates |

## Edge Cases Handled

- zero measurement files remain invalid;
- one and exactly 512 canonical measured files are valid;
- 513 or more files fail closed before authority can be minted;
- every retained Chromium, ffmpeg, application, native, package, and Node file remains measured;
- changed, missing, empty, special, symlinked, duplicate, unsorted, or digest-mismatched files still
  fail the existing exact checks; and
- no customer flag, role, capability, protocol version, migration, or hosted authority changes.

## How to Test

Observed locally on 2026-08-24:

```bash
node --test jobs/scripts/managed-cloud-release-gate.test.mjs
# PASS: 16 tests, including 512 accepted / 513 rejected

node --experimental-strip-types --check jobs/automation/src/managed-cloud-runtime.ts
node --experimental-strip-types --check jobs/automation/tests/managed-cloud-runtime.test.ts
# PASS

cargo fmt --all -- --check
git -P diff --check
# PASS
```

Run on a resource-capable successor or exact-tip CI before accepting the correction:

```bash
npm --prefix jobs run test --workspace @bluey/jobs-automation -- \
  tests/managed-cloud-runtime.test.ts
cargo test --manifest-path server/Cargo.toml \
  managed_cloud_runtime_measurement_file_bound_is_closed
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path server/Cargo.toml --all-targets -- --test-threads=4
# Then run the exact jobs-ci managed-runner Docker build and native smoke.
```

## Known Limitations

- The local machine had 5.0 GiB free, below the 8 GiB release-work floor; no PortableSSD was
  mounted, and Docker was unavailable. Vitest, Cargo compilation/tests, and the managed-runner
  Docker build were therefore not rerun locally after this correction.
- The earlier full-suite counts prove the Phase 611 baseline at `423fba5c`, not this correction.
  The successor must record exact-tip results rather than inheriting those counts.
- Registry publication/read-back, signing, hosted PostgreSQL/Temporal behavior, real runner
  capacity, image-digest/read-only-rootfs attestation, cohort/canary/rollback, and every release
  flag remain parked external gates.
