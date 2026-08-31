# FIX-704: Transport-Only Track Writes Invalidated Prepared Evidence

> **Codex preflight:** Loaded `$bluey-ops` and diagnosed the execution-authority regression against
> the current Phase 613 worktree. Only local test databases were inspected; no provider, hosted
> database, external account, deployment, or production flag was used.

## Issue

A semantic no-op Career Track upsert could make an already prepared application fail execution
authority even though its reviewed policy revision and candidate facts were unchanged.

## Root Cause

Profile evidence hashed the full Track projection, including `created_at_ms`, `updated_at_ms`, and
derived `match_count`. A no-op Track persistence retry advanced transport timestamps while reusing
the exact Phase 613 policy head. Rebuilding current evidence then produced a different ID and
content hash, so the frozen-evidence equality check denied the first application's lease.

## Fix Summary

- Canonicalize the Track inside profile evidence by zeroing creation/update timestamps and
  `match_count` before hashing.
- Preserve semantic Track fields and the complete reviewed policy authority in the evidence
  snapshot.
- Extend the parallel-browser-profile regression to prove the first application's frozen policy,
  eligibility, evidence ID/hash, stored evidence, and execution authority remain current after a
  second semantic no-op fixture write.
- Keep the actual one-browser-profile lease exclusion and fence progression assertions unchanged.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/evidence.rs` | Exclude Track transport and derived projection fields from candidate-evidence hashing |
| `server/src/db/jobs/tests.rs` | Prove evidence stability before exercising the existing parallel-lease exclusion |

## Edge Cases Handled

- repeated Track upsert with an identical policy head;
- changed persistence timestamps;
- changed derived match count without changed candidate truth;
- two applications sharing one browser profile; and
- a genuine policy/evidence change, which remains execution-denying.

## How to Test

```bash
cargo test --manifest-path server/Cargo.toml --lib \
  execution_lease_excludes_parallel_runs_for_one_browser_profile -- --nocapture
```

The pre-fix failure was observed at the first lease claim after the evidence hash changed. The fix
and strengthened named regression are included in the clean 1,401-test server-library target,
which passed. The complete all-target result was 1,517 passed with zero failures or ignored tests.

## Known Limitations

- Only transport/derived Track fields are excluded. Role, locations, activation, identity,
  reviewed policy, and other semantic evidence continue to invalidate stale work.
- This fix removes a false denial; it does not broaden execution authority.
