# IMPL: ROUND-582 - Jobs Runner Authority And Allowance Atomicity

> **Codex preflight:** Loaded `$bluey-ops` before implementation and verified
> its memory against current `origin/main` and the newest Jobs round records.

## Scope

**Does:**

- close customer-route submission and evidence-forgery paths;
- make verified runner finalization atomic and replay-safe;
- verify browser writeback before irreversible submission;
- keep packet allowance accounting correct across failure and period rollover;
- add focused unit, database, and HTTP regression coverage.

**Does NOT:**

- enable any currently disabled Jobs execution or mailbox feature flag;
- certify a new ATS provider;
- deploy or restart production services;
- modify the native Bluey overlay, audio, STT, or meeting runtime.

## Files Created / Modified

| File | Action | Purpose |
|------|--------|---------|
| `jobs/automation/src/playwright-page.ts` | Modified | Verify actual browser state after writes. |
| `jobs/automation/tests/playwright-page.test.ts` | Created | Test strict cardinality and writeback. |
| `jobs/portal/src/App.tsx` | Modified | Preserve explicit approval/queue flow. |
| `jobs/portal/src/api.ts` | Modified | Remove customer evidence mutation. |
| `server/src/api/jobs.rs` | Modified | Enforce route ownership boundaries. |
| `server/src/api/jobs_resume_generation/tests.rs` | Modified | Verify unused allowance release. |
| `server/src/db/jobs/applications.rs` | Modified | Guard submission and period-scoped metering. |
| `server/src/db/jobs/customer_data.rs` | Modified | Add transactional verified finalization. |
| `server/src/db/jobs/tests.rs` | Modified | Add authority and rollover tests. |
| `server/tests/integration_e2e.rs` | Modified | Add public-forgery regression test. |
| `CHANGELOG.md` | Modified | Record the customer-visible reliability fix. |

## Build & Test

```bash
npm test --prefix jobs
# 473 tests passed across 78 files

npm run typecheck --prefix jobs
# success

npm run build --prefix jobs
# success; Vite reports the existing portal chunk-size advisory

cargo fmt --manifest-path server/Cargo.toml --all --check
# success

cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
# success

cargo test --manifest-path server/Cargo.toml --all-targets
# all server unit and integration targets passed

node jobs/scripts/check-provenance-licenses.mjs
node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node scripts/check-bluey-jobs-client-boundary.mjs
node jobs/scripts/ci-guards-self-test.mjs
# all Jobs policy guards passed
```

The broad SQLite-boundary inventory completed with its known transitional
warning list and did not identify a new runtime SQLite dependency from this
batch.

## Deviations from Plan

| Deviation | Rationale |
|-----------|-----------|
| No production deployment | Repository rules require review and merge first; disabled production features remain disabled. |

## Known Follow-ups

- Review and merge this focused branch before building a deploy artifact.
- Run the exact production preflight, backup, canary, and rollback workflow if
  the merged change is selected for deployment.

## Review Checklist (for reviewer)

- [x] Files match the scope described above
- [x] No unrelated changes included
- [x] Tests cover the acceptance criteria
- [x] Rust and TypeScript checks pass
- [x] No unsupported product capability is enabled or claimed
