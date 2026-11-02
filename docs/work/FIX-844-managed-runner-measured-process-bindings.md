# FIX-844: Bind managed process hashes to measured automation and claimed image

Codex preflight: `bluey-ops`; Jobs-only Phase639 from local checkpoint2b85339a.

## Issue

Process-runtime grant hashing bound the configured image and automation hashes,
but startup did not independently connect those two values to the verified
runtime measurement and server-claimed release artifact. Agreement between a
grant and echoed environment fields did not establish that connection.

## Root Cause

`runnerProcessRuntimeGrantFromEnv` correctly read the existing closed runtime
descriptor, but `startRunner` passed it straight to volume control. The existing
runtime scanner already verified canonical embedded measurement, exact current
file set and every measured file's bytes; it did not expose an automation-only
derivative. Startup also omitted comparison with the claimed manifest artifact.

## Fix Summary

- The release inspector derives the automation bundle from its fully validated
  runner inventory/embedded measurement. All ordered `app/automation/` measured
  files contribute; package metadata and executable index are mandatory.
- The live scanner derives the same canonical-LF v1 domain during the existing
  validated scan, without a second Chromium-file hashing pass or configurable
  identity override. Only the exact managed-runner component/role exposes it.
- Startup parses the process grant once and checks the measured bundle before
  managed-runtime claim. It compares image hash with the claimed manifest
  artifact before constructing/starting volume control. Nonmanaged execution
  retains its existing behavior.

## Files Modified

`jobs/scripts/managed-cloud-release-gate.mjs` and its tests;
`jobs/automation/src/managed-cloud-runtime.ts` and its tests;
`jobs/runner/src/server.ts` and `server-storage-startup.test.ts`.

## Edge Cases Handled

Missing required automation files, coherently changed automation versus
runner-only bytes, unmeasured/tampered files, invalid role, missing/configured
bundle mismatch, claimed artifact mismatch, and startup ordering.

## Verification

Parent's release-gate suite37/37 passes, including five new derivation tests.
Independent static reviews of inspector and four-file live integration found
no P0-P3 issue. Parent session89044 terminated0 after automation build, all five
Jobs workspace typechecks, all811 automation tests and315 runner tests. One
explicit browser integration test was skipped by the existing offline mode;
this command did not claim browser execution. All ten new tests passed within
those suites. The four exact source/test hashes were unchanged afterward and
the canonical launcher's run root and empty parent were removed.

## Known Limitations

This patch does not establish that archive bytes were loaded/run, independently
measure Playwright package/Chromium revision metadata, derive Rust fixture
parity, issue fresh startup grants, or qualify normal native-storage startup.
OCI manifest digest and Docker image/config ID remain distinct and must be
proven separately in the actual-artifact test. No production flag changed.
