# FIX-858: Keep the private cost-report prototype in test builds only

> **Codex preflight:** `$bluey-ops`; current source and review are authority.

## Issue / Root Cause

The new report and stored-receipt adapter were enabled by either unit tests or
`integration-test-support`, but every caller is an inline unit test. The
non-test integration library therefore compiled an unreachable private report
root, a static strict-lint risk with no useful execution surface.

## Fix Summary

Narrow all30 newly added report-graph configuration attributes to `cfg(test)`.
Actual PostgreSQL verification remains an inline unit case, so no external
integration driver or public API is needed. Do not add a dead-code suppression
or broaden visibility to hide the issue. Existing production functions and
test-helper behavior are unchanged.

## Files Modified

- `server/src/db/jobs_provider_cost_holds.rs`: report types/helpers/root.
- `server/src/db/jobs/customer_data.rs`: report receipt-validation graph.

## How to Test

Changed-file rustfmt, server formatting and diff checks pass. Independent
corrected-source review confirms the narrowing. Strict all-target Clippy and focused/full
database execution remain unexecuted behind the unchanged32-GiB capacity gate;
this document does not claim an observed compiler warning or a lint pass.

## Known Limitations

The report is a test-only prototype. Runtime, observer/isolation evidence,
complete cost attribution and the full Jobs release goal remain open.
