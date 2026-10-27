# FIX-801: PostgreSQL pre-authorization unknown-result capacity parity

> Codex preflight: `bluey-ops`, independent security/source review.

## Issue / Root Cause

The configured PostgreSQL pre-authorization uncertain-result regression reached
the real effect path after its beta fixture was corrected, then failed with
`local submission evidence capacity is missing`.

SQLite passes this legitimate claimed/needs-input case into the existing
reserve-and-retain helper. PostgreSQL instead unconditionally rejected an absent
capacity row before invoking its otherwise symmetric helper, making that
documented zero-capacity starting state impossible to finalize.

## Fix Summary / Files

Keep the exact capacity `SELECT ... FOR UPDATE` in
`server/src/db/jobs/local_runner.rs` before sampling database effect time, but
make its missing-row rejection conditional on the ticket status. Preserve the existing status-sensitive
`retain_local_unknown_capacity_postgres_tx` path:

- Reserve only for `claimed` / `needs_input`, with the existing account fence,
  exact scope/runner/bytes/objects, quotas and idempotency.
- Extend only the exact active unexpired reservation to the ticket's bounded
  expiry plus reconciliation grace.
- Do not manufacture missing capacity for `click_started` or terminal replay;
  those states still fail and roll back on missing/mismatched capacity.

There is no new submit authority, retry of an employer action, unlimited
retention window or bypass of the PostgreSQL reservation ledger.

Independent review rejected an initial complete precheck removal before any
runtime execution: removing the row lock would sample time before waiting for
an existing capacity, potentially extending an already-expired row with stale
time. The final patch retains that lock and sampling order. Only absent
`claimed`/`needs_input` capacity may proceed; absent irreversible/terminal
capacity still fails immediately.

## Verification / Limits

The existing configured PostgreSQL case starts with zero capacity, finalizes a
synthetic uncertain result, verifies exact reserved capacity/state, and replays
without duplicate accounting. Negative coverage must retain the irreversible
state's missing/mismatched-capacity denial and no-mutation behavior. Actual
commands/results are in the Phase 629 IMPL. The final configured 30/30 batch
passed the zero-capacity positive/replay case, explicit missing-capacity
click-started negative and lock-wait/expiry regression; strict Clippy passed.
No real employer/mailbox action is performed.
