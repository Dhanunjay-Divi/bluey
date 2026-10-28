# FIX-813: Resolve PDF.js standard fonts from the installed package

Codex preflight: bluey-ops; current Jobs branch and Round637 remain authority.

## Issue and root cause

Node materialization disabled system fonts without configuring PDF.js standard
font assets. PDF.js's Node binary-data factory reads a filesystem path string;
a literal file URL or a remote/CDN fallback is not an equivalent correction.

## Fix summary and files

- `jobs/automation/src/pdfjs-node-options.ts` resolves the installed legacy PDF.js
  module with createRequire, derives its standard_fonts directory and converts
  it to an absolute filesystem path with exactly one trailing literal slash.
- Production `documents.ts` and test extraction share those options. Worker
  fetch stays disabled; production validation limits and frozen bytes are unchanged.
- The helper tests cover installed assets, POSIX/Windows/mixed separators and
  local rather than URL paths. The real Helvetica regression requires complete
  expected text, zero fetch calls and zero console warnings without suppression.

## Diagnostics and verification

Session26763 failed before collecting tests because the Vitest module runner
does not implement import.meta.resolve. createRequire corrects that environment
compatibility without changing resource authority. Session56895 then passed17/18:
the synthetic fixture's default PDF-lib font size/origin clipped its final word
outside the page. Explicit in-bounds size12/x48/y740 fixes fixture geometry; the
expected text was not weakened. Session17034 passed18/18 with source unchanged.
Warnings from deliberately malformed-PDF negative cases remain visible and do
not represent standard-font lookup failures.

Run the focused automation documents and pdfjs-node-options tests through the
canonical isolated launcher. Compiled Node qualification14801 also passed zero
warnings/fetches, complete expected text, exact PDF bytes/SHA and frozen snapshot
validation. FullJobs34408 passed2085/zero skips and all workspace typechecks/
builds. Server66480 then passed1872 all-target tests,38 actual privatePG and30
focused cases plus strict Clippy/formatting with exact source/cleanup checks.
Final review/privacy remain; this is not a release or production claim.

## Limits

This does not change candidate facts, resume bytes, unsupported-script policy,
document budgets, browser execution authority, providers or production flags.
