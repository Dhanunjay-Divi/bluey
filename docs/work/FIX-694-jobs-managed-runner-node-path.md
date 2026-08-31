# FIX-694: Normalize the Managed-Runner Node Binary Before Measurement

> **Codex preflight:** Loaded `$bluey-ops` and reconciled the current Phase 611 worktree,
> release contract, pinned container source, and exact failing CI log. No registry, credential,
> hosted runner, deployment, customer cohort, external effect, or release flag was used.

## Issue

Pull request #32 Actions run `33313140429`, job `99261666660`, passed every preceding Jobs,
privacy, portal, and native-storage gate but failed while constructing the managed-runner image:

```text
Runtime measurement root is missing: /usr/local/bin/node
```

The failure occurred before the image could be created or its native addon smoked.

## Root Cause

Phase 611 deliberately established `/usr/local/bin/node` as the cross-language managed-runtime
authority. The separately pinned Playwright 1.61.1 Noble base installs NodeSource Node as the
regular file `/usr/bin/node`; it does not provide the authoritative runner path. Unqualified
build-time `node` commands succeeded because `/usr/bin` was in `PATH`, but the fail-closed runtime
measurement correctly rejected the missing authoritative root.

The former 256-file ceiling failed first while walking Chromium and masked this independent path
mismatch. Raising the legitimate closed ceiling to 512 in FIX-693 exposed the next fail-closed
error. The pinned base also places global package-manager material under `/usr/lib` and `/usr/bin`,
so cleanup restricted to `/usr/local` would have left unnecessary tools in the final image.

The first failing pull-request run also built GitHub's synthetic merge SHA rather than binding the
runtime identity to the pull-request head. That association is useful for mergeability, but it is
not exact-head runtime-attestation evidence.

## Fix Summary

- Verify the pinned base supplies `/usr/bin/node`, then copy it with `install -m 0555` to the
  already-authoritative `/usr/local/bin/node` before runtime measurement.
- Prove the normalized path is a regular file rather than a symlink, execute measurement through
  it, and remove the original `/usr/bin/node` after measurement.
- Preserve `/usr/local/bin/node` in the TypeScript verifier, JavaScript release gate, Rust release
  authority, image command, and CI/release native smoke instead of redesigning the contract around
  a base-image implementation detail.
- Remove package-manager binaries and global modules from both NodeSource and `/usr/local` paths.
- Make CI check out and embed the same `github.event.pull_request.head.sha` for pull requests, with
  `github.sha` as the non-PR fallback, and regression-test both sides of that binding.
- Reject symbolic links anywhere inside a measured runtime root during filesystem generation,
  startup remeasurement, stored OCI inspection, and Rust release-evidence validation.

## Files Modified

| File | Change |
|------|--------|
| `.github/workflows/jobs-ci.yml` | Check out and bind image identity to the same PR head/fallback SHA, then smoke `/usr/local/bin/node` |
| `.github/workflows/release.yml` | Apply the same exact-source checkout and runtime contract to the release gate |
| `jobs/runner/Dockerfile` | Normalize a regular Node binary before measurement and minimize both source layouts |
| `jobs/runner/tests/container-hardening.test.ts` | Lock normalization, cleanup, runtime command, and source-SHA selection |
| `jobs/automation/src/managed-cloud-runtime.ts` | Retain `/usr/local/bin/node` as the runner remeasurement root |
| `jobs/automation/tests/managed-cloud-runtime.test.ts` | Prove the normalized runner Node file is measured and required |
| `jobs/scripts/managed-cloud-release-gate.mjs` | Retain the established runtime and stored-image authority |
| `jobs/scripts/managed-cloud-release-gate.test.mjs` | Exercise OCI runner fixtures with the authoritative path |
| `server/src/db/jobs/managed_cloud_release_authority.rs` | Keep runner and workflows evidence on the same explicit path |
| `CHANGELOG.md` and Phase 611 records | Record the fail-closed correction and remaining proof |

## Edge Cases Handled

- A missing base-image Node binary fails before any measurement is written.
- A symlink at the normalized path cannot satisfy the Docker build assertion, and a symlink at any
  measured runtime root fails generation and every verifier rather than being skipped or followed.
- The original `/usr/bin/node` and package-manager entrypoints do not remain in the final runtime.
- Startup remeasurement and stored-image verification require the same file measured at build time.
- Pull-request evidence is built from and embeds the same actual head SHA; push and dispatch runs
  retain the same non-empty fallback SHA for checkout and provenance.
- No measurement limit, hash, root set, runtime principal, capability, flag, or hosted authority is
  weakened.

## How to Test

Observed locally after the final correction:

```text
automation full test suite and typecheck
  PASS: 660 tests; 1 intentional skip; TypeScript clean

runner full test suite and typecheck
  PASS: 308 tests; TypeScript clean

managed-cloud release-gate tests
  PASS: 17 tests, including filesystem and OCI symlink rejection

server Rust formatting
  PASS

git diff --check
  PASS
```

Before acceptance, CI must run the exact managed-runner Docker build/native smoke and complete
Rust 1.98 check, Clippy, and tests at the corrected head. The built image must contain a regular
`/usr/local/bin/node`, omit `/usr/bin/node`, load the staged native addon, and embed the PR head SHA.

## Known Limitations

- Docker is unavailable on the local Mac, so source tests cannot replace the required Linux image
  build and native-addon smoke.
- Registry publication/read-back, protected signing, hosted runtime attestation, read-only-rootfs
  enforcement, real capacity, canary, cohort, rollback, deployment, and every release flag remain
  parked.
