# FIX-828: Random proof request IDs violate the server identifier grammar

## Issue

Independent source review found a runner-volume proof availability defect in both
the production client and private protocol adapter. It is separate from the exact
unretained cause of private run61106's HTTP400.

## Root Cause

Both generators use raw24-byte base64url as request ID. Rust
`RunnerVolumeAuthorityProof::validate_unsigned` calls `require_runner_identifier`,
which requires an ASCII-alphanumeric first byte. Base64url's64-character alphabet
includes '-' and '_', so2/64 (1/32) generated IDs violate the grammar before any
database work. The production method is shared across volume authority operations.
The server correctly returns InvalidRequest; its grammar must not be loosened.

## Fix Summary

Implemented/static review clear: one shared production generator with fixed
`rva-` prefix and all192 bits of entropy retained; private adapter reuses it. It
requires exactly24 entropy bytes. Added exhaustive256-leading-byte TypeScript
vectors and a zero-database Rust validator regression. Signature canonicalization,
nonce uniqueness and every authorization check remain unchanged.

## Files Modified

Production runner-volume client/test, private client adapter, and one pure
Rust validator test in the existing cfg(test) volume module. No schema/API change.

## Edge Cases Handled

Required: raw '-'/'_' prefix rejection, every prefixed value accepted, unchanged
entropy suffix, bounded identifier length, invalid entropy-size rejection.

## How to Test

Scoped rustfmt/whitespace and independent static review pass; regression execution
remains pending. Obtain prospective privacy/format gates, TypeScript checks and
actual private protocol evidence before qualification. The current58 Node tests
cover FIX827 diagnostics, not this new production generator.

Focused59716 exited0: two selected Vitest tests passed in32ms (1.79s total), with
32 unrelated tests excluded by the explicit name filter; runner tsc --noEmit passed.
No full runner-suite pass is implied. Native69688 then passed the actual Rust
256-vector validator regression1/1, zero skipped, after2m38s compile. Ordinary
managed-client claim returned200 but a later harness policy/count assertion failed
(FIX829); the full lifecycle remains unqualified. Source774 manifest
9e928d8c3a34d16bd78d5740254d1d99869735ceccec98bd1c94e1e72f00e4e8
and HEAD/status matched before/after those runs; temporary roots cleaned.

## Known Limitations

The source contract defect is proven; attribution of61106 remains unproven because
its exact response/proof identifier was not retained. No positive managed-client,
native cloud, employer submission or production readiness is implied.
