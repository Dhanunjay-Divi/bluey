# FIX-616: Fail Closed on Missing ATS Portal Authority

> **Codex preflight:** Loaded `$bluey-ops` before diagnosis and reconciled its
> operating memory against the active Round 604 source and portal contracts.

## Issue

The Jobs portal trusted generic eligibility booleans without a bounded ATS
certification summary, so a mixed-version or malformed response could present
Auto-submit or widen a Local-only certification to an available Cloud runner.

## Root Cause

Portal views consumed `JobEligibilityDecision` as a compile-time TypeScript
shape rather than decoding the API value at runtime. The previous model had no
exact certification status, verification window, runner scope, or safe summary
boundary, and Auto-mode helpers checked general runner availability instead of
the intersection of available and certified runner kinds.

## Fix Summary

Added a strict, exact-key decoder for the server-authored
`ats_certification` summary and one normalized eligibility boundary used by
Matches, Applications, preview flow, and submission-mode selection. Missing,
unknown, malformed, inconsistent, future, expired, suspended, revoked, or
drifted authority cannot display Certified or enable a runner. Auto-submit now
requires at least one runner that is both available and present in the signed
server scope. Display text is bounded and rejects internal identifiers, hashes,
counts, signature detail, check IDs, URLs, and email addresses.

## Files Modified

| File | Change |
|------|--------|
| `jobs/portal/src/types.ts` | Added the bounded ATS certification wire types while keeping raw response authority untrusted. |
| `jobs/portal/src/lib/ats-certification.ts` | Added strict decoding, mixed-version fallback, runner-scope intersection, and safe presentation. |
| `jobs/portal/src/components/AtsCertificationSummary.tsx` | Added truthful status, runner, verification, expiry, reason, and next-action display. |
| `jobs/portal/src/App.tsx` | Prevented preview Auto mode from trusting raw eligibility. |
| `jobs/portal/src/lib/application-flow.ts` | Required an available runner inside the exact certification scope. |
| `jobs/portal/src/views/MatchesView.tsx` | Applied normalized eligibility and the bounded summary to match actions. |
| `jobs/portal/src/views/ApplicationsView.tsx` | Applied the same boundary to queued applications and runner actions. |
| `jobs/portal/src/**/*.test.*` | Added malformed, mixed-version, expiry, revocation, inconsistency, and runner-scope regressions. |

## Edge Cases Handled

- An older server omits `ats_certification` while claiming `certified`.
- The summary contains an unknown field or an unrecognized provider label,
  status, runner kind, adapter version, or timestamp.
- An active summary conflicts with a non-certified capability.
- A summary is expired locally or reports an implausibly future verification.
- Local-only authority is paired with only a Cloud runner, and vice versa.
- Internal target, tenant, manifest, activation, evidence, circuit, rollout,
  selector, signature, count, hash, UUID, token, URL, or email detail appears in
  customer-facing reason text.

## How to Test

```bash
cd jobs
npm test --workspace @bluey/jobs-portal
npm run typecheck --workspace @bluey/jobs-portal
npm run build --workspace @bluey/jobs-portal
git diff --check -- jobs/portal docs/work/FIX-616-jobs-portal-certification-scope.md
```

The portal checkpoint passed 16 files and 178 tests, strict typecheck, and the
production build. A focused rerun after the final capability/status consistency
guard passed 4 files and 60 tests plus strict typecheck.

## Known Limitations

- This is a display and client-action guard, not certification authority. The
  server resolver, frozen admission, Phase A, and Phase B remain the only paths
  that may grant employer-facing capability.
- No provider is certified and both Browser distribution flags remain disabled.
