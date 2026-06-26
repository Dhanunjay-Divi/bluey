# ROUND-202-BILLING-RISK-LEDGER-SYNC-GUARDS

Date: 2026-06-26

Branch: `codex/bluey-overlay-routing-hardening`

Backup thread id: `019e133e-d92a-7830-8df0-3a050a4e22f6`

## Why This Round

Owner asked to compare Bluey against Pinky's Square/billing dispute protections and copy the important safety posture:

- explicit paid-usage/auto-reload control
- refund/dispute should stop paid compute immediately
- auto-reload and credits need strong idempotency/evidence
- trial abuse and usage abuse must stay controlled
- the ledger must be strong enough that a dispute can be investigated later

Bluey already had several pieces from earlier rounds: Square webhook verification, processor credit idempotency, refund/dispute billing restriction, trial grants/abuse signals, Turnstile support, STT reservation, and router/embed/STT billing-restricted checks.

This round closed the concrete gaps found in code.

## Implemented

### Balance Movement Evidence Ledger

Added `balance_ledger_entries` to both:

- SQLite migrations in `server/src/db/mod.rs`
- Postgres runtime schema in `infra/postgres/server-runtime/001_server_runtime_compat.sql`

Each movement records:

- `account_id`
- `event_type`
- `amount_cents`
- `balance_cents_before`
- `balance_cents_after`
- `reason`
- `provider`
- `processor_payment_id`
- `source_id`
- `idempotency_key`
- `request_id`
- `metadata_json`
- `created_at`

This does not replace `credit_batches`. `credit_batches` remains the spendable FIFO credit source of truth. The new table is the audit/evidence ledger for dispute/debug review.

### Ledger Writes Added

Ledger entries are now written in the same transaction as the balance mutation for:

- processor payment credits
- internal credits
- paid usage deductions
- request-specific LLM deductions
- request-specific web-search-inclusive deductions
- request-specific embedding deductions
- request-specific transcribe deductions
- processor credit revocation after refund/dispute
- credit expiry sweeps
- STT reservation debits
- STT settlement refunds/extra charge movement

### Request Evidence For Router Spend

`/router/complete`, `/router/complete/stream`, `/router/embed`, and `/router/transcribe` now call the request-aware deduction wrapper so balance movement rows can be tied to the request id.

### Billing-Restricted Sync And RAG Guards

Before this round, `billing_restricted` blocked router/STT/embed paths but did not block cloud sync/RAG usage surfaces.

Now billing-restricted accounts are blocked from:

- `POST /sync/batch`
- `POST /rag/query`
- `POST /sync/artifacts/:artifact_id/object`
- `GET /sync/artifacts/:artifact_id/object`
- `POST /usage/event`

Read-only session/account/export paths are intentionally left available so a user/operator can still inspect or export data.

### Admin Billing-Risk Review API

Added admin-only:

- `GET /admin/billing-risk`

It lists billing-restricted accounts with:

- account id
- email
- balance
- restriction reason/time
- latest balance-ledger event type
- latest balance-ledger amount
- latest balance-ledger timestamp

This is not a polished UI dashboard yet, but it gives the owner/operator a review surface immediately.

## Tests Added

New coverage:

- balance ledger records processor credit, request deduction, provider/payment id, idempotency key, and request id
- processor credit revocation records actual balance delta
- STT reservation and settlement ledger entries show net reserve/refund movement
- billing-restricted account cannot use sync/RAG compute surfaces
- billing-risk admin summary includes latest balance-ledger evidence

## Verification

Ran:

```bash
cargo fmt --manifest-path server/Cargo.toml
cargo test --manifest-path server/Cargo.toml db::balance::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml db::stt_accounting::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml api::sync::tests -- --nocapture
cargo test --manifest-path server/Cargo.toml db::accounts::create_dup_tests -- --nocapture
cargo test --manifest-path server/Cargo.toml billing_ -- --nocapture
cargo test --manifest-path server/Cargo.toml router_complete_rejects_billing_restricted_account -- --nocapture
cargo test --manifest-path server/Cargo.toml
```

Result:

- full server suite passed
- `172` unit tests passed
- `41` integration tests passed
- doc-tests passed

## Remaining Follow-Ups

Still recommended before wider paid scale:

- build a polished admin abuse/dispute UI on top of `/admin/billing-risk`
- store explicit checkout/reload terms version, IP, user agent, threshold, selected amount, and consent snapshots
- add a durable auto-reload attempt table with idempotency, daily/monthly spend guard, and receipt state
- add a manual unblock/reinstate path for won disputes or benign refunds
- include `balance_ledger_entries` in account export if owner wants customer-visible evidence
- add real operator playbook for Square dashboard replay, dispute packet assembly, and support macros

