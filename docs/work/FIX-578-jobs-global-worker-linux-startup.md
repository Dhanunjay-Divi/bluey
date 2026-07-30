# FIX-578: Jobs Global Worker Linux Startup

## Issue

The global Jobs discovery service failed at process startup on production
Linux while the direct discovery service remained healthy.

## Root Cause

`jobs/workflows/src/global-discovery-runtime.ts` imported the root
`@bluey/jobs-automation` barrel. That barrel eagerly exports resume and PDF
document helpers, so loading the global feed worker also loaded `pdfjs-dist`
and its optional native canvas dependency. The macOS-built optional binding was
not a valid Linux native module, and PDF.js then failed because `DOMMatrix` was
unavailable.

The global worker only needs Jobhive manifest and artifact streaming helpers.
It does not parse or render resumes.

## Fix Summary

Added the narrow `@bluey/jobs-automation/jobhive-runtime` export and moved the
global worker to that import boundary. Added a Linux-style module-load guard
that fails if the global worker imports PDF.js, pdf-lib, or fontkit.

## Files Modified

| File | Change |
|------|--------|
| `jobs/automation/package.json` | Exposes the narrow Jobhive runtime subpath. |
| `jobs/automation/src/jobhive-runtime.ts` | Re-exports only manifest and artifact helpers used by global discovery. |
| `jobs/workflows/src/global-discovery-runtime.ts` | Imports global discovery helpers from the narrow subpath. |
| `ops/tests/test-bluey-jobs-global-worker-import-boundary.sh` | Verifies the built global worker loads without document dependencies. |
| `CHANGELOG.md` | Records the production startup fix. |

## Edge Cases Handled

- The test executes the built ESM worker through a custom loader rather than
  checking source text alone.
- Importing `pdfjs-dist`, `pdf-lib`, or `@pdf-lib/fontkit` fails the test.
- The full automation package still exports document helpers for the portal
  and resume workflows that use them.
- Direct discovery and global discovery continue to share the same immutable
  worker artifact.

## How to Test

```bash
npm test --prefix jobs
npm run typecheck --prefix jobs
npm run build --prefix jobs
ops/tests/test-bluey-jobs-global-worker-import-boundary.sh
ops/tests/test-bluey-jobs-discovery-units.sh
ops/tests/test-check-bluey-jobs-discovery.sh
ops/tests/test-verify-bluey-jobs-workers-archive.sh
ops/tests/test-install-bluey-jobs-workers.sh
node jobs/scripts/ci-guards-self-test.mjs
node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/check-provenance-licenses.mjs
git diff --check
```

## Known Limitations

- This fixes accidental document-runtime coupling. It does not change source
  licensing, source eligibility, original-employer revalidation, or submission
  authority.
- Production activation and source catch-up remain separate deployment gates.
