# IMPL: ROUND-582 - Jobs Match Filter Production Audit

## Scope

**Does:**

- Audit every Matches control, default, reset, URL, empty state, and pagination
  path as a user.
- Reconcile view filtering with server-owned hard eligibility.
- Fix confirmed persistence, normalization, inactive-track, maximum-score, and
  empty-state defects.
- Verify desktop, mobile, light, dark, compact, filtered, passed, and large-list
  behavior.

**Does NOT:**

- Enable protected Jobs runtime flags.
- Change discovery, scoring, preparation, queueing, metering, or submission
  authority.
- Deploy or restart production.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/portal/src/lib/match-filters.ts` | Created | Typed match-filter contract |
| `jobs/portal/src/lib/match-filters.test.ts` | Created | Focused regression suite |
| `jobs/portal/src/views/MatchesView.tsx` | Modified | URL-backed production filter UX |
| `jobs/portal/src/App.tsx` | Modified | Safe preview navigation |
| `web/jobs/index.html` and `web/jobs/assets/*` | Rebuilt | Deployable portal output |
| `CHANGELOG.md` | Modified | Unreleased user-visible fix |
| `docs/work/FIX-581-jobs-match-filter-state.md` | Created | Bug record |
| `docs/rounds/ROUND-582-JOBS-MATCH-FILTER-PRODUCTION-AUDIT.md` | Created | Audit evidence |

## Build & Test

```bash
npm test --prefix jobs
# 479 passed

npm run typecheck --prefix jobs
# all five Jobs TypeScript packages passed

npm run build --prefix jobs
# all five Jobs packages and portal production bundle passed

cargo test --manifest-path server/Cargo.toml
# 781 unit + 76 HTTP integration + focused integration suites passed

cargo fmt --manifest-path server/Cargo.toml -- --check
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
# passed

node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node jobs/scripts/check-provenance-licenses.mjs
node jobs/scripts/ci-guards-self-test.mjs
node scripts/check-bluey-jobs-client-boundary.mjs
node scripts/check-bluey-edge-policy.mjs
# passed
```

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| No production deployment | This is an isolated feature branch; deployment requires reviewed merge and does not remove the separate R2/runtime launch blockers |

## Known Follow-ups

- Restore R2 object-storage and backup replication health.
- Keep model generation and local/cloud Browser distribution disabled until
  their independent launch gates pass.
- Monitor bundle size as document/PDF features continue to grow; the current
  build emits one Vite chunk-size warning but completes successfully.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover acceptance criteria from plan
- [x] Code style matches repository rules
- [x] No TODOs without linked task IDs
