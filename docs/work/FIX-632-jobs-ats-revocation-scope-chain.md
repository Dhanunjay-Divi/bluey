# FIX-632 — ATS revocation scope and chain authority

**Status:** Fixed
**Severity:** Critical
**Found:** 2026-08-06
**Fixed:** 2026-08-06
**Component:** Jobs ATS certification authority and paired runtime migrations

## Summary

The first Round 604 revocation model covered only a subset of authority subjects
and did not bind a revocation generation to its exact predecessor. A signed
revocation could therefore leave policy, trust-key, target, layout, or exact
runner-component authority usable, and generation replay/regression was not
fully constrained.

## Root cause

The Rust validator and both database dialects treated revocations as immutable
rows but not as a per-policy monotonic chain. The allowed subject grammar also
stopped at aggregate runtime and omitted exact Browser-release, runner-build,
and image components that certification resolves independently.

## Fix

- Added predecessor-bound, monotonically sequenced revocation authority with
  per-policy uniqueness and paired SQLite/PostgreSQL insert guards.
- Added exact signed subjects for `policy`, `trust_key`, `target`,
  `layout_observation`, `browser_release_manifest`, `runner_build`, and
  `runner_image` alongside the existing activation, adapter, evidence,
  manifest, runtime, scope, and target-independent subjects.
- Applied all thirteen subject kinds to target status, new Phase A admission,
  and the complete Phase B authority recheck.
- Extended shared Rust/TypeScript authority vectors.

## Verification

- Revocation chain/replay/regression/wrong-predecessor test passed.
- Thirteen-scope Phase A/status/Phase B fence matrix passed.
- Legacy revocation/quarantine behavior passed.
- Rust and TypeScript canonical-vector tests passed.
- Strict Clippy, formatting, and 65-table/57-index schema parity passed.
- The PostgreSQL behavioral case compiled and explicitly skipped because
  `BLUEY_TEST_POSTGRES_URL` was unavailable; this is not live PostgreSQL proof.

## Production impact

No authority, runner, provider, feature flag, or deployment was enabled. The
change makes future signed revocation complete and fail-closed.
