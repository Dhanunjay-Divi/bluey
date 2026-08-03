# FIX-600: Intervention Answers Require Packet Reapproval

> **Codex preflight:** Loaded `$bluey-ops` and verified its execution-authority,
> packet-checksum, receipt, and side-effect-unknown invariants against this
> checkout before implementation.

## Issue

Answering a browser intervention could move an application directly back to the
runner even though the answer changed the employer-facing application packet.
The old approval checksum and execution authority could therefore outlive the
packet that the candidate had reviewed.

## Root Cause

`resolve_intervention` in `server/src/api/jobs.rs` treated answer actions like
OTP and final-submit approvals. It resolved the intervention, moved the
application to `queued`, and signaled the existing runner without revising the
stored packet or invalidating its approval.

## Fix Summary

Answer resolution now runs in one database transaction. It stores the answer in
the application and receipt, removes the old approved-execution checksum,
records a packet revision, resolves the intervention, returns the application
to `awaiting_review`, clears its run binding, releases the active attempt, and
fences any pre-submit cloud or local runner authority. A run that may already
have clicked Submit rejects the answer change and remains in reconciliation.

OTP and explicit final-submit approvals remain the only actions that can resume
an existing runner.

## Files Modified

| File | Change |
|------|--------|
| `server/src/db/jobs/customer_data.rs` | Added the atomic answer-revision transaction for SQLite and PostgreSQL. |
| `server/src/db/jobs/applications.rs` | Allowed the guarded `needs_input` to `awaiting_review` transition. |
| `server/src/api/jobs.rs` | Split answer revision from runner-resume actions and retained Answer Memory as best-effort reuse. |
| `server/src/db/jobs/tests.rs` | Added cloud, local, idempotency-authority, and irreversible-state coverage. |
| `server/tests/integration_e2e.rs` | Added the signed-in HTTP workflow regression test. |
| `jobs/portal/src/lib/application-flow.ts` | Centralized intervention action state behavior. |
| `jobs/portal/src/App.tsx` | Kept answered browser sessions paused and surfaced reapproval. |
| `jobs/portal/src/views/ApplicationsView.tsx` | Changed the action to `Save answer for review`. |
| `jobs/portal/src/**/*.test.ts*` | Added portal workflow and copy coverage. |

## Edge Cases Handled

- Empty and overlong answers are rejected.
- Only open answer-bearing interventions can revise a packet.
- Structured answer keys and human-readable questions remain separate.
- Existing answers with the same normalized key are replaced, not duplicated.
- Prepared cloud leases are released and local tickets are failed.
- Browser sessions remain paused for packet review.
- `click_started`, `submitted`, and `side_effect_unknown` runs cannot be edited.
- A repeated or stale intervention cannot mutate the packet twice.
- Packet metering is not repeated while the application waits for reapproval.

## How to Test

```bash
cd server
cargo test intervention_answer_ -- --nocapture
cargo test --test integration_e2e \
  jobs_intervention_answer_revises_packet_without_resuming_runner -- --nocapture
cargo clippy --all-targets -- -D warnings
cargo test

cd ../jobs/portal
npm test -- --run
npm run typecheck
npm run build
```

## Known Limitations

- Saving the optional reusable Answer Memory entry happens after the safety
  transaction. If that convenience write fails, the packet revision remains
  safe and the answer can be saved to memory later from Settings.
