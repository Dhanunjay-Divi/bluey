# FIX-796: Jobs CSV parser dependency advisory

> **Codex preflight:** `bluey-ops` and repository `AGENTS.md`.

## Issue

The Phase 629 managed-runner Docker validation reported a production-dependency
`npm audit` finding for `csv-parse` 6.2.1: moderate
[GHSA-8cw4-87c7-c6xx](https://github.com/advisories/GHSA-8cw4-87c7-c6xx).
This records the observed dependency inventory finding, not a demonstrated
exploit of Bluey's Jobhive importer.

## Root Cause

`jobs/automation/package.json` allowed `csv-parse` `^6.2.1`; the runtime lockfile
resolved 6.2.1. The advisory concerns duplicate `__proto__` column names when
object columns and `group_columns_by_name: true` are enabled together. The
advisory identifies 7.0.2 as patched.

The actual Jobhive parser uses a `columns` callback, not array-row parsing. Its
callback validates normalized duplicate headers and required columns before
returning the headers. It does **not** enable `group_columns_by_name`.
Consequently, the affected grouping combination is not configured here, and
this batch does not claim the existing importer was exploitable.

## Fix Summary

- Pin the direct automation dependency to exactly `csv-parse` 7.0.3, removing
  the known affected version from the intended runtime dependency inventory.
- Retain the existing production parser and its extra-column compatibility.
  Unknown extra headers are not generally forbidden by its current contract.
- Add bounded synthetic regressions for duplicate `__proto__` headers,
  whitespace-normalized duplicate `__proto__` headers, duplicate `constructor`
  headers, and a `__proto__` header replacing the required `raw` column. Each
  must fail header validation before any batch is emitted and leave all own
  `Object.prototype` descriptors and a fresh object's prototype/marker unchanged.
- Add positive cases for unique `future_extension` and `__proto__` extra headers
  alongside every required column. They must emit the exact normalized candidate
  while ignoring the extra key and preserving object prototypes.

The official
[node-csv changelog](https://github.com/adaltas/node-csv/blob/master/packages/csv-parse/CHANGELOG.md)
describes 7.0.0 as an accidental major without breaking changes and records
7.0.3's typed-column-key preservation. Choosing the exact 7.0.3 patch includes
the advisory fix without relying on a floating major range.

## Files Modified

| File | Change |
|------|--------|
| `jobs/automation/package.json` | Exact 7.0.3 dependency pin |
| `jobs/automation/tests/jobhive-artifact.test.ts` | Malicious-header/no-prototype-mutation regressions |
| `jobs/package-lock.json` | Parent-owned mechanical lockfile regeneration completed and audited |
| This FIX record | Advisory reachability distinction and verification boundary |

## Edge Cases Handled

- Duplicate dangerous names are rejected rather than grouped into object keys.
- Trimming cannot conceal duplicate header names.
- A dangerous unknown name cannot substitute for a required schema column.
- Unknown additional-column compatibility remains unchanged; these tests do
  not assert a new denylist or an all-unknown-header rejection policy.

## How to Test

Reproduction commands (use the isolated launcher/private npm configuration):

```bash
cd jobs
npm test --workspace=@bluey/jobs-automation -- tests/jobhive-artifact.test.ts
npm run typecheck --workspace=@bluey/jobs-automation
npm audit --omit=dev
```

Parent-run final validation passed: the 13-case Jobhive file, all 804 automation
tests and the complete 2,025-test Jobs workspace run, all five typechecks/builds,
and installed full audit with zero findings. The corrected Linux image also
loaded CSV 7.0.3 and passed its native/hardening smoke. The original bounded
author did not execute these commands; evidence comes from the parent run and
is recorded in the Phase 629 IMPL.

## Known Limitations

- Audit results describe the observed dependency inventory, not future advisory immunity.
- This dependency update is not permission to submit applications, send email,
  publish artifacts, deploy, or change production feature flags.
