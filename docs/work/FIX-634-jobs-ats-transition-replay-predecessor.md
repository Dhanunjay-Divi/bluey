# FIX-634 — ATS transition replay predecessor binding

**Status:** Fixed
**Severity:** High
**Found:** 2026-08-06
**Fixed:** 2026-08-06
**Component:** ATS activation and quarantine transition replay

## Summary

Higher-sequence activation and quarantine replays accepted an arbitrary
well-formed predecessor value as long as it was not the current head value.
This did not prove that the caller was replaying the immutable stored
transition.

## Root cause

Replay compared against current mutable head state instead of loading the exact
predecessor recorded in the immutable transition/command row.

## Fix

- Activation replay in both dialects now loads the immutable transition row and
  requires the caller's expected predecessor transition to match exactly.
- Quarantine replay now requires the exact stored predecessor command digest and
  sequence.
- Wrong hashes, current-head hashes, current-command hashes, and regressions
  fail without mutation; exact predecessor replay remains idempotent.

## Verification

- SQLite sequence-two activation and quarantine replay regressions passed.
- Legacy activation replay and revocation/quarantine lifecycle tests passed.
- The optional PostgreSQL parity test compiled and explicitly skipped without
  `BLUEY_TEST_POSTGRES_URL`.
- Strict Clippy and formatting passed.

## Production impact

No authority was applied and no live state changed.
