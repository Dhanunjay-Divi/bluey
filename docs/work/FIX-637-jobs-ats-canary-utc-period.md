# FIX-637 — Server-owned UTC ATS canary day

**Status:** Fixed
**Severity:** High
**Found:** 2026-08-06
**Fixed:** 2026-08-06
**Component:** ATS canary daily-cap authority

## Summary

The canary daily-cap bucket reused each account's quota period. Accounts in
adjacent local dates could therefore consume separate buckets at the same
instant and exceed one activation-wide daily side-effect cap.

## Root cause

Phase B copied `jobs_attempt_reservations.period_key`, which is deliberately
derived from the account timezone, into the certification reservation.

## Fix

- Derived the canary capacity period from server `now_ms` as one UTC
  `YYYY-MM-DD` value.
- Production Phase B ignores the account quota period for certification
  capacity while retaining it in the separate metering identity.
- Both dialect capacity checks recompute and require the exact UTC value before
  any binding, marker, or reservation mutation.
- The raw helper now rejects arbitrary or stale period strings.

## Verification

- UTC-midnight boundary test passed.
- A non-UTC/mismatched period is rejected with the binding still at preflight,
  fence zero, and zero reservations.
- The four-limit atomic denial matrix remains green.

## Production impact

No canary or production rollout was enabled.
