# FIX-827: Private runner denial lacks a safe HTTP error fingerprint

## Issue

Ordinary private API61106 returned400 at real_client_claim in2ms. Existing metadata
retained only status/timing, so the exact public validation error was lost during
canonical cleanup. No PG error or panic was observed. Do not guess its cause.

## Root Cause

The real client intentionally exposes only a typed error and status; the private
transport adapter had retained an error-body hash only in causal-red mode. The
positive-mode harness therefore discarded evidence needed to distinguish HTTP
validation rejections without exposing private response contents.

## Fix Summary

On a non-OK protocol response, retain a bounded status, exact-public-constant
category and SHA256 of the bounded response. Only a fixed allowlist can yield a
named category. Unknown or private-containing bodies become unclassified; no body,
field capture, request ID or candidate content is printed. The production client,
API response, authentication and authority checks are unchanged.

## Files Modified

- `jobs/scripts/nonprod-runner-control-plane.mjs`
- `jobs/scripts/nonprod-runner-control-plane.test.mjs`

## Edge Cases Handled

Known invalid execution, volume-proof mismatch, volume request, managed-authority
and unavailable/unauthorized responses; private text appended to a known constant
does not match it. Invalid status isnull. Response reads retain their existing bound.

## How to Test

58 Node report/cleanup/privacy tests passed, zero skipped, including exact-match,
private-content stripping/hash and unknown status cases. Runtime fingerprint
capture is pending a reviewed source/privacy-frozen private API reproduction.

## Known Limitations

No exact cause for61106 or positive managed lifecycle result is established by
these diagnostics. Full release and broader Jobs gates remain open.
