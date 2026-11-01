# FIX-817: Protocol-only fixture must not initialize unrelated PDF exports

> Preflight: bluey-ops, current AGENTS, Round638; no product behavior change.

## Issue

Private run23761 built Rust, then failed on DOMMatrix during bundled client
import, before API startup. This is setup failure, not causal-red evidence.

## Root Cause

ExecutionLeaseClient imports assertFinalSubmitProof via the automation package
index. That index also re-exports document code, eagerly initializing PDF.js in
the single-file Node fixture bundle. The protocol-only process does not provide
browser rendering APIs and should not import unrelated rendering functionality.

## Fix Summary

The test bundler resolves the client's sole runtime root export to its actual
production final-submit-proof.ts re-export module. Assert the production index
still exports that module and that the importer is exactly execution-lease.ts.
No mock, replacement proof checker or product import behavior is introduced.
Add a cheap bundle/import-only preflight before Cargo; its private output is
separate from the later actual reproduction bundle and cannot qualify a claim.

## Files Modified

- jobs/scripts/run-nonprod-runner-control-plane-tests.mjs: narrow source resolution
  and import-only preflight.
- docs/work/COMMAND-PHASE-638-CAUSAL-RED.md: preflight before expensive compilation.

## Edge Cases Handled

Changed export/importer boundaries fail closed. Preflight needs no compiled Rust
artifacts and reports sourceAccepted:false. Actual reproduction still checks
owned binary identities and uses the complete real production lease client.

## How to Test

Run the revised COMMAND-PHASE-638-CAUSAL-RED.md in isolated private PG/launcher.
Correction execution is unverified at this record's creation.

## Known Limitations

No causal red, production fix, positive lifecycle or release is proven by imports.
