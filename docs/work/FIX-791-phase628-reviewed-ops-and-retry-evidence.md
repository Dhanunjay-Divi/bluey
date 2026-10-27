# FIX-791: Phase 628 reviewed operations and retry evidence

> **Codex preflight:** Loaded `$bluey-ops` and reconciled it with the current
> Phase 628 round, local handoff, and independent client/release review.

## Issue

The active v3 operations procedure still described the superseded legacy startup
endpoint and recovery sequence. The response-loss retry test compared bodies but
did not prove every delivery received a fresh valid worker-auth signature.

## Root Cause

The original v3 startup authority documentation at heads 062/040 remained next
to the later boot-generation procedure at heads 065/043. The retry fixture did
not retain request headers, so a future request-signature cache could replay a
nonce while keeping the body test green.

## Fix Summary

- Replaced the active v3 procedure with boot registration plus boot startup,
  current heads 065/043, exact retry identity, and the no-speculative-failure
  heartbeat rule.
- Captured all four retry deliveries and verified stable bodies/content digests,
  distinct nonces/signatures, and each HMAC against its path and canonical v2
  worker-auth payload.

## Files Modified

| File | Change |
|---|---|
| `jobs/OPERATIONS.md` | Reconciled the active release-v3 readiness procedure. |
| `jobs/automation/tests/managed-cloud-runtime-client.test.ts` | Proved fresh valid auth headers for registration and startup retries. |

## Edge Cases Handled

- Registration and startup each preserve their request/session binding across a
  lost response while producing a fresh nonce and signature per delivery.
- Historical 062/040 migrations remain documented as historical, not actionable
  v3 readiness instructions.
- Ambiguous startup never authorizes a speculative `degraded/probe_failed`
  heartbeat.

## How to Test

```bash
cd jobs/automation
npm test -- managed-cloud-runtime-client.test.ts
git diff --check -- jobs/OPERATIONS.md jobs/automation/tests/managed-cloud-runtime-client.test.ts
```

Observed: focused Vitest passed: 1 file, 11 tests. The package has no `format`
script, so `npm run format -- --check` reports a missing script rather than a
format result.

## Known Limitations

- This correction does not add the separate Phase 628 SQL trigger/function
  semantic mutation coverage tracked by independent review P2.
