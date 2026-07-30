# FIX-579: Jobs Direct Worker Linux Startup

## Issue

The direct and global Jobs discovery services both failed at process startup
on production Linux after activating the first immutable worker release.

## Root Cause

Round 578 isolated the global Jobhive worker from the root
`@bluey/jobs-automation` barrel, but direct discovery still imported that
barrel through `discovery-runtime.ts` and `discovery-provider.ts`. The barrel
eagerly loaded resume/PDF helpers and `pdfjs-dist`, which requires an optional
native canvas binding that was not present in the Linux runtime.

Direct discovery only needs curated-feed and public-ATS discovery helpers. It
does not parse or render resumes.

## Fix Summary

Added the narrow `@bluey/jobs-automation/discovery-runtime` export and moved
both direct-discovery imports to it. Extended the executable import-boundary
test so both direct and global worker entry points fail if they load PDF.js,
pdf-lib, or fontkit.

## Files Modified

| File | Change |
|------|--------|
| `jobs/automation/package.json` | Exposes the discovery-only runtime subpath. |
| `jobs/automation/src/discovery-runtime.ts` | Re-exports only curated-feed and public-ATS helpers. |
| `jobs/workflows/src/discovery-runtime.ts` | Imports curated-feed helpers from the narrow subpath. |
| `jobs/workflows/src/discovery-provider.ts` | Imports ATS helpers from the narrow subpath. |
| `ops/tests/test-bluey-jobs-global-worker-import-boundary.sh` | Loads both built workers while rejecting document dependencies. |
| `CHANGELOG.md` | Records the production startup correction. |

## Verification

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

## Boundaries

- This changes dependency loading only. It does not expand source eligibility,
  submission authority, browser distribution, or production feature flags.
- Resume and document workflows continue to use the full automation package.
- Worker activation and source catch-up remain deployment gates.
