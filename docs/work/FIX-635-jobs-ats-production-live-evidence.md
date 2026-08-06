# FIX-635 — Production ATS activation requires live and sandbox evidence

**Status:** Fixed
**Severity:** Critical
**Found:** 2026-08-06
**Fixed:** 2026-08-06
**Component:** ATS manifest activation admission

## Summary

A non-shadow activation could be imported from a manifest backed only by an
authorized-sandbox layout. That contradicted the Round 604 production boundary,
which requires both authorized-sandbox and authorized-live evidence classes.

## Root cause

Activation admission checked only for one non-synthetic evidence object and the
absence of synthetic layouts. It did not prove the complete required check and
signed-layout class pair for every certified runtime.

## Fix

- Non-shadow activation now requires both `authorized_live` and
  `authorized_sandbox` check-result pairs for every runtime target.
- It separately requires signed layout-observation pairs for both classes and
  every runtime target.
- Synthetic or unauthorized evidence remains shadow-only.
- Test-only certified fixtures now construct the exact two-class evidence
  matrix rather than weakening production validation.

## Verification

- General activation succeeds only with both class pairs.
- Missing live or missing sandbox layout evidence is rejected.
- Synthetic evidence and synthetic-layout production negatives remain green.
- Local and cloud certified-code-path source fixtures continue to pass.

## Production impact

Authorized live evidence was not created locally. Production activation remains
impossible until approved external evidence is imported and independently
signed.
