# FIX-802: Temporal workflow imported activity-only Node modules

Preflight: `bluey-ops`; Round 630 real runtime qualification.

## Issue

The first actual Temporal worker could not start. The real SDK rejected its
workflow bundle before any of the ten integration cases could execute. Mocked
workflow unit tests and TypeScript compilation had not exercised that boundary.

## Root cause

`jobs/workflows/src/workflows.ts` imported two deterministic checksum/snapshot
functions from the automation package root. That root reexports all modules,
including network, browser and PDF/document activity code. Temporal's workflow
bundle therefore included forbidden Node modules even though the v2 workflow
only needed opaque command coordination. The direct real-SDK bundle regression
reproduced `UnhandledSchemeError` for `node:crypto`, `node:buffer`,
`node:dns/promises`, `node:fs/promises`, `node:fs`, `node:https` and `node:net`.

## Fix

- Export the already-existing, deterministic `approved-execution` module as an
  explicit package subpath. No checksum, snapshot or validation function changes.
- Import workflow runtime helpers only through that pure subpath; keep the
  root `SubmissionReceipt` dependency type-only, erased before execution.
- Add a real SDK bundle test for both exported workflow protocols and the
  production failure converter. Do not mock the SDK, ignore disallowed modules,
  replace Node globals or relax Temporal's determinism checks.

## Verification and limits

The initial direct bundle test failed 1/1 with the exact module diagnostics.
After the narrow import correction, the real SDK bundle test passed 1/1 and
workflow TypeScript typecheck passed, following a fresh automation build.
The subsequent actual local service run passed all ten cases, including restart,
replay, Update, history privacy and exact cleanup. The final full Jobs workspace
run passed 2,039 tests, all typechecks/builds and audit. IMPL-630 records scope,
failure history and external release holds.
No migration, application authority, history schema, package dependency version,
production flag, provider action or host/overlay code changes in this fix.
