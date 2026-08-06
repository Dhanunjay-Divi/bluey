# FIX-633 — Signed ATS canary capacity enforcement

**Status:** Fixed
**Severity:** Critical
**Found:** 2026-08-06
**Fixed:** 2026-08-06
**Component:** Jobs ATS Phase B certification transaction

## Summary

The initial Phase B capacity query did not read `canary_max_submissions`, scoped
the distinct-account limit to one period, and computed concurrency from a field
that was populated at insertion. A signed canary could therefore exceed its
activation-wide total/account limits while its concurrency count remained zero.

## Root cause

Reservation timestamps were mistaken for reservation lifecycle state, and all
four signed dimensions were aggregated through one period-scoped query.

## Fix

- Enforced activation-wide total submissions from `canary_max_submissions`.
- Enforced the activation-wide distinct-account cap.
- Enforced the daily side-effect cap independently.
- Derived live concurrency from the authoritative attempt status, counting
  `reserved`, `running`, `side_effect_unknown`, and missing attempt state
  conservatively.
- Preserved SQLite immediate transactions and PostgreSQL transaction locking so
  validation and insertion serialize atomically.
- Allowed independently bounded daily/account/concurrency limits only when each
  is positive and no greater than the signed activation maximum.

## Verification

- The direct four-limit matrix passed.
- Every denial preserved the second binding at `preflight`/fence zero and left
  the reservation count unchanged.
- Activation validation accepted bounded independent limits and rejected every
  limit that exceeded the activation maximum.
- The complete ATS authority suite passed after the change.

## Production impact

No canary account, provider, runner, feature flag, or deployment was enabled.
