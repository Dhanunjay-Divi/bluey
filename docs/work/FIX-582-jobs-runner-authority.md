# FIX-582: Jobs Runner Authority And Allowance Atomicity

> **Codex preflight:** Loaded `$bluey-ops` before diagnosis and implementation
> and verified its Jobs authority invariants against current `origin/main`.

## Issue

Public customer routes could write application evidence and request a
`submitted` state. The browser wrapper also selected the first matching control
and did not verify that an ATS retained values after a write. Separately, a
reserved generation allowance could be reused after an entitlement-period
rollover or remain held when generation failed before provider exposure.

## Root Cause

- `server/src/api/jobs.rs` exposed evidence writes and accepted runner-owned
  application states through the customer PATCH route.
- `server/src/db/jobs/applications.rs` allowed the generic state updater to
  finalize submission after checking customer-writable evidence.
- `jobs/automation/src/playwright-page.ts` narrowed every locator to `.first()`
  and trusted successful Playwright calls without checking browser state.
- Packet commit treated every reserved generation allowance as current without
  comparing its `period_start_ms` to the active entitlement period.
- A generation that failed before managed-provider exposure did not prove the
  packet slot could be released.

## Fix Summary

- Made application evidence read-only on customer routes.
- Rejected queued/running/intervention/submitted states on the generic customer
  PATCH route; explicit packet approval remains the only customer queue path.
- Made final submission an internal runner transaction that validates the exact
  run, runner, receipt fingerprint, resume evidence, confirmation evidence,
  lease or local ticket, attempt reservation, and browser session.
- Preserved full Playwright locator cardinality and read back text, select,
  checkbox, and file-input state after every write.
- Scoped allowance reuse to the active entitlement period and covered
  pre-provider failure release with regression tests.

## Files Modified

| File | Change |
|------|--------|
| `jobs/automation/src/playwright-page.ts` | Preserve strict locators and verify ATS writeback. |
| `jobs/automation/tests/playwright-page.test.ts` | Cover ambiguity and writeback failures. |
| `jobs/portal/src/App.tsx` | Keep approval as the sole customer queue transition. |
| `jobs/portal/src/api.ts` | Remove customer evidence-write client. |
| `server/src/api/jobs.rs` | Remove evidence POST and reject runner-owned PATCH states. |
| `server/src/api/jobs_resume_generation/tests.rs` | Cover release before provider exposure. |
| `server/src/db/jobs/applications.rs` | Block generic submission and fix period-scoped metering. |
| `server/src/db/jobs/customer_data.rs` | Finalize evidence and submission in one runner transaction. |
| `server/src/db/jobs/tests.rs` | Cover runner authority and allowance rollover. |
| `server/tests/integration_e2e.rs` | Prove public routes cannot forge submission. |

## Edge Cases Handled

- Ambiguous ATS selectors fail instead of silently filling the first control.
- ATS-controlled fields that reject or normalize away a requested value block
  execution before submit.
- File inputs must retain the exact expected filenames.
- Replaying the same verified receipt is idempotent; a different receipt for an
  already-submitted application fails closed.
- Old-period generation reservations cannot suppress current-period metering.
- Customer-written resume and confirmation evidence cannot create a submitted
  application.

## How to Test

```bash
npm test --prefix jobs
npm run typecheck --prefix jobs
npm run build --prefix jobs
cargo fmt --manifest-path server/Cargo.toml --all --check
cargo clippy --manifest-path server/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path server/Cargo.toml --all-targets
node jobs/scripts/check-provenance-licenses.mjs
node jobs/scripts/privacy-gate.mjs
node jobs/scripts/check-jobs-schema-parity.mjs
node scripts/check-bluey-jobs-client-boundary.mjs
```

## Known Limitations

- This fix does not enable model generation, local Browser distribution, cloud
  Browser distribution, mailbox sync, or uncertified ATS submission.
- Production deployment requires a reviewed merge, exact artifact promotion,
  database backup, canary, and rollback evidence.
