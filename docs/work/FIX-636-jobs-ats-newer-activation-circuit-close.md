# FIX-636 — Verified newer-activation circuit closure

**Status:** Fixed
**Severity:** Critical
**Found:** 2026-08-06
**Fixed:** 2026-08-06
**Component:** ATS certification circuit authority

## Summary

The generic circuit validator accepted a `closed` event with trigger
`newer_activation` from string fields alone. An administrator could therefore
claim a successor activation without proving that an exact signed successor was
imported and applied.

## Root cause

Circuit event validation checked event shape and counters but did not resolve
the `authority_ref` through the activation/head authority graph.

## Fix

- Both dialects now require `authority_ref` to resolve to an imported, applied
  successor activation whose complete trust, manifest, activation, runtime,
  quarantine, revocation, and canary authority remains current at the
  server-controlled transaction time.
- The successor must have higher generation and channel sequence, be applied
  after the circuit opened, and name the exact predecessor activation circuit.
- Provider, target, adapter, and runtime circuits require the existing explicit
  reviewed-close path because one activation cannot dominate their full scope.

## Verification

- Forged, stale, imported-only, wrong-scope, backdated-revoked, expired,
  runtime-revoked, and canary-allowlist-revoked successor closures are rejected.
- Exact activation-scope successor and broader reviewed closures pass.
- Focused circuit tests, strict Clippy, formatting, and diff hygiene passed.
- PostgreSQL execution explicitly skipped without `BLUEY_TEST_POSTGRES_URL`.

## Production impact

No live circuit or activation was changed.
